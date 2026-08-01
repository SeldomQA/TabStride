import type { SessionManager } from "@/session-manager/manager";
import type { RpcError } from "@/transport/types";
import {
  type AgentOverlayResetApi,
  chromeAgentOverlayResetApi,
  returnBorrowedTab,
  type TabManagementDeps,
} from "./tabs";
import {
  type CdpRunner,
  captureFailureSnapshot,
} from "./observation";
import { readDocumentIdentity } from "@/session-manager/document-cache";

export interface SessionStartParams {
  session_id: string;
  browser_instance_id?: string;
  mode?: "isolated" | "attach";
  tab?: "active";
  tab_id?: number;
  /** Request an initial accessibility snapshot + page metadata alongside
   *  the session creation reply (A-2: merged attach+snapshot). */
  snapshot?: boolean;
}

export interface SessionStartResult {
  agent_window_id?: number;
  attached_tab_id?: number;
  /** Tab URL at session creation time (populated when snapshot requested). */
  url?: string;
  /** Tab title at session creation time (populated when snapshot requested). */
  title?: string;
  /** CDP document version at session creation time. */
  document_version?: number;
  /** Indented aria-snapshot text captured during session creation. */
  snapshot_text?: string;
  /** Number of @e<N> refs registered by the initial snapshot. */
  snapshot_ref_count?: number;
  /** Whether the initial snapshot was truncated. */
  snapshot_truncated?: boolean;
}

export interface SessionStartDeps {
  tabs?: Pick<typeof chrome.tabs, "get">;
  windows?: Pick<typeof chrome.windows, "getLastFocused" | "getAll">;
  /** CDP runner required when `snapshot` is requested. */
  cdp?: CdpRunner;
}

async function resolveAttachTab(
  manager: SessionManager,
  params: SessionStartParams,
  deps: SessionStartDeps,
): Promise<chrome.tabs.Tab | RpcError> {
  const tabsApi = deps.tabs ?? chrome.tabs;
  const windowsApi = deps.windows ?? chrome.windows;
  if (params.tab_id !== undefined) {
    if (!Number.isSafeInteger(params.tab_id) || params.tab_id <= 0) {
      return { code: "invalid_params", message: "tab_id must be a positive integer" };
    }
    try {
      return await tabsApi.get(params.tab_id);
    } catch (err) {
      return {
        code: "not_found",
        message: err instanceof Error ? err.message : `tab ${params.tab_id} not found`,
      };
    }
  }
  if (params.tab !== "active") {
    return {
      code: "invalid_params",
      message: "attach mode requires tab=active or tab_id",
    };
  }
  const candidates: chrome.windows.Window[] = [];
  const seen = new Set<number>();
  try {
    const last = await windowsApi.getLastFocused({ populate: true, windowTypes: ["normal"] });
    if (typeof last.id === "number") {
      seen.add(last.id);
      candidates.push(last);
    }
  } catch (err) {
    console.debug("[tabstride attach] getLastFocused failed", err);
  }
  try {
    const all = await windowsApi.getAll({ populate: true, windowTypes: ["normal"] });
    for (const win of all) {
      if (typeof win.id !== "number" || seen.has(win.id)) continue;
      candidates.push(win);
    }
  } catch (err) {
    console.debug("[tabstride attach] getAll failed", err);
  }
  for (const win of candidates) {
    if (typeof win.id !== "number" || manager.findByWindowId(win.id)) continue;
    const active = win.tabs?.find((tab) => tab.active === true && typeof tab.id === "number");
    if (active) return active;
  }
  return { code: "not_found", message: "no active user tab available to attach" };
}

export interface SessionStopParams {
  session_id: string;
}

export interface SessionStopResult {
  /** Tab ids that were returned to their original (or fallback) window. */
  returned_tab_ids?: number[];
  /** Tab ids whose return path failed; those entries remain borrowed so
   *  shutdown can be retried without closing the Agent Window. */
  return_failures?: Array<{ tab_id: number; code: string; message: string }>;
}

export interface SessionStopDeps {
  cdp?: {
    detachSession(sessionId: string): Promise<void>;
  };
  /** Clears the control overlay from an attached user tab during stop. */
  agentOverlayReset?: AgentOverlayResetApi;
  /**
   * Tab management deps forwarded to `returnBorrowedTab`. Defaults to
   * the production `chrome.tabs` / `chrome.windows` wrappers, but
   * tests can inject fakes so the auto-return path is unit-tested
   * without a real browser.
   */
  tabManagement?: TabManagementDeps;
}

/**
 * Handler for `tool.session_start` (called by the daemon over WS).
 *
 * Creates an isolated Agent Window or leases one existing tab in place,
 * then registers a fresh SessionContext. When `params.snapshot` is set
 * the handler also captures an initial accessibility snapshot so the
 * agent can skip a separate `tool.snapshot` round-trip (A-2).
 */
export async function handleSessionStart(
  manager: SessionManager,
  params: SessionStartParams,
  deps: SessionStartDeps = {},
): Promise<SessionStartResult | RpcError> {
  if (!params?.session_id) {
    return {
      code: "invalid_params",
      message: "session.start requires session_id",
    };
  }
  const mode = params.mode ?? "isolated";
  if (mode !== "isolated" && mode !== "attach") {
    return { code: "invalid_params", message: "session mode must be isolated or attach" };
  }
  if (mode === "isolated" && (params.tab !== undefined || params.tab_id !== undefined)) {
    return { code: "invalid_params", message: "tab selectors require attach mode" };
  }
  if (mode === "attach" && params.tab !== undefined && params.tab_id !== undefined) {
    return { code: "invalid_params", message: "tab and tab_id are mutually exclusive" };
  }
  if (mode === "attach" && params.tab === undefined && params.tab_id === undefined) {
    return {
      code: "invalid_params",
      message: "attach mode requires tab=active or tab_id",
    };
  }
  try {
    if (mode === "attach") {
      const tab = await resolveAttachTab(manager, params, deps);
      if ("code" in tab) return tab;
      if (typeof tab.id !== "number" || typeof tab.windowId !== "number") {
        return { code: "not_found", message: "attach target has no tab/window id" };
      }
      const leasedBy = manager.findBorrowingSession(tab.id, params.session_id);
      if (leasedBy) {
        return {
          code: "permission_denied",
          message: `tab ${tab.id} is already controlled by session ${leasedBy}`,
          data: { reason: "borrow_conflict" },
        };
      }
      if (manager.findByWindowId(tab.windowId)) {
        return {
          code: "permission_denied",
          message: `tab ${tab.id} belongs to an Agent Window`,
          data: { reason: "agent_window_scope" },
        };
      }
      const ctx = manager.startAttached(params.session_id, tab.id, tab.windowId);
      const snapshot = await captureInitialSnapshot(manager, ctx, tab.id, tab.url, tab.title, params, deps);
      if ("code" in snapshot) {
        return { ...snapshot, attached_tab_id: ctx.attachedTabId };
      }
      return { attached_tab_id: ctx.attachedTabId, ...snapshot };
    }
    const ctx = await manager.start(params.session_id);
    const snapshot = await captureInitialSnapshot(manager, ctx, ctx.agentWindowId, undefined, undefined, params, deps);
    if ("code" in snapshot) {
      // Snapshot failure is non-fatal for isolated sessions (page may
      // still be loading about:blank). Return the session without
      // snapshot data so the agent can proceed and take a snapshot
      // once the page has loaded.
      return { agent_window_id: ctx.agentWindowId };
    }
    return { agent_window_id: ctx.agentWindowId, ...snapshot };
  } catch (err) {
    return {
      code: "protocol_error",
      message: err instanceof Error ? err.message : String(err),
    };
  }
}

/**
 * Capture initial page metadata + accessibility snapshot for a freshly
 * created session. Returns empty fields when `params.snapshot` is
 * `false` or CDP is unavailable (the caller should omit snapshot fields
 * when no snapshot was requested).
 */
async function captureInitialSnapshot(
  manager: SessionManager,
  ctx: { sessionId: string },
  targetId: number,
  fallbackUrl: string | undefined,
  fallbackTitle: string | undefined,
  params: SessionStartParams,
  deps: SessionStartDeps,
): Promise<Partial<SessionStartResult> | RpcError> {
  if (!params.snapshot) {
    return {};
  }
  if (!deps.cdp) {
    return { code: "cdp_failed", message: "snapshot requested but CDP is unavailable" };
  }
  const cdp = deps.cdp;
  const tabsApi = deps.tabs ?? chrome.tabs;

  // Resolve url / title. For attach mode the caller provides them;
  // for isolated mode we read from Chrome.
  let url = fallbackUrl ?? "";
  let title = fallbackTitle ?? "";
  let tabId = targetId;
  if (!fallbackUrl) {
    try {
      const tab = await tabsApi.get(targetId);
      url = tab.url ?? url;
      title = tab.title ?? title;
      if (typeof tab.id === "number") tabId = tab.id;
    } catch {
      // Non-fatal: proceed without url/title.
    }
  }

  // Resolve document version (best-effort).
  let documentVersion: number | undefined;
  try {
    const docId = await readDocumentIdentity(cdp, tabId);
    if (docId) documentVersion = docId.version;
  } catch {
    // Non-fatal.
  }

  // Capture the accessibility snapshot.
  const ctxFull = manager.get(ctx.sessionId);
  if (!ctxFull) {
    return {
      url: url || undefined,
      title: title || undefined,
      document_version: documentVersion,
    };
  }

  try {
    const snapshot = await captureFailureSnapshot(cdp, ctxFull, tabId, { max_depth: 16 });
    if ("code" in snapshot) {
      // Snapshot capture failed; return metadata without snapshot text
      // so the agent can retry with a dedicated `tool.snapshot` call.
      console.warn("[tabstride session_start] initial snapshot capture failed", snapshot);
      return {
        url: url || undefined,
        title: title || undefined,
        document_version: documentVersion,
      };
    }
    return {
      url: url || undefined,
      title: title || undefined,
      document_version: documentVersion,
      snapshot_text: snapshot.text || undefined,
      snapshot_ref_count: snapshot.ref_count,
      snapshot_truncated: snapshot.truncated,
    };
  } catch (err) {
    console.warn("[tabstride session_start] initial snapshot capture threw", err);
    return {
      url: url || undefined,
      title: title || undefined,
      document_version: documentVersion,
    };
  }
}

/**
 * Handler for `tool.session_stop` (called by the daemon over WS).
 *
 * Teardown order is intentional and must not be reordered without
 * thinking through the consequences (review M8.4):
 *
 *   1. Return every borrowed tab to its original (or fallback)
 *      window. We MUST do this before closing the Agent Window —
 *      otherwise the borrowed tab gets removed along with the window.
 *   2. Clear the RefStore so any pending `@e1` resolver bails out
 *      cleanly (review parity with M6).
 *   3. Clear the control overlay from an attached user tab.
 *   4. Detach CDP sessions the extension still holds for this
 *      session (no-op if M6/M7 didn't attach to any tab).
 *   5. Stop the SessionContext. Isolated mode closes its Agent Window;
 *      attach mode only releases the existing-tab lease.
 *
 * Failures in step 1 keep the Agent Window open: a failed borrowed tab
 * may still be there, so closing the window would risk losing user
 * state. The daemon/CLI surface the failure and keep the session
 * retryable.
 */
export async function handleSessionStop(
  manager: SessionManager,
  params: SessionStopParams,
  deps: SessionStopDeps = {},
): Promise<SessionStopResult | RpcError> {
  if (!params?.session_id) {
    return {
      code: "invalid_params",
      message: "session.stop requires session_id",
    };
  }
  const ctx = manager.get(params.session_id);
  if (!ctx) {
    return {
      code: "not_found",
      message: `session ${params.session_id} unknown`,
    };
  }

  // Step 1: auto-return borrowed tabs. Iterate over a snapshot of the
  // ids so deletions during iteration do not break the Map iterator.
  const returnedTabIds: number[] = [];
  const returnFailures: SessionStopResult["return_failures"] = [];
  const borrowedIds = Array.from(ctx.borrowedTabs.keys());
  for (const tabId of borrowedIds) {
    try {
      const tabManagement = {
        ...(deps.tabManagement ?? {}),
        isAgentWindowId:
          deps.tabManagement?.isAgentWindowId ??
          ((windowId: number) => manager.findByWindowId(windowId) !== null),
      };
      const outcome = await returnBorrowedTab(ctx, tabId, tabManagement);
      if (typeof outcome === "object" && "code" in outcome) {
        console.warn(`[tabstride session_stop] auto-return failed for tab ${tabId}`, outcome);
        returnFailures?.push({
          tab_id: tabId,
          code: outcome.code,
          message: outcome.message,
        });
      } else {
        returnedTabIds.push(outcome.tabId);
        ctx.borrowedTabs.delete(tabId);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      console.warn(`[tabstride session_stop] auto-return threw for tab ${tabId}: ${message}`);
      returnFailures?.push({
        tab_id: tabId,
        code: "protocol_error",
        message,
      });
    }
  }

  const result: SessionStopResult = {};
  if (returnedTabIds.length > 0) result.returned_tab_ids = returnedTabIds;
  if (returnFailures && returnFailures.length > 0) {
    result.return_failures = returnFailures;
    // A failed return means at least one borrowed user tab may still be
    // inside the Agent Window. Keep the session/window alive so the user
    // can retry `tabstride session stop` or explicitly `tabstride tab return` after the
    // underlying Chrome issue is resolved.
    return result;
  }

  // Step 2: clear the per-session RefStore (review M6/M7 parity).
  ctx.refStore.clear();

  // Step 3: attach mode keeps the user's tab open, so explicitly retract
  // the content-script control overlay before releasing the lease. This is
  // best-effort because restricted pages may not have a content script.
  if (ctx.mode === "attach" && ctx.attachedTabId !== undefined) {
    const overlayReset = deps.agentOverlayReset ?? chromeAgentOverlayResetApi;
    try {
      await overlayReset.resetAgentOverlays(ctx.attachedTabId, ctx.sessionId);
    } catch (err) {
      console.debug("[tabstride session_stop] attached-tab overlay reset failed", err);
    }
  }

  // Step 4: detach CDP sessions this session opened (no-op if none).
  await deps.cdp?.detachSession(params.session_id);

  // Step 5: close the Agent Window or release the attach lease, then drop
  // the context.
  await manager.stop(params.session_id);

  return result;
}
