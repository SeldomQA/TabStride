#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer as createHttpServer } from "node:http";
import { createServer as createNetServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..");
const fixtureDir = join(repoRoot, "tests/e2e/actionability");

const options = parseArgs(process.argv.slice(2));
const children = new Set();
let temporaryRoot;
let fixtureServer;
let devtools;
let sessionId;

async function main() {
  // Unix domain sockets have a small path-length limit (104 bytes on macOS).
  // Keep the disposable TABSTRIDE_HOME path deliberately short.
  const temporaryBase = process.platform === "darwin" ? "/private/tmp" : tmpdir();
  temporaryRoot = await mkdtemp(join(temporaryBase, "ts-e2e-"));
  try {
    const binary = resolve(repoRoot, options.binary);
    const extensionDir = resolve(repoRoot, options.extension);
    const wsPort = await freePort();
    const fixturePort = await freePort();
    const tabstrideHome = join(temporaryRoot, "tabstride-home");
    const profileDir = join(temporaryRoot, "chrome-profile");
    const commandEnv = {
      ...process.env,
      TABSTRIDE_HOME: tabstrideHome,
    };

    if (options.prepare) {
      checked("cargo", ["build", "--locked", "-p", "tabstride"], {
        cwd: repoRoot,
        env: commandEnv,
        stdio: "inherit",
      });
      checked("pnpm", ["--filter", "@tabstride/extension", "build"], {
        cwd: repoRoot,
        env: {
          ...commandEnv,
          TABSTRIDE_DAEMON_WS_URL: `ws://127.0.0.1:${wsPort}`,
        },
        stdio: "inherit",
      });
    }

    fixtureServer = await startFixtureServer(fixturePort);
    const fixtureUrl = `http://127.0.0.1:${fixturePort}/`;

    const serve = spawnLogged(binary, ["serve", "--port", String(wsPort)], {
      cwd: repoRoot,
      env: commandEnv,
      logPrefix: "serve",
    });
    children.add(serve);

    await waitFor(
      "TabStride serve",
      async () => {
        ensureRunning(serve, "TabStride serve");
        const result = runCli(binary, commandEnv, ["status"]);
        return result.ok;
      },
      15_000,
    );

    const chrome = findChrome();
    await mkdir(join(profileDir, "Default"), { recursive: true });
    await writeFile(
      join(profileDir, "Default", "Preferences"),
      JSON.stringify({ extensions: { ui: { developer_mode: true } } }),
    );
    const chromeArgs = [
      `--user-data-dir=${profileDir}`,
      "--remote-debugging-port=0",
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-sync",
      // Google Chrome 137+ disables unpacked command-line extensions by
      // default. Chrome for Testing/Chromium ignore this compatibility
      // override because the feature is already disabled there.
      "--disable-features=DisableLoadExtensionCommandLineSwitch,DisableDisableExtensionsExceptCommandLineSwitch",
      `--disable-extensions-except=${extensionDir}`,
      `--load-extension=${extensionDir}`,
      ...(options.headed ? [] : ["--headless=new"]),
      ...(process.platform === "linux" ? ["--no-sandbox", "--disable-dev-shm-usage"] : []),
      fixtureUrl,
    ];
    const chromeProcess = spawnLogged(chrome, chromeArgs, {
      cwd: repoRoot,
      env: commandEnv,
      logPrefix: "chrome",
    });
    children.add(chromeProcess);

    const devtoolsPort = await readDevToolsPort(profileDir);
    devtools = new DevTools(`http://127.0.0.1:${devtoolsPort}`);
    await waitFor(
      "fixture page",
      async () => (await devtools.targets()).some((target) => target.url === fixtureUrl),
      15_000,
    );
    try {
      await waitFor(
        "extension connection",
        async () => {
          ensureRunning(chromeProcess, "Chrome");
          const status = runCli(binary, commandEnv, ["status"]);
          return (
            status.ok && Array.isArray(status.json?.browsers) && status.json.browsers.length === 1
          );
        },
        20_000,
      );
    } catch (error) {
      const targets = await devtools.targets().catch(() => []);
      throw new Error(
        `${error}\nChrome output:\n${chromeProcess.capturedOutput}\nDevTools targets:\n${JSON.stringify(
          targets.map(({ type, url }) => ({ type, url })),
          null,
          2,
        )}`,
      );
    }

    const attachedTabId = await waitForExtensionTabId(fixtureUrl);
    const extensionWorker = await devtools.serviceWorker();
    const attachProbe = await extensionWorker.evaluate(
      `new Promise((resolve) => chrome.debugger
        .attach({ tabId: ${attachedTabId} }, "1.3")
        .then(() => chrome.debugger
          .detach({ tabId: ${attachedTabId} })
          .then(() => resolve({ ok: true })))
        .catch((error) => resolve({ ok: false, message: error.message })))`,
    );
    if (!attachProbe?.ok) {
      throw new Error(`chrome.debugger attach probe failed: ${JSON.stringify(attachProbe)}`);
    }
    const context = { binary, env: commandEnv, fixtureUrl, attachedTabId };

    if (options.mode === "task-baseline") {
      await runTaskBaseline(
        context,
        options.samples,
        options.taskBaseline,
        options.taskReport,
        tabstrideHome,
      );
      console.log("PASS real Chrome single-task performance baseline");
    } else {
      const start = runCli(binary, commandEnv, [
        "session",
        "start",
        "--mode",
        "attach",
        "--tab-id",
        String(attachedTabId),
      ]);
      expectOk(start, "start attach session");
      sessionId = start.json.session_id;

      if (options.mode !== "benchmark") {
        try {
          await runE2e(context);
        } catch (error) {
          throw new Error(
            `${error}\nServe output:\n${serve.capturedOutput}\nChrome output:\n${chromeProcess.capturedOutput}`,
          );
        }
      }
      if (options.mode !== "e2e") {
        await runBenchmark(context, options.samples, options.baseline, options.report);
      }

      const stop = runCli(binary, commandEnv, ["session", "stop", sessionId]);
      expectOk(stop, "stop attach session");
      sessionId = undefined;
      console.log("PASS real Chrome Actionability E2E and performance regression");
    }
  } finally {
    if (sessionId) {
      try {
        const binary = resolve(repoRoot, options.binary);
        runCli(binary, { ...process.env, TABSTRIDE_HOME: join(temporaryRoot, "tabstride-home") }, [
          "session",
          "stop",
          sessionId,
        ]);
      } catch {
        // Best-effort cleanup after a failed assertion.
      }
    }
    const runningChildren = [...children].reverse();
    for (const child of runningChildren) {
      child.kill("SIGTERM");
    }
    await Promise.all(runningChildren.map((child) => waitForChildExit(child)));
    fixtureServer?.close();
    devtools?.close();
    await rm(temporaryRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
}

async function runE2e(ctx) {
  const cli = (...args) => runCli(ctx.binary, ctx.env, args);
  const click = (testId, timeout = "3s") =>
    cli("click", "--test-id", testId, "--session", sessionId, "--timeout", timeout);
  const assertText = (testId, value, timeout = "3s") =>
    cli(
      "assert",
      "--test-id",
      testId,
      "--text-equals",
      value,
      "--session",
      sessionId,
      "--timeout",
      timeout,
    );

  console.log("E2E delayed appearance");
  expectOk(
    cli(
      "assert",
      "--test-id",
      "start-delayed",
      "--visible",
      "--session",
      sessionId,
      "--timeout",
      "3s",
    ),
    "fixture preflight",
  );
  expectOk(click("start-delayed"), "arm delayed appearance");
  expectOk(click("delayed-target"), "wait for delayed target");
  expectOk(assertText("delayed-result", "clicked"), "delayed target result");

  console.log("E2E disabled becomes enabled");
  expectOk(click("start-enabled"), "arm enabled transition");
  expectOk(click("enabled-target"), "wait for enabled target");
  expectOk(assertText("enabled-result", "clicked"), "enabled target result");

  console.log("E2E continuously moving target");
  expectOk(click("start-moving"), "arm target movement");
  expectOk(click("moving-target"), "wait for stable target");
  expectOk(assertText("moving-result", "clicked"), "moving target result");

  console.log("E2E obscured target recovers");
  expectOk(click("start-obscured"), "arm target obscurer");
  expectOk(click("obscured-target"), "wait for unobscured target");
  expectOk(assertText("obscured-result", "clicked"), "obscured target result");

  console.log("E2E DOM rebuild re-resolves locator");
  expectOk(click("start-rebuild"), "arm DOM rebuild");
  expectOk(click("rebuild-target"), "re-resolve rebuilt target");
  expectOk(assertText("rebuild-result", "clicked"), "rebuilt target result");

  console.log("E2E strict multi-match");
  const ambiguous = cli(
    "click",
    "--text",
    "Duplicate target",
    "--exact",
    "--session",
    sessionId,
    "--timeout",
    "1s",
  );
  expectErrorCode(ambiguous, "ambiguous_target", "strict multi-match");
  if (ambiguous.json?.data?.match_count !== 2) {
    throw new Error(`multi-match did not report match_count=2: ${ambiguous.output}`);
  }

  console.log("E2E timeout includes failure evidence");
  const timedOut = click("never-enabled", "400ms");
  expectErrorCode(timedOut, "timeout", "actionability timeout");
  const evidence = timedOut.json?.data?.evidence;
  if (
    timedOut.json?.data?.failed_check !== "enabled" ||
    !evidence?.snapshot ||
    !evidence?.screenshot?.image_base64 ||
    !Array.isArray(evidence?.actionability_history)
  ) {
    throw new Error(
      `timeout evidence is incomplete: ${JSON.stringify({
        failed_check: timedOut.json?.data?.failed_check,
        evidence_keys: Object.keys(evidence ?? {}),
      })}`,
    );
  }

  console.log("E2E CLI and Flow use the same actionability path");
  expectOk(click("reset-consistency"), "reset CLI consistency case");
  expectOk(click("consistency-target"), "CLI actionability execution");
  expectOk(assertText("consistency-result", "1"), "CLI consistency result");
  expectOk(click("reset-consistency"), "reset Flow consistency case");
  const flow = cli(
    "flow",
    "run",
    join(fixtureDir, "flow-consistency.yaml"),
    "--session",
    sessionId,
  );
  expectOk(flow, "Flow actionability execution");
  if (
    flow.json?.completed_steps?.[0]?.method !== "tool.click" ||
    flow.json?.completed_steps?.[1]?.method !== "tool.assert"
  ) {
    throw new Error(`Flow did not use the shared tool path: ${flow.output}`);
  }

  // Keep cross-tab creation after every positive browser action: the
  // permission assertion happens before any extension CDP dispatch.
  console.log("E2E attach tab scope isolation");
  const siblingUrl = `${ctx.fixtureUrl}?sibling=1`;
  const siblingTargetId = await devtools.createTarget(siblingUrl);
  try {
    const siblingTabId = await waitForExtensionTabId(siblingUrl);
    if (siblingTabId === ctx.attachedTabId) {
      throw new Error("scope fixture unexpectedly reused the attached tab");
    }
    const crossTab = cli(
      "click",
      "--test-id",
      "scope-target",
      "--tab-id",
      String(siblingTabId),
      "--session",
      sessionId,
      "--timeout",
      "1s",
    );
    expectErrorCode(crossTab, "permission_denied", "attach sibling tab isolation");
    if (crossTab.json?.data?.reason !== "attached_tab_scope") {
      throw new Error(`attach scope error lost its structured reason: ${crossTab.output}`);
    }
  } finally {
    await devtools.closeTarget(siblingTargetId);
  }

  // User Stop leaves the session in a deliberate pending-interrupt state, so
  // it must be the final request in the session.
  console.log("E2E user Stop interrupts an inflight action");
  const interrupted = runCliAsync(ctx.binary, ctx.env, [
    "click",
    "--test-id",
    "interrupt-target",
    "--session",
    sessionId,
    "--timeout",
    "10s",
  ]);
  const page = await devtools.page(ctx.fixtureUrl);
  await waitFor(
    "TabStride Stop overlay",
    async () =>
      Boolean(
        await page.evaluate(
          `document.querySelector("tabstride-overlay")?.shadowRoot
            ?.querySelector('[data-slot="control-overlay-stop-all"]')`,
        ),
      ),
    5_000,
  );
  await page.evaluate(
    `document.querySelector("tabstride-overlay").shadowRoot
      .querySelector('[data-slot="control-overlay-stop-all"]').click()`,
  );
  page.close();
  const interruptedResult = await interrupted;
  expectErrorCode(interruptedResult, "user_aborted", "overlay user Stop");
}

async function runBenchmark(ctx, samples, baselinePath, reportPath) {
  const cliSamples = [];
  const flowSamples = [];
  const cli = (...args) => runCli(ctx.binary, ctx.env, args);
  const warmups = 3;
  const total = samples + warmups;

  console.log(`Benchmark hot paths (${samples} samples, ${warmups} warmups)`);
  for (let index = 0; index < total; index += 1) {
    const clickStarted = performance.now();
    expectOk(
      cli("click", "--test-id", "hot-target", "--session", sessionId, "--timeout", "3s"),
      "benchmark CLI click",
    );
    const clickMs = performance.now() - clickStarted;
    if (index >= warmups) cliSamples.push(clickMs);

    const flowStarted = performance.now();
    expectOk(
      cli("flow", "run", join(fixtureDir, "flow-hot-path.yaml"), "--session", sessionId),
      "benchmark Flow click/assert",
    );
    const flowMs = performance.now() - flowStarted;
    if (index >= warmups) flowSamples.push(flowMs);
  }

  const report = {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    chrome: basename(findChrome()),
    samples,
    scenarios: {
      cli_click_hot: summarize(cliSamples),
      flow_click_assert_hot: summarize(flowSamples),
    },
  };
  const baseline = JSON.parse(await readFile(resolve(repoRoot, baselinePath), "utf8"));
  const threshold = baseline.threshold_percent ?? 15;
  if (samples < (baseline.minimum_samples ?? 1)) {
    throw new Error(
      `performance regression requires at least ${baseline.minimum_samples} samples; received ${samples}`,
    );
  }
  const regressions = [];
  for (const [name, actual] of Object.entries(report.scenarios)) {
    const expectedP95 = baseline.scenarios?.[name]?.p95_ms;
    if (typeof expectedP95 !== "number") {
      throw new Error(`performance baseline is missing ${name}.p95_ms`);
    }
    const limit = expectedP95 * (1 + threshold / 100);
    actual.baseline_p95_ms = expectedP95;
    actual.regression_limit_p95_ms = round(limit);
    if (actual.p95_ms > limit) {
      regressions.push(`${name}: p95 ${actual.p95_ms}ms > ${round(limit)}ms`);
    }
  }
  const output = `${JSON.stringify(report, null, 2)}\n`;
  console.log(output.trimEnd());
  if (reportPath) {
    const absoluteReportPath = resolve(repoRoot, reportPath);
    await mkdir(dirname(absoluteReportPath), { recursive: true });
    await writeFile(absoluteReportPath, output);
  }
  if (regressions.length > 0) {
    throw new Error(
      `P95 performance regression exceeded ${threshold}%:\n${regressions.join("\n")}`,
    );
  }
}

async function runTaskBaseline(ctx, samples, baselinePath, reportPath, tabstrideHome) {
  const scenarios = {
    attach_single_task: [],
    isolated_single_task: [],
  };
  console.log(`Benchmark complete single-task paths (${samples} samples per session mode)`);

  for (const [scenario, mode] of [
    ["attach_single_task", "attach"],
    ["isolated_single_task", "isolated"],
  ]) {
    for (let index = 0; index < samples; index += 1) {
      const runId = `baseline-${mode}-${Date.now()}-${index}`;
      const taskStarted = performance.now();
      const cli = (...args) => runCli(ctx.binary, ctx.env, ["--run-id", runId, ...args]);
      let taskSession;
      let taskTabId;
      let flowFile = join(fixtureDir, "task-baseline-attach.yaml");
      let cliProcesses = 0;
      try {
        const startArgs =
          mode === "attach"
            ? ["session", "start", "--mode", "attach", "--tab-id", String(ctx.attachedTabId)]
            : ["session", "start"];
        const start = cli(...startArgs);
        cliProcesses += 1;
        expectOk(start, `${scenario} session start`);
        taskSession = start.json.session_id;
        const attachMs = start.durationMs;

        if (mode === "isolated") {
          const create = cli("tab", "create", "--session", taskSession, "--url", ctx.fixtureUrl);
          cliProcesses += 1;
          expectOk(create, `${scenario} create fixture tab`);
          taskTabId = create.json.tab_id;
          flowFile = join(temporaryRoot, `${runId}.json`);
          await writeFile(flowFile, `${JSON.stringify(taskFlowForTab(taskTabId), null, 2)}\n`);
        }

        const snapshotTabArgs = taskTabId === undefined ? [] : ["--tab-id", String(taskTabId)];
        const initialSnapshot = cli("snapshot", "--session", taskSession, ...snapshotTabArgs);
        cliProcesses += 1;
        expectOk(initialSnapshot, `${scenario} initial snapshot`);
        const firstActionDispatchMs = performance.now() - taskStarted;

        const flow = cli("flow", "run", flowFile, "--session", taskSession);
        cliProcesses += 1;
        expectOk(flow, `${scenario} flow`);

        const finalSnapshot = cli("snapshot", "--session", taskSession, ...snapshotTabArgs);
        cliProcesses += 1;
        expectOk(finalSnapshot, `${scenario} final snapshot`);

        const stop = cli("session", "stop", taskSession);
        cliProcesses += 1;
        expectOk(stop, `${scenario} session stop`);
        taskSession = undefined;

        const taskTotalMs = performance.now() - taskStarted;
        const metrics = await metricsForRun(tabstrideHome, runId);
        const counters = sumRuntimeCounters(metrics);
        scenarios[scenario].push({
          run_id: runId,
          temperature: index === 0 ? "cold" : "warm",
          mode,
          task_total_ms: round(taskTotalMs),
          attach_ms: round(attachMs),
          first_action_dispatch_ms: round(firstActionDispatchMs),
          flow_ms: round(flow.durationMs),
          session_stop_ms: round(stop.durationMs),
          cli_process_count: cliProcesses,
          rpc_count: metrics.length,
          flow_count: 1,
          flow_step_count: flow.json?.completed_steps?.length ?? 0,
          snapshot_request_count: metrics.filter((record) => record.method === "tool.snapshot")
            .length,
          full_ax_tree_calls: counters.full_ax_tree_calls,
          cdp_calls: counters.cdp_calls,
          locator_cache_hit_rate: counterHitRate(
            counters.locator_cache_hits,
            counters.locator_cache_misses,
          ),
          snapshot_cache_hit_rate: counterHitRate(
            counters.snapshot_cache_hits,
            counters.snapshot_cache_misses,
          ),
          overlay_cache_hit_rate: counterHitRate(
            counters.overlay_cache_hits,
            counters.overlay_cache_misses,
          ),
        });
      } finally {
        if (taskSession) {
          cli("session", "stop", taskSession);
        }
      }
    }
  }

  const report = {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    chrome: basename(findChrome()),
    samples_per_scenario: samples,
    measurement: {
      first_action_dispatch:
        "elapsed from task harness start until flow.run is submitted; browser-side step timing is added in Task A-5",
      cold: "first task after Chrome and extension startup",
      warm: "subsequent tasks in the same browser process",
    },
    scenarios: Object.fromEntries(
      Object.entries(scenarios).map(([name, runs]) => [name, summarizeTaskRuns(runs)]),
    ),
    runs: scenarios,
  };

  const baseline = JSON.parse(await readFile(resolve(repoRoot, baselinePath), "utf8"));
  const threshold = baseline.threshold_percent ?? 15;
  if (samples < (baseline.minimum_samples ?? 1)) {
    throw new Error(
      `single-task regression requires at least ${baseline.minimum_samples} samples; received ${samples}`,
    );
  }
  const regressions = [];
  for (const [name, actual] of Object.entries(report.scenarios)) {
    const expectedP95 = baseline.scenarios?.[name]?.p95_ms;
    if (typeof expectedP95 !== "number") {
      throw new Error(`single-task baseline is missing ${name}.p95_ms`);
    }
    const limit = expectedP95 * (1 + threshold / 100);
    actual.baseline_p95_ms = expectedP95;
    actual.regression_limit_p95_ms = round(limit);
    if (actual.task_total_ms.p95_ms > limit) {
      regressions.push(`${name}: p95 ${actual.task_total_ms.p95_ms}ms > ${round(limit)}ms`);
    }
  }

  const output = `${JSON.stringify(report, null, 2)}\n`;
  console.log(output.trimEnd());
  const absoluteReportPath = resolve(repoRoot, reportPath);
  await mkdir(dirname(absoluteReportPath), { recursive: true });
  await writeFile(absoluteReportPath, output);
  if (regressions.length > 0) {
    throw new Error(
      `single-task P95 regression exceeded ${threshold}%:\n${regressions.join("\n")}`,
    );
  }
}

function taskFlowForTab(tabId) {
  return {
    name: "single-task-isolated-performance-baseline",
    timeout: "30s",
    steps: [
      {
        fill: {
          target: { testId: "baseline-name" },
          value: "Baseline customer",
          tab_id: tabId,
        },
      },
      {
        select: {
          target: { testId: "baseline-kind" },
          values: ["priority"],
          tab_id: tabId,
        },
      },
      {
        click: {
          target: { testId: "baseline-submit" },
          tab_id: tabId,
        },
      },
    ],
    assertions: [
      {
        target: { testId: "baseline-result" },
        text_equals: "Baseline customer:priority",
        timeout_ms: 3000,
        tab_id: tabId,
      },
    ],
  };
}

async function metricsForRun(tabstrideHome, runId) {
  const path = join(tabstrideHome, "metrics.jsonl");
  const contents = await readFile(path, "utf8").catch((error) => {
    if (error?.code === "ENOENT") return "";
    throw error;
  });
  return contents
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter((record) => record.run_id === runId);
}

function sumRuntimeCounters(records) {
  const total = {
    cdp_calls: 0,
    full_ax_tree_calls: 0,
    locator_cache_hits: 0,
    locator_cache_misses: 0,
    snapshot_cache_hits: 0,
    snapshot_cache_misses: 0,
    overlay_cache_hits: 0,
    overlay_cache_misses: 0,
  };
  for (const record of records) {
    for (const key of Object.keys(total)) {
      total[key] += record.timing?.counters?.[key] ?? 0;
    }
  }
  return total;
}

function counterHitRate(hits, misses) {
  const total = hits + misses;
  return total === 0 ? null : round(hits / total);
}

function summarizeTaskRuns(runs) {
  const warm = runs.filter((run) => run.temperature === "warm");
  const measured = warm.length > 0 ? warm : runs;
  return {
    samples: runs.length,
    warm_samples: warm.length,
    cold_run: runs[0],
    task_total_ms: summarize(measured.map((run) => run.task_total_ms)),
    attach_ms: summarize(measured.map((run) => run.attach_ms)),
    first_action_dispatch_ms: summarize(measured.map((run) => run.first_action_dispatch_ms)),
    flow_ms: summarize(measured.map((run) => run.flow_ms)),
    session_stop_ms: summarize(measured.map((run) => run.session_stop_ms)),
    cli_process_count: summarizeCount(measured.map((run) => run.cli_process_count)),
    rpc_count: summarizeCount(measured.map((run) => run.rpc_count)),
    snapshot_request_count: summarizeCount(measured.map((run) => run.snapshot_request_count)),
    full_ax_tree_calls: summarizeCount(measured.map((run) => run.full_ax_tree_calls)),
    cdp_calls: summarizeCount(measured.map((run) => run.cdp_calls)),
    snapshot_cache_hit_rate: meanOptional(measured.map((run) => run.snapshot_cache_hit_rate)),
    locator_cache_hit_rate: meanOptional(measured.map((run) => run.locator_cache_hit_rate)),
    overlay_cache_hit_rate: meanOptional(measured.map((run) => run.overlay_cache_hit_rate)),
  };
}

function meanOptional(values) {
  const present = values.filter((value) => typeof value === "number");
  if (present.length === 0) return null;
  return round(present.reduce((sum, value) => sum + value, 0) / present.length);
}

function parseArgs(args) {
  const parsed = {
    binary: "target/debug/tabstride",
    extension: "apps/extension/dist/chrome-mv3",
    baseline: "tests/e2e/actionability/performance-baseline.json",
    taskBaseline: "tests/e2e/actionability/task-performance-baseline.json",
    report: "",
    taskReport: "artifacts/single-task-performance.json",
    mode: "all",
    samples: 20,
    prepare: false,
    headed: false,
  };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--prepare") parsed.prepare = true;
    else if (arg === "--headed") parsed.headed = true;
    else if (arg === "--binary") parsed.binary = args[++index];
    else if (arg === "--extension") parsed.extension = args[++index];
    else if (arg === "--baseline") parsed.baseline = args[++index];
    else if (arg === "--task-baseline") parsed.taskBaseline = args[++index];
    else if (arg === "--report") parsed.report = args[++index];
    else if (arg === "--task-report") parsed.taskReport = args[++index];
    else if (arg === "--mode") parsed.mode = args[++index];
    else if (arg === "--samples") parsed.samples = Number(args[++index]);
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!["all", "e2e", "benchmark", "task-baseline"].includes(parsed.mode)) {
    throw new Error("--mode must be all, e2e, benchmark, or task-baseline");
  }
  if (!Number.isInteger(parsed.samples) || parsed.samples < 1) {
    throw new Error("--samples must be a positive integer");
  }
  return parsed;
}

function runCli(binary, env, args) {
  const started = performance.now();
  const result = spawnSync(binary, ["--json", ...args], {
    cwd: repoRoot,
    env,
    encoding: "utf8",
    timeout: 45_000,
  });
  const output = [result.stdout, result.stderr].filter(Boolean).join("\n").trim();
  return {
    ok: result.status === 0,
    status: result.status,
    json: parseJson(result.stdout),
    output,
    durationMs: performance.now() - started,
  };
}

function runCliAsync(binary, env, args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(binary, ["--json", ...args], { cwd: repoRoot, env });
    children.add(child);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.once("error", reject);
    child.once("close", (status) => {
      children.delete(child);
      resolvePromise({
        ok: status === 0,
        status,
        json: parseJson(stdout),
        output: [stdout, stderr].filter(Boolean).join("\n").trim(),
      });
    });
  });
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function expectOk(result, label) {
  if (!result.ok) throw new Error(`${label} failed:\n${result.output}`);
  return result.json;
}

function expectErrorCode(result, code, label) {
  if (result.ok || result.json?.code !== code) {
    throw new Error(`${label} expected ${code}:\n${result.output}`);
  }
}

function checked(command, args, options) {
  const result = spawnSync(command, args, options);
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with ${result.status}`);
  }
}

function spawnLogged(command, args, { logPrefix, ...options }) {
  const child = spawn(command, args, { ...options, stdio: ["ignore", "pipe", "pipe"] });
  child.capturedOutput = "";
  child.stdout.on("data", (chunk) => {
    child.capturedOutput += `[${logPrefix}] ${chunk}`;
  });
  child.stderr.on("data", (chunk) => {
    child.capturedOutput += `[${logPrefix}] ${chunk}`;
  });
  return child;
}

function ensureRunning(child, label) {
  if (child.exitCode !== null) {
    throw new Error(`${label} exited with ${child.exitCode}\n${child.capturedOutput}`);
  }
}

async function waitForChildExit(child) {
  if (child.exitCode !== null) return;
  await Promise.race([
    new Promise((resolvePromise) => child.once("close", resolvePromise)),
    new Promise((resolvePromise) => setTimeout(resolvePromise, 2_000)),
  ]);
  if (child.exitCode === null) child.kill("SIGKILL");
}

async function freePort() {
  const server = createNetServer();
  await new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolvePromise);
  });
  const { port } = server.address();
  await new Promise((resolvePromise) => server.close(resolvePromise));
  return port;
}

async function startFixtureServer(port) {
  const index = await readFile(join(fixtureDir, "index.html"));
  const server = createHttpServer((request, response) => {
    if (request.url === "/favicon.ico") {
      response.writeHead(204).end();
      return;
    }
    response.writeHead(200, {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
    });
    response.end(index);
  });
  await new Promise((resolvePromise, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolvePromise);
  });
  return server;
}

function findChrome() {
  if (process.env.TABSTRIDE_CHROME) return process.env.TABSTRIDE_CHROME;
  const candidates =
    process.platform === "darwin"
      ? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"]
      : ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"];
  for (const candidate of candidates) {
    if (candidate.startsWith("/") || spawnSync("which", [candidate]).status === 0) return candidate;
  }
  throw new Error("Chrome not found; set TABSTRIDE_CHROME to its executable");
}

async function readDevToolsPort(profileDir) {
  const path = join(profileDir, "DevToolsActivePort");
  return waitFor(
    "Chrome DevTools port",
    async () => {
      try {
        const [port] = (await readFile(path, "utf8")).trim().split("\n");
        return Number(port) || false;
      } catch {
        return false;
      }
    },
    15_000,
  );
}

async function waitFor(label, condition, timeoutMs) {
  const deadline = performance.now() + timeoutMs;
  let lastError;
  while (performance.now() < deadline) {
    try {
      const result = await condition();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 50));
  }
  throw new Error(`timed out waiting for ${label}${lastError ? `: ${lastError}` : ""}`);
}

class DevTools {
  constructor(baseUrl) {
    this.baseUrl = baseUrl;
    this.clients = new Set();
  }

  async targets() {
    const response = await fetch(`${this.baseUrl}/json/list`);
    if (!response.ok) throw new Error(`DevTools target list failed: ${response.status}`);
    return response.json();
  }

  async page(url) {
    const target = (await this.targets()).find(
      (candidate) => candidate.type === "page" && candidate.url === url,
    );
    if (!target) throw new Error(`page target not found: ${url}`);
    const client = new CdpClient(target.webSocketDebuggerUrl);
    await client.connect();
    this.clients.add(client);
    return client;
  }

  async serviceWorker() {
    const target = (await this.targets()).find(
      (candidate) =>
        candidate.type === "service_worker" &&
        candidate.url.startsWith("chrome-extension://") &&
        candidate.url.endsWith("/background.js"),
    );
    if (!target) throw new Error("TabStride extension service worker target not found");
    const client = new CdpClient(target.webSocketDebuggerUrl);
    await client.connect();
    this.clients.add(client);
    return client;
  }

  async browser() {
    const response = await fetch(`${this.baseUrl}/json/version`);
    if (!response.ok) throw new Error(`DevTools browser endpoint failed: ${response.status}`);
    const version = await response.json();
    const client = new CdpClient(version.webSocketDebuggerUrl);
    await client.connect();
    this.clients.add(client);
    return client;
  }

  async createTarget(url) {
    const client = await this.browser();
    const result = await client.send("Target.createTarget", { url });
    return result.targetId;
  }

  async closeTarget(targetId) {
    const client = await this.browser();
    await client.send("Target.closeTarget", { targetId });
  }

  close() {
    for (const client of this.clients) client.close();
  }
}

class CdpClient {
  constructor(url) {
    this.url = url;
    this.nextId = 1;
    this.pending = new Map();
  }

  async connect() {
    this.socket = new WebSocket(this.url);
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
    await new Promise((resolvePromise, reject) => {
      this.socket.addEventListener("open", resolvePromise, { once: true });
      this.socket.addEventListener("error", reject, { once: true });
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolvePromise, reject) => {
      this.pending.set(id, { resolve: resolvePromise, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const response = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (response.exceptionDetails) {
      throw new Error(response.exceptionDetails.text);
    }
    return response.result?.value;
  }

  close() {
    this.socket?.close();
  }
}

async function waitForExtensionTabId(url) {
  return waitFor(
    `extension tab id for ${url}`,
    async () => {
      const worker = await devtools.serviceWorker();
      const tabs = await worker.evaluate(
        `new Promise((resolve) => chrome.tabs.query({}, (tabs) =>
          resolve(tabs.map((tab) => ({ id: tab.id, url: tab.url })))))`,
      );
      return tabs?.find((tab) => tab.url === url)?.id ?? false;
    },
    5_000,
  );
}

function summarize(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    samples: sorted.length,
    p50_ms: round(percentile(sorted, 0.5)),
    p95_ms: round(percentile(sorted, 0.95)),
    p99_ms: round(percentile(sorted, 0.99)),
    min_ms: round(sorted[0]),
    max_ms: round(sorted.at(-1)),
  };
}

function summarizeCount(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    samples: sorted.length,
    p50: round(percentile(sorted, 0.5)),
    p95: round(percentile(sorted, 0.95)),
    p99: round(percentile(sorted, 0.99)),
    min: round(sorted[0]),
    max: round(sorted.at(-1)),
  };
}

function percentile(sorted, quantile) {
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)];
}

function round(value) {
  return Math.round(value * 1000) / 1000;
}

if (typeof WebSocket === "undefined") {
  const relaunched = spawnSync(
    process.execPath,
    ["--experimental-websocket", ...process.argv.slice(1)],
    {
      cwd: process.cwd(),
      env: process.env,
      stdio: "inherit",
    },
  );
  process.exitCode = relaunched.status ?? 1;
} else {
  await main();
}
