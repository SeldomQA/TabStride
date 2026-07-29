// Best-effort failure diagnostics. Collection never replaces the original
// RpcError: unavailable artifacts are recorded in `collection_errors`.

import type { SessionManager } from "@/session-manager/manager";
import type {
  ConsoleEntry,
  FailureActionabilityAttempt,
  FailureEvidence,
  FailureTiming,
  Locator,
  RpcError,
} from "@/transport/types";
import { captureFailureScreenshot, captureFailureSnapshot } from "./observation";
import {
  type CdpRunner,
  type ChromeTabsApi,
  isRpcError,
  lookupSession,
  resolveTargetTab,
} from "./shared";

const SNAPSHOT_MAX_DEPTH = 16;
const SNAPSHOT_MAX_TOKENS = 4_000;
const CONSOLE_LIMIT = 20;
const CONSOLE_ERROR_LIMIT = 10;
const CONSOLE_MAX_TEXT_CHARS = 2_000;

export interface FailureEvidenceDeps {
  cdp: CdpRunner;
  tabsApi: ChromeTabsApi;
  signal?: AbortSignal;
}

export async function enrichFailureEvidence(
  manager: SessionManager,
  params: { session_id?: string; tab_id?: number; target?: Locator },
  error: RpcError,
  deps: FailureEvidenceDeps,
): Promise<RpcError> {
  if (
    deps.signal?.aborted ||
    error.code === "cancelled" ||
    error.code === "permission_denied" ||
    error.data?.evidence
  ) {
    return error;
  }
  const startedAt = performance.now();
  const collectionErrors: string[] = [];
  const locator = params.target ?? asLocator(error.data?.locator);
  const history = asActionabilityHistory(error.data?.actionability_history);
  const baseTiming = asTiming(error.data?.timing);
  const matchCount = asNonNegativeInteger(error.data?.match_count) ?? 0;
  const failedCheck =
    typeof error.data?.failed_check === "string" ? error.data.failed_check : undefined;

  const ctx = lookupSession(manager, params, "failure evidence");
  if (isRpcError(ctx)) {
    collectionErrors.push(`session: ${ctx.message}`);
    return attachEvidence(error, {
      locator,
      match_count: matchCount,
      actionability_history: history,
      last_failed_check: failedCheck,
      timing: finishTiming(baseTiming, startedAt),
      collection_errors: collectionErrors,
    });
  }
  const target = await resolveTargetTab(manager, ctx, params.tab_id, deps.tabsApi);
  if (isRpcError(target)) {
    collectionErrors.push(`tab: ${target.message}`);
    return attachEvidence(error, {
      locator,
      match_count: matchCount,
      actionability_history: history,
      last_failed_check: failedCheck,
      timing: finishTiming(baseTiming, startedAt),
      collection_errors: collectionErrors,
    });
  }

  const [urlResult, snapshotResult, screenshotResult, consoleResult] = await Promise.all([
    collectCurrentUrl(deps.cdp, target.tabId),
    captureFailureSnapshot(deps.cdp, ctx, target.tabId, {
      max_depth: SNAPSHOT_MAX_DEPTH,
      max_tokens: SNAPSHOT_MAX_TOKENS,
    }),
    captureFailureScreenshot(deps.cdp, target.tabId),
    collectConsoleErrors(deps.cdp, target.tabId),
  ]);

  const currentUrl = takeResult("url", urlResult, collectionErrors);
  const snapshot = takeResult("snapshot", snapshotResult, collectionErrors);
  const screenshot = takeResult("screenshot", screenshotResult, collectionErrors);
  const recentConsoleErrors = takeResult("console", consoleResult, collectionErrors);

  return attachEvidence(error, {
    locator,
    match_count: matchCount,
    actionability_history: history,
    last_failed_check: failedCheck,
    current_url: currentUrl,
    snapshot: snapshot
      ? {
          text: snapshot.text,
          ref_count: snapshot.ref_count,
          truncated: snapshot.truncated === true,
        }
      : undefined,
    screenshot: screenshot
      ? {
          image_base64: screenshot.image_base64,
          width: screenshot.width,
          height: screenshot.height,
          format: "png",
        }
      : undefined,
    recent_console_errors: recentConsoleErrors,
    timing: finishTiming(baseTiming, startedAt),
    collection_errors: collectionErrors,
  });
}

function attachEvidence(error: RpcError, evidence: FailureEvidence): RpcError {
  return {
    ...error,
    data: {
      ...error.data,
      evidence,
    },
  };
}

async function collectCurrentUrl(cdp: CdpRunner, tabId: number): Promise<string | RpcError> {
  try {
    const result = await cdp.send<{
      result?: { value?: unknown };
      exceptionDetails?: { text?: string };
    }>(tabId, "Runtime.evaluate", {
      expression: "location.href",
      returnByValue: true,
    });
    const value = result.result?.value;
    if (result.exceptionDetails || typeof value !== "string") {
      return {
        code: "cdp_failed",
        message: result.exceptionDetails?.text ?? "could not read current URL",
      };
    }
    return value;
  } catch (error) {
    return {
      code: "cdp_failed",
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

async function collectConsoleErrors(
  cdp: CdpRunner,
  tabId: number,
): Promise<ConsoleEntry[] | RpcError> {
  if (!cdp.ensureConsoleCapture || !cdp.consoleEntriesSince) {
    return { code: "cdp_failed", message: "console buffer is unavailable" };
  }
  try {
    await cdp.ensureConsoleCapture(tabId);
    const result = cdp.consoleEntriesSince(
      tabId,
      undefined,
      CONSOLE_LIMIT,
      CONSOLE_MAX_TEXT_CHARS,
      true,
    );
    return result.entries
      .filter(
        (entry) =>
          entry.kind === "exception" ||
          ["error", "assert"].includes(entry.level.toLocaleLowerCase()),
      )
      .slice(-CONSOLE_ERROR_LIMIT);
  } catch (error) {
    return {
      code: "cdp_failed",
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

function takeResult<T>(label: string, result: T | RpcError, errors: string[]): T | undefined {
  if (isRpcError(result)) {
    errors.push(`${label}: ${result.message}`);
    return undefined;
  }
  return result;
}

function asLocator(value: unknown): Locator | undefined {
  return value && typeof value === "object" ? (value as Locator) : undefined;
}

function asActionabilityHistory(value: unknown): FailureActionabilityAttempt[] {
  return Array.isArray(value) ? (value as FailureActionabilityAttempt[]) : [];
}

function asNonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}

function asTiming(value: unknown): Omit<FailureTiming, "evidence_ms" | "total_ms"> {
  const timing = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  return {
    locator_ms: asNonNegativeInteger(timing.locator_ms) ?? 0,
    wait_ms: asNonNegativeInteger(timing.wait_ms) ?? 0,
    cdp_ms: asNonNegativeInteger(timing.cdp_ms) ?? 0,
  };
}

function finishTiming(
  timing: Omit<FailureTiming, "evidence_ms" | "total_ms">,
  startedAt: number,
): FailureTiming {
  const evidenceMs = Math.max(0, Math.round(performance.now() - startedAt));
  return {
    ...timing,
    evidence_ms: evidenceMs,
    total_ms: timing.locator_ms + timing.wait_ms + timing.cdp_ms + evidenceMs,
  };
}
