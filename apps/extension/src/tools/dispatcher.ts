import { operationSessionId, summarizeOperation } from "@/lib/operation-log";
import type { OverlayOperationLogEntry } from "@/lib/overlay-bridge";
import { OVERLAY_AUTOMATION_BYPASS } from "@/lib/overlay-bridge";
import type { SessionManager } from "@/session-manager/manager";
import type { Transport } from "@/transport/transport";
import type {
  AssertParams,
  ClickParams,
  ConsoleParams,
  EvaluateParams,
  FillParams,
  GetHtmlParams,
  NavigateBackParams,
  NavigateForwardParams,
  NavigateParams,
  PressParams,
  ProtocolFrame,
  ReloadParams,
  RequestFrame,
  RequestHelpParams,
  ResponseFrame,
  RpcError,
  ScreenshotParams,
  SelectParams,
  SnapshotParams,
  TimingTrace,
  WaitForNavigationParams,
} from "@/transport/types";
import { isRequestFrame } from "@/transport/types";
import { handleAssert } from "./assertion";
import { handleConsole } from "./console";
import { handleEvaluate } from "./evaluate";
import { enrichFailureEvidence } from "./failure-evidence";
import { defaultWatchTabNavigation, handleRequestHelp } from "./human-loop";
import { handleClick, handleFill, handlePress, handleSelect } from "./interaction";
import {
  handleNavigate,
  handleNavigateBack,
  handleNavigateForward,
  handleReload,
} from "./navigation";
import {
  type CdpRunner,
  chromeTabsCaptureApi,
  handleGetHtml,
  handleScreenshot,
  handleSnapshot,
} from "./observation";
import {
  handleSessionStart,
  handleSessionStop,
  type SessionStartParams,
  type SessionStopParams,
} from "./session";
import { chromeTabsApi } from "./shared";
import {
  type BorrowConfirmationApprover,
  handleTabBorrow,
  handleTabClose,
  handleTabCreate,
  handleTabList,
  handleTabReturn,
  handleTabSelect,
  type TabBorrowParams,
  type TabCloseParams,
  type TabCreateParams,
  type TabListParams,
  type TabReturnParams,
  type TabSelectParams,
} from "./tabs";
import { handleWaitForNavigation } from "./waits";

export interface DispatcherDeps {
  transport: Transport;
  sessions: SessionManager;
  cdp?: CdpRunner & {
    detachSession(sessionId: string): Promise<void>;
  };
  /**
   * Invoked whenever a dispatched RPC may have changed the live
   * session set (currently `tool.session_start` and
   * `tool.session_stop`). Used to refresh side caches such as the
   * `chrome.storage.session` "sessions live" flag (review M4/M5 I3).
   */
  onSessionsChanged?: () => void;
  /** User approval for `tool.tab_borrow` (overlay in content script). */
  approveBorrow?: BorrowConfirmationApprover;
  /** i18n notification copy for `tool.request_help` (resolved per-call). */
  helpNotificationCopy?: () => { title: string; body: string };
  /** Publishes privacy-safe AI operation state to the in-page log panel. */
  onOperationLog?: (entry: OverlayOperationLogEntry) => void;
}

/**
 * Routes RPC requests pushed by the daemon over the Transport to the
 * appropriate tool implementation.
 *
 * M5 wires `tool.session_start` and `tool.session_stop`. M6+ tools
 * will register additional method handlers here.
 *
 * M10.2 wires the cancel chain: every dispatched RPC owns one
 * `AbortController` keyed by its wire `id` in
 * [`inflightAbortControllers`]. When the daemon pushes a `cancel`
 * request the dispatcher trips the matching controller; tool
 * handlers that already accept a `signal` (waits, navigation,
 * interaction, evaluate, tabs) react in line, and the dispatcher
 * additionally races the in-flight invocation against the abort
 * promise so handlers without explicit signal plumbing still respond
 * promptly with `cancelled`.
 */
export class ToolDispatcher {
  private readonly transport: Transport;
  private readonly sessions: SessionManager;
  private readonly cdp?: CdpRunner & {
    detachSession(sessionId: string): Promise<void>;
  };
  private readonly onSessionsChanged?: () => void;
  private readonly approveBorrow?: BorrowConfirmationApprover;
  private readonly helpNotificationCopy?: () => { title: string; body: string };
  private readonly onOperationLog?: (entry: OverlayOperationLogEntry) => void;
  private subscription: { dispose(): void } | null = null;
  /**
   * Per-rpc-id `AbortController` registry. Populated inside
   * [`dispatch`] before we await the tool handler and torn down in
   * the matching `finally` so failures + send errors never leak
   * controllers. Made public for tests.
   */
  readonly inflightAbortControllers = new Map<string, AbortController>();

  constructor(deps: DispatcherDeps) {
    this.transport = deps.transport;
    this.sessions = deps.sessions;
    this.cdp = deps.cdp;
    this.onSessionsChanged = deps.onSessionsChanged;
    this.approveBorrow = deps.approveBorrow;
    this.helpNotificationCopy = deps.helpNotificationCopy;
    this.onOperationLog = deps.onOperationLog;
  }

  start(): void {
    if (this.subscription) return;
    this.subscription = this.transport.onMessage((msg) => {
      void this.dispatch(msg);
    });
  }

  stop(): void {
    this.subscription?.dispose();
    this.subscription = null;
    // Trip every outstanding controller so dependent waits unblock
    // before the dispatcher is GC'd.
    for (const ac of this.inflightAbortControllers.values()) {
      try {
        ac.abort();
      } catch (_) {
        // ignore
      }
    }
    this.inflightAbortControllers.clear();
  }

  private async dispatch(msg: ProtocolFrame): Promise<void> {
    if (!isRequestFrame(msg)) return;
    const req = msg as RequestFrame;

    // Cancel frames take a fast path: trip the matching controller
    // (if any), reply with `{cancelled}` so the daemon can answer
    // its own peer, and skip the regular tool dispatch.
    if (req.method === "cancel") {
      const params = (req.params as { rpc_id?: string } | undefined) ?? {};
      const target = typeof params.rpc_id === "string" ? params.rpc_id : "";
      const ac = target ? this.inflightAbortControllers.get(target) : undefined;
      if (ac) {
        try {
          ac.abort();
        } catch (err) {
          console.warn("[tabstride dispatcher] AbortController.abort() threw", err);
        }
      }
      const reply: ResponseFrame = {
        id: req.id,
        result: { cancelled: ac !== undefined },
      };
      try {
        this.transport.send(reply);
      } catch (sendErr) {
        console.warn("[tabstride dispatcher] failed to ack cancel", sendErr);
      }
      return;
    }

    const mutatesSessions =
      req.method === "tool.session_start" || req.method === "tool.session_stop";
    const timing = requestTiming(req);
    if (timing) timing.extension_received_at = epochMicroseconds();
    const requestCdp = this.cdp && timing ? timedCdp(this.cdp, timing) : this.cdp;
    const operationStartedAtMs = Date.now();
    const operationSession = operationSessionId(req.params);
    const operationDetail = summarizeOperation(req.method, req.params);
    if (operationSession && req.method.startsWith("tool.")) {
      this.emitOperationLog({
        id: req.id,
        sessionId: operationSession,
        method: req.method,
        status: "running",
        ...(operationDetail ? { detail: operationDetail } : {}),
        startedAtMs: operationStartedAtMs,
      });
    }
    const ac = new AbortController();
    this.inflightAbortControllers.set(req.id, ac);
    let body: ResponseFrame;
    let startedSession: string | null = null;
    try {
      const result = await Promise.race([
        this.invoke(req, ac.signal, requestCdp),
        abortPromise(ac.signal),
      ]);
      if (isRpcError(result)) {
        body = { id: req.id, error: result };
      } else {
        body = { id: req.id, result };
        if (req.method === "tool.session_start") {
          startedSession = (req.params as SessionStartParams | undefined)?.session_id ?? null;
        }
      }
    } catch (err) {
      if (isAbortLikeError(err)) {
        body = {
          id: req.id,
          error: { code: "cancelled", message: "rpc aborted by daemon cancel" },
        };
      } else {
        body = {
          id: req.id,
          error: {
            code: "protocol_error",
            message: err instanceof Error ? err.message : String(err),
          },
        };
      }
    } finally {
      this.inflightAbortControllers.delete(req.id);
    }
    if (timing) {
      timing.extension_replied_at = epochMicroseconds();
      attachTiming(body, timing);
    }
    if (operationSession && req.method.startsWith("tool.")) {
      const error = "error" in body ? body.error : undefined;
      this.emitOperationLog({
        id: req.id,
        sessionId: operationSession,
        method: req.method,
        status: error ? "failed" : "succeeded",
        ...(operationDetail ? { detail: operationDetail } : {}),
        ...(error ? { errorCode: error.code } : {}),
        startedAtMs: operationStartedAtMs,
        durationMs: Math.max(0, Date.now() - operationStartedAtMs),
      });
    }
    let sent = true;
    try {
      this.transport.send(body);
    } catch (sendErr) {
      sent = false;
      // Transport is dead by the time we want to reply. Drop the link
      // proactively so the alarm-driven keepalive reconnects sooner
      // and the daemon's pending RPC times out cleanly instead of
      // waiting for the full 15s budget (review M4/M5 I9).
      console.warn("[tabstride dispatcher] failed to send response; dropping transport", sendErr);
      void this.transport.disconnect().catch((e) => {
        console.debug("[tabstride dispatcher] disconnect after send failure errored", e);
      });
    }
    if (!sent && startedSession) {
      // The daemon never observed the session id we just allocated, so
      // its `start_session` reservation will be cancelled. Roll back
      // the Agent Window + SessionContext here so we do not leak an
      // orphan window the user has to close manually (review M4/M5
      // round 3 I-R3-3).
      try {
        const ctx = await this.sessions.stop(startedSession);
        if (ctx) {
          console.warn(
            "[tabstride dispatcher] rolled back orphan session after send failure",
            startedSession,
          );
        }
      } catch (rollbackErr) {
        console.warn(
          "[tabstride dispatcher] session rollback after send failure failed",
          rollbackErr,
        );
      }
    }
    if (mutatesSessions) this.onSessionsChanged?.();
  }

  private emitOperationLog(entry: OverlayOperationLogEntry): void {
    try {
      this.onOperationLog?.(entry);
    } catch (err) {
      console.debug("[tabstride dispatcher] operation log observer failed", err);
    }
  }

  private async invoke(
    req: RequestFrame,
    signal: AbortSignal,
    cdp = this.cdp,
  ): Promise<unknown | RpcError> {
    switch (req.method) {
      case "tool.session_start":
        return handleSessionStart(this.sessions, req.params as SessionStartParams);
      case "tool.session_stop":
        return handleSessionStop(this.sessions, req.params as SessionStopParams, {
          cdp,
        });
      case "tool.tab_list":
        return handleTabList(this.sessions, req.params as TabListParams);
      case "tool.tab_create":
        return handleTabCreate(this.sessions, req.params as TabCreateParams);
      case "tool.tab_close":
        return handleTabClose(this.sessions, req.params as TabCloseParams);
      case "tool.tab_select":
        return handleTabSelect(this.sessions, req.params as TabSelectParams);
      case "tool.tab_borrow":
        return handleTabBorrow(this.sessions, req.params as TabBorrowParams, {
          signal,
          approveBorrow: this.approveBorrow,
        });
      case "tool.tab_return":
        return handleTabReturn(this.sessions, req.params as TabReturnParams);
      case "tool.screenshot":
        return handleScreenshot(
          this.sessions,
          req.params as ScreenshotParams,
          cdp
            ? { cdp, tabsApi: chromeTabsCaptureApi, captureApi: chromeTabsCaptureApi }
            : undefined,
        );
      case "tool.console":
        return handleConsole(
          this.sessions,
          req.params as ConsoleParams,
          cdp ? { cdp, tabsApi: chromeTabsApi } : undefined,
        );
      case "tool.snapshot":
        return handleSnapshot(
          this.sessions,
          req.params as SnapshotParams,
          cdp ? { cdp, tabsApi: chromeTabsCaptureApi } : undefined,
        );
      case "tool.get_html":
        return handleGetHtml(
          this.sessions,
          req.params as GetHtmlParams,
          cdp ? { cdp, tabsApi: chromeTabsCaptureApi } : undefined,
        );
      case "tool.navigate":
        return handleNavigate(
          this.sessions,
          req.params as NavigateParams,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
      case "tool.navigate_back":
        return handleNavigateBack(
          this.sessions,
          req.params as NavigateBackParams,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
      case "tool.navigate_forward":
        return handleNavigateForward(
          this.sessions,
          req.params as NavigateForwardParams,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
      case "tool.reload":
        return handleReload(
          this.sessions,
          req.params as ReloadParams,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
      case "tool.click": {
        const params = req.params as ClickParams;
        const result = await handleClick(
          this.sessions,
          params,
          cdp
            ? {
                cdp,
                tabsApi: chromeTabsApi,
                signal,
                bypassOverlay: async (tabId, enabled) => {
                  try {
                    await chrome.tabs.sendMessage(tabId, {
                      type: OVERLAY_AUTOMATION_BYPASS,
                      enabled,
                    });
                  } catch {
                    // Content script may be unavailable on restricted pages.
                  }
                },
              }
            : undefined,
        );
        return this.withFailureEvidence(params, result, signal, cdp);
      }
      case "tool.fill": {
        const params = req.params as FillParams;
        const result = await handleFill(
          this.sessions,
          params,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
        return this.withFailureEvidence(params, result, signal, cdp);
      }
      case "tool.press": {
        const params = req.params as PressParams;
        const result = await handlePress(
          this.sessions,
          params,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
        return this.withFailureEvidence(params, result, signal, cdp);
      }
      case "tool.select": {
        const params = req.params as SelectParams;
        const result = await handleSelect(
          this.sessions,
          params,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
        return this.withFailureEvidence(params, result, signal, cdp);
      }
      case "tool.assert": {
        const params = req.params as AssertParams;
        const result = await handleAssert(
          this.sessions,
          params,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
        return this.withFailureEvidence(params, result, signal, cdp);
      }
      case "tool.evaluate":
        return handleEvaluate(
          this.sessions,
          req.params as EvaluateParams,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
      case "tool.wait_for_navigation":
        return handleWaitForNavigation(
          this.sessions,
          req.params as WaitForNavigationParams,
          cdp ? { cdp, tabsApi: chromeTabsApi, signal } : undefined,
        );
      case "tool.request_help":
        return handleRequestHelp(this.sessions, req.params as RequestHelpParams, {
          tabsApi: chromeTabsApi,
          windows: { update: (id, info) => chrome.windows.update(id, info) },
          activateTab: async (tabId) => {
            await chrome.tabs.update(tabId, { active: true });
          },
          sendToTab: (tabId, msg) => chrome.tabs.sendMessage(tabId, msg),
          watchTabNavigation: defaultWatchTabNavigation,
          ...(cdp ? { cdp } : {}),
          notifications: makeHelpNotifications(),
          notificationCopy: this.helpNotificationCopy?.(),
          signal,
        });
      default:
        return {
          code: "unknown_method",
          message: `${req.method} not implemented in extension`,
        } satisfies RpcError;
    }
  }

  private async withFailureEvidence<T>(
    params: { session_id?: string; tab_id?: number; target?: ClickParams["target"] },
    result: T | RpcError,
    signal: AbortSignal,
    cdp = this.cdp,
  ): Promise<T | RpcError> {
    if (!cdp || !isRpcError(result)) return result;
    return enrichFailureEvidence(this.sessions, params, result, {
      cdp,
      tabsApi: chromeTabsApi,
      signal,
    });
  }
}

const TIMING_FIELD = "__tabstride_timing";

function requestTiming(req: RequestFrame): TimingTrace | null {
  if (req.timing && typeof req.timing === "object") return req.timing;
  const params = req.params;
  if (!params || typeof params !== "object") return null;
  const timing = (params as Record<string, unknown>)[TIMING_FIELD];
  return timing && typeof timing === "object" ? (timing as TimingTrace) : null;
}

function attachTiming(frame: ResponseFrame, timing: TimingTrace): void {
  if ("result" in frame && frame.result && typeof frame.result === "object") {
    (frame.result as Record<string, unknown>)[TIMING_FIELD] = timing;
    return;
  }
  if ("error" in frame) {
    frame.error.data = { ...(frame.error.data ?? {}), [TIMING_FIELD]: timing };
  }
}

function epochMicroseconds(): number {
  if (typeof performance !== "undefined" && Number.isFinite(performance.timeOrigin)) {
    return Math.round((performance.timeOrigin + performance.now()) * 1000);
  }
  return Date.now() * 1000;
}

function timedCdp<T extends CdpRunner>(cdp: T, timing: TimingTrace): T {
  timing.counters ??= {};
  return new Proxy(cdp, {
    get(target, property, receiver) {
      if (property === "runtimeCounters") return timing.counters;
      if (property === "send") {
        return async (...args: Parameters<CdpRunner["send"]>) => {
          timing.counters!.cdp_calls = (timing.counters!.cdp_calls ?? 0) + 1;
          if (args[1] === "Accessibility.getFullAXTree") {
            timing.counters!.full_ax_tree_calls = (timing.counters!.full_ax_tree_calls ?? 0) + 1;
          }
          timing.cdp_started_at ??= epochMicroseconds();
          try {
            return await target.send(...args);
          } finally {
            timing.cdp_finished_at = epochMicroseconds();
          }
        };
      }
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

function isRpcError(v: unknown): v is RpcError {
  return (
    typeof v === "object" &&
    v !== null &&
    "code" in v &&
    "message" in v &&
    typeof (v as RpcError).code === "string"
  );
}

/**
 * Resolves never; rejects with `AbortLikeError` as soon as the signal
 * fires (or immediately if it is already aborted). Used by the
 * dispatcher to race the tool invocation so handlers without explicit
 * signal plumbing still surface a `cancelled` reply promptly.
 */
function abortPromise(signal: AbortSignal): Promise<never> {
  return new Promise<never>((_, reject) => {
    if (signal.aborted) {
      reject(new AbortLikeError());
      return;
    }
    signal.addEventListener(
      "abort",
      () => {
        reject(new AbortLikeError());
      },
      { once: true },
    );
  });
}

/**
 * Sentinel error class so [`isAbortLikeError`] can recognise our own
 * race-rejection without confusing it with a real CDP failure.
 */
class AbortLikeError extends Error {
  constructor() {
    super("rpc aborted by daemon cancel");
    this.name = "BhAbortError";
  }
}

function isAbortLikeError(err: unknown): boolean {
  if (err instanceof AbortLikeError) return true;
  if (err instanceof DOMException && err.name === "AbortError") return true;
  if (typeof err === "object" && err !== null && (err as { name?: string }).name === "AbortError") {
    return true;
  }
  return false;
}

function makeHelpNotifications() {
  if (typeof chrome.notifications?.create !== "function") return null;
  return {
    create: (id: string, opts: chrome.notifications.NotificationOptions<true>) =>
      new Promise<string>((resolve, reject) =>
        chrome.notifications.create(id, opts, (rid) => {
          const err = chrome.runtime?.lastError;
          if (err) reject(new Error(err.message ?? String(err)));
          else resolve(rid ?? id);
        }),
      ),
    clear: (id: string) =>
      new Promise<boolean>((resolve) => chrome.notifications.clear(id, (c) => resolve(c ?? false))),
  };
}
