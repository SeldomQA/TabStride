// Web-first Assertions v1. Every failed attempt waits for DOM/CDP activity
// (with a short geometry/history fallback), then re-resolves the original
// Locator so DOM replacement never leaves us holding a stale backendNodeId.

import type { SessionManager } from "@/session-manager/manager";
import type {
  AssertionSpec,
  AssertParams,
  AssertResult,
  FailureTiming,
  Locator,
  RpcError,
} from "@/transport/types";
import { type AutoWaitWakeReason, waitForPageChange } from "./auto-wait";
import { rpcError } from "./errors";
import { countLocatorMatches, resolveLocator, validateLocator } from "./locator";
import {
  type CdpRunner,
  type ChromeTabsApi,
  enforceAgentWindow,
  isRpcError,
  lookupSession,
  resolveTargetTab,
} from "./shared";

const DEFAULT_TIMEOUT_MS = 30_000;
const POLL_INTERVAL_MS = 50;
const PREDICATES = [
  "visible",
  "hidden",
  "attached",
  "detached",
  "text_equals",
  "text_contains",
  "value_equals",
  "enabled",
  "disabled",
  "editable",
  "checked",
  "unchecked",
  "count",
  "populated",
  "url_equals",
  "url_matches",
] as const;

type AssertionName = (typeof PREDICATES)[number];

interface ElementState {
  attached: boolean;
  visible: boolean;
  text: string;
  value: string | null;
  enabled: boolean;
  editable: boolean;
  checked: boolean | null;
}

interface Attempt {
  passed: boolean;
  actual: unknown;
  expected: unknown;
  matchCount: number;
  usedTarget?: Locator;
  retryError?: RpcError;
}

interface TimingAccumulator {
  locatorMs: number;
  waitMs: number;
  cdpMs: number;
}

export interface AssertionDeps {
  cdp: CdpRunner;
  tabsApi: ChromeTabsApi;
  signal?: AbortSignal;
  /** Test seam; production uses MutationObserver and CDP page events. */
  waitForChange?: (remainingMs: number) => Promise<AutoWaitWakeReason>;
}

export async function handleAssert(
  manager: SessionManager,
  params: AssertParams,
  deps?: AssertionDeps,
): Promise<AssertResult | RpcError> {
  if (!deps) {
    return { code: "unsupported", message: "assert requires the CDP browser driver" };
  }
  const invalid = validateAssertion(params);
  if (invalid) return invalid;

  const ctx = lookupSession(manager, params, "assert");
  if (isRpcError(ctx)) return ctx;
  const target = await resolveTargetTab(manager, ctx, params.tab_id, deps.tabsApi);
  if (isRpcError(target)) return target;
  const scopeError = enforceAgentWindow(ctx, target, "assert");
  if (scopeError) return scopeError;

  const name = PREDICATES.find((key) => params[key] !== undefined) as AssertionName;
  const timeoutMs = params.timeout_ms ?? DEFAULT_TIMEOUT_MS;
  const startedAt = performance.now();
  const deadline = startedAt + timeoutMs;
  let lastAttempt: Attempt = {
    passed: false,
    actual: null,
    expected: params[name],
    matchCount: 0,
  };
  const timing: TimingAccumulator = { locatorMs: 0, waitMs: 0, cdpMs: 0 };

  while (true) {
    if (deps.signal?.aborted) {
      return { code: "cancelled", message: "assertion wait aborted" };
    }

    lastAttempt = await evaluateAttempt(deps.cdp, ctx, target.tabId, params, name, timing);
    if (lastAttempt.retryError && !isRetryable(lastAttempt.retryError)) {
      return withAssertionDiagnostics(lastAttempt.retryError, params, lastAttempt, timing);
    }
    if (lastAttempt.passed) {
      return {
        tab_id: target.tabId,
        assertion: name,
        passed: true,
        elapsed_ms: elapsed(startedAt),
        expected: lastAttempt.expected,
        actual: lastAttempt.actual,
        match_count: lastAttempt.matchCount,
        ...(lastAttempt.usedTarget ? { used_target: lastAttempt.usedTarget } : {}),
      };
    }

    const remainingMs = deadline - performance.now();
    if (remainingMs <= 0) {
      return assertionTimeout(name, params, startedAt, lastAttempt, timing);
    }
    const waitMs = Math.min(POLL_INTERVAL_MS, remainingMs);
    const waitStartedAt = performance.now();
    const wakeReason = deps.waitForChange
      ? await deps.waitForChange(waitMs)
      : await waitForPageChange(deps.cdp, target.tabId, {
          maxWaitMs: waitMs,
          fallbackMs: POLL_INTERVAL_MS,
          signal: deps.signal,
        });
    timing.waitMs += performance.now() - waitStartedAt;
    if (wakeReason === "cancelled" || deps.signal?.aborted) {
      return { code: "cancelled", message: "assertion wait aborted" };
    }
  }
}

function validateAssertion(params: AssertParams): RpcError | null {
  if (!params || typeof params !== "object") {
    return { code: "invalid_params", message: "assert requires params" };
  }
  const selected = PREDICATES.filter((key) => params[key] !== undefined);
  if (selected.length !== 1) {
    return {
      code: "invalid_params",
      message: "assertion must specify exactly one expectation",
    };
  }
  if (
    params.timeout_ms !== undefined &&
    (!Number.isSafeInteger(params.timeout_ms) || params.timeout_ms <= 0)
  ) {
    return { code: "invalid_params", message: "timeout_ms must be a positive integer" };
  }
  const name = selected[0];
  const isUrl = name === "url_equals" || name === "url_matches";
  if (isUrl) {
    if (params.target !== undefined) {
      return { code: "invalid_params", message: "URL assertions do not accept a target" };
    }
    if (typeof params[name] !== "string") {
      return { code: "invalid_params", message: `${name} must be a string` };
    }
    if (name === "url_matches") {
      try {
        new RegExp(params.url_matches as string);
      } catch (error) {
        return {
          code: "invalid_params",
          message: `invalid URL regular expression: ${
            error instanceof Error ? error.message : String(error)
          }`,
        };
      }
    }
    return null;
  }
  const locatorError = validateLocator(params.target);
  if (locatorError) return locatorError;
  if (name === "count") {
    if (!Number.isSafeInteger(params.count) || (params.count ?? -1) < 0) {
      return { code: "invalid_params", message: "count must be a non-negative integer" };
    }
  } else if (["text_equals", "text_contains", "value_equals"].includes(name)) {
    if (typeof params[name] !== "string") {
      return { code: "invalid_params", message: `${name} must be a string` };
    }
  } else if (typeof params[name] !== "boolean") {
    return { code: "invalid_params", message: `${name} must be a boolean` };
  }
  return null;
}

async function evaluateAttempt(
  cdp: CdpRunner,
  ctx: Parameters<typeof resolveLocator>[1],
  tabId: number,
  params: AssertionSpec,
  name: AssertionName,
  timing: TimingAccumulator,
): Promise<Attempt> {
  if (name === "url_equals" || name === "url_matches") {
    const cdpStartedAt = performance.now();
    const current = await currentUrl(cdp, tabId);
    timing.cdpMs += performance.now() - cdpStartedAt;
    const expected = params[name] as string;
    if (isRpcError(current)) {
      return { passed: false, actual: null, expected, matchCount: 0, retryError: current };
    }
    return {
      passed: name === "url_equals" ? current === expected : new RegExp(expected).test(current),
      actual: current,
      expected,
      matchCount: 0,
    };
  }

  if (name === "count" || name === "detached") {
    const locatorStartedAt = performance.now();
    const counted = await countLocatorMatches(cdp, ctx, tabId, params.target);
    timing.locatorMs += performance.now() - locatorStartedAt;
    const expected = name === "count" ? (params.count as number) : (params.detached as boolean);
    if (isRpcError(counted)) {
      return { passed: false, actual: null, expected, matchCount: 0, retryError: counted };
    }
    return {
      passed:
        name === "count"
          ? counted.matchCount === expected
          : (counted.matchCount === 0) === expected,
      actual: name === "count" ? counted.matchCount : counted.matchCount === 0,
      expected,
      matchCount: counted.matchCount,
      usedTarget: counted.usedTarget,
    };
  }

  const locatorStartedAt = performance.now();
  const resolved = await resolveLocator(cdp, ctx, tabId, params.target);
  timing.locatorMs += performance.now() - locatorStartedAt;
  if (isRpcError(resolved)) {
    const expected = params[name];
    if (resolved.code === "not_found" && name === "hidden") {
      const actual = true;
      return {
        passed: actual === expected,
        actual,
        expected,
        matchCount: 0,
        usedTarget: params.target,
      };
    }
    return {
      passed: false,
      actual: null,
      expected,
      matchCount: Number(resolved.data?.match_count ?? 0),
      usedTarget: params.target,
      retryError: resolved,
    };
  }

  const cdpStartedAt = performance.now();
  const state = await inspectElement(cdp, tabId, resolved.backendNodeId);
  timing.cdpMs += performance.now() - cdpStartedAt;
  if (isRpcError(state)) {
    return {
      passed: false,
      actual: null,
      expected: params[name],
      matchCount: 1,
      usedTarget: resolved.usedTarget,
      retryError: state,
    };
  }
  const expected = params[name];
  let actual: unknown;
  switch (name) {
    case "attached":
      actual = state.attached;
      break;
    case "visible":
      actual = state.visible;
      break;
    case "hidden":
      actual = !state.visible;
      break;
    case "text_equals":
      actual = state.text;
      return result(state.text === normaliseText(expected), actual, expected, resolved.usedTarget);
    case "text_contains":
      actual = state.text;
      return result(
        state.text.includes(normaliseText(expected)),
        actual,
        expected,
        resolved.usedTarget,
      );
    case "value_equals":
      actual = state.value;
      break;
    case "enabled":
      actual = state.enabled;
      break;
    case "disabled":
      actual = !state.enabled;
      break;
    case "editable":
      actual = state.editable;
      break;
    case "checked":
      actual = state.checked;
      break;
    case "unchecked":
      actual = state.checked === null ? null : !state.checked;
      break;
    case "populated":
      actual = state.value !== null && state.value.length > 0;
      break;
    default:
      actual = null;
  }
  return result(actual === expected, actual, expected, resolved.usedTarget);
}

function result(passed: boolean, actual: unknown, expected: unknown, usedTarget: Locator): Attempt {
  return { passed, actual, expected, matchCount: 1, usedTarget };
}

async function currentUrl(cdp: CdpRunner, tabId: number): Promise<string | RpcError> {
  try {
    const response = await cdp.send<{
      result?: { value?: unknown };
      exceptionDetails?: { text?: string };
    }>(tabId, "Runtime.evaluate", {
      expression: "location.href",
      returnByValue: true,
    });
    if (response.exceptionDetails || typeof response.result?.value !== "string") {
      return {
        code: "cdp_failed",
        message: response.exceptionDetails?.text ?? "could not read the current URL",
      };
    }
    return response.result.value;
  } catch (error) {
    return { code: "cdp_failed", message: error instanceof Error ? error.message : String(error) };
  }
}

async function inspectElement(
  cdp: CdpRunner,
  tabId: number,
  backendNodeId: number,
): Promise<ElementState | RpcError> {
  try {
    const resolved = await cdp.send<{ object?: { objectId?: string } }>(tabId, "DOM.resolveNode", {
      backendNodeId,
    });
    const objectId = resolved.object?.objectId;
    if (!objectId) return { code: "cdp_failed", message: "DOM.resolveNode returned no objectId" };
    const response = await cdp.send<{ result?: { value?: ElementState } }>(
      tabId,
      "Runtime.callFunctionOn",
      {
        objectId,
        functionDeclaration: `function __tabstrideAssertionStateV1() {
          const attached = this instanceof Element && this.isConnected &&
            this.ownerDocument?.documentElement.contains(this);
          const rect = attached ? this.getBoundingClientRect() : null;
          const style = attached ? this.ownerDocument.defaultView.getComputedStyle(this) : null;
          const visible = !!(attached && rect && rect.width > 0 && rect.height > 0 && style &&
            style.display !== 'none' && style.visibility !== 'hidden' &&
            style.visibility !== 'collapse' && Number(style.opacity) > 0);
          const ariaDisabled = attached && this.getAttribute('aria-disabled') === 'true';
          const nativeDisabled = attached && 'disabled' in this && this.disabled === true;
          const disabledFieldset = attached && this.closest('fieldset:disabled') !== null;
          const enabled = !!(attached && !ariaDisabled && !nativeDisabled && !disabledFieldset);
          const readOnly = attached && 'readOnly' in this && this.readOnly === true;
          const editable = !!(enabled && !readOnly && (
            this instanceof HTMLInputElement ||
            this instanceof HTMLTextAreaElement ||
            this.isContentEditable
          ));
          const text = String(this.innerText ?? this.textContent ?? '').replace(/\\s+/g, ' ').trim();
          const value = attached && 'value' in this ? String(this.value) : null;
          let checked = null;
          if (attached && 'checked' in this && typeof this.checked === 'boolean') {
            checked = this.checked;
          } else if (attached && this.getAttribute('aria-checked') !== null) {
            checked = this.getAttribute('aria-checked') === 'true';
          }
          return { attached, visible, text, value, enabled, editable, checked };
        }`,
        returnByValue: true,
      },
    );
    const value = response.result?.value;
    if (!value) return { code: "cdp_failed", message: "element assertion inspection failed" };
    return value;
  } catch (error) {
    return { code: "cdp_failed", message: error instanceof Error ? error.message : String(error) };
  }
}

function normaliseText(value: unknown): string {
  return String(value ?? "")
    .replace(/\s+/g, " ")
    .trim();
}

function isRetryable(error: RpcError): boolean {
  return error.code === "not_found" || error.code === "cdp_failed";
}

function assertionTimeout(
  name: AssertionName,
  params: AssertionSpec,
  startedAt: number,
  attempt: Attempt,
  timing: TimingAccumulator,
): RpcError {
  const elapsedMs = elapsed(startedAt);
  return rpcError(
    "timeout",
    "assertion_failed",
    `assertion ${name} did not pass within ${elapsedMs}ms`,
    {
      assertion: name,
      expected: attempt.expected,
      actual: attempt.actual,
      elapsed_ms: elapsedMs,
      match_count: attempt.matchCount,
      locator: params.target,
      timing: serialiseTiming(timing),
      last_error: attempt.retryError,
    },
  );
}

function withAssertionDiagnostics(
  error: RpcError,
  params: AssertionSpec,
  attempt: Attempt,
  timing: TimingAccumulator,
): RpcError {
  return {
    ...error,
    data: {
      ...error.data,
      locator: params.target,
      match_count: attempt.matchCount,
      timing: serialiseTiming(timing),
    },
  };
}

function serialiseTiming(
  timing: TimingAccumulator,
): Omit<FailureTiming, "evidence_ms" | "total_ms"> {
  return {
    locator_ms: Math.max(0, Math.round(timing.locatorMs)),
    wait_ms: Math.max(0, Math.round(timing.waitMs)),
    cdp_ms: Math.max(0, Math.round(timing.cdpMs)),
  };
}

function elapsed(startedAt: number): number {
  return Math.max(0, Math.round(performance.now() - startedAt));
}
