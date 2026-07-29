// Shared Actionability Engine v1 for click/fill/press/select.
//
// Locator resolution remains strict and independent: matching errors fail
// immediately, while a uniquely matched node is re-resolved as actionability
// state changes until the action deadline.
// Every successful attempt returns the freshly resolved backend node so the
// caller never acts on a node captured before an actionability wait.

import type { SessionContext } from "@/session-manager/manager";
import type { Locator, RpcError } from "@/transport/types";
import { rpcError } from "./errors";
import { scrollNodeIntoView } from "./element-geometry";
import { type ResolvedLocator, resolveLocator } from "./locator";
import { type CdpRunner, isRpcError } from "./shared";

export type ActionabilityAction = "click" | "fill" | "press" | "select";

export type ActionabilityCheck =
  | "attached"
  | "visible"
  | "stable"
  | "enabled"
  | "editable"
  | "receives_events"
  | "not_obscured"
  | "focusable"
  | "select";

export interface ActionabilityRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ActionabilityState {
  attached: boolean;
  visible: boolean;
  stable: boolean;
  enabled: boolean;
  editable: boolean;
  receives_events: boolean;
  not_obscured: boolean;
  focusable: boolean;
  select: boolean;
  rect?: ActionabilityRect;
  obscured_by?: string;
}

export interface ActionableTarget extends ResolvedLocator {
  actionability: ActionabilityState;
  elapsedMs: number;
}

export interface ActionabilityOptions {
  timeoutMs: number;
  signal?: AbortSignal;
  pollIntervalMs?: number;
}

interface InspectedState extends Omit<ActionabilityState, "stable"> {}

const DEFAULT_POLL_INTERVAL_MS = 50;

const REQUIRED_CHECKS: Record<ActionabilityAction, readonly ActionabilityCheck[]> = {
  click: ["attached", "visible", "stable", "enabled", "receives_events", "not_obscured"],
  fill: ["attached", "visible", "enabled", "editable"],
  press: ["attached", "visible", "focusable"],
  select: ["attached", "visible", "enabled", "select"],
};

const TERMINAL_CHECKS = new Set<ActionabilityCheck>(["editable", "focusable", "select"]);

export async function waitForActionable(
  cdp: CdpRunner,
  ctx: SessionContext,
  tabId: number,
  locator: Locator | undefined,
  action: ActionabilityAction,
  options: ActionabilityOptions,
): Promise<ActionableTarget | RpcError> {
  const startedAt = performance.now();
  const timeoutMs = Math.max(1, options.timeoutMs);
  const deadline = startedAt + timeoutMs;
  const pollIntervalMs = Math.max(1, options.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS);
  let previousRect: ActionabilityRect | undefined;
  let lastScrolledNodeId: number | undefined;
  let lastState: ActionabilityState | undefined;
  let lastFailedCheck: ActionabilityCheck = "attached";

  while (true) {
    if (options.signal?.aborted) {
      return { code: "cancelled", message: `${action} actionability wait aborted` };
    }

    const resolved = await resolveLocator(cdp, ctx, tabId, locator);
    if (isRpcError(resolved)) {
      return resolved;
    } else {
      // Scrolling is part of preparing an actionable element. Do it once per
      // freshly resolved backend node so a detached node cannot flood logs
      // during the wait loop.
      if (lastScrolledNodeId !== resolved.backendNodeId) {
        await scrollNodeIntoView(cdp, tabId, resolved.backendNodeId);
        lastScrolledNodeId = resolved.backendNodeId;
      }
      const inspected = await inspectActionability(cdp, tabId, resolved.backendNodeId);
      if (isRpcError(inspected)) {
        lastState = unavailableState();
        lastFailedCheck = "attached";
      } else {
        const stable = previousRect !== undefined && rectsEqual(previousRect, inspected.rect);
        lastState = { ...inspected, stable };
        previousRect = inspected.rect;
        const failed = firstFailedCheck(action, lastState);
        if (!failed) {
          return {
            ...resolved,
            actionability: lastState,
            elapsedMs: elapsed(startedAt),
          };
        }
        lastFailedCheck = failed;
        if (TERMINAL_CHECKS.has(failed)) {
          return actionabilityError(action, failed, startedAt, lastState, false);
        }
      }
    }

    const remainingMs = deadline - performance.now();
    if (remainingMs <= 0) {
      return actionabilityError(
        action,
        lastFailedCheck,
        startedAt,
        lastState ?? unavailableState(),
        true,
      );
    }
    await abortableDelay(Math.min(pollIntervalMs, remainingMs), options.signal);
  }
}

function firstFailedCheck(
  action: ActionabilityAction,
  state: ActionabilityState,
): ActionabilityCheck | null {
  for (const check of REQUIRED_CHECKS[action]) {
    if (!state[check]) return check;
  }
  return null;
}

async function inspectActionability(
  cdp: CdpRunner,
  tabId: number,
  backendNodeId: number,
): Promise<InspectedState | RpcError> {
  try {
    const resolved = await cdp.send<{ object?: { objectId?: string } }>(tabId, "DOM.resolveNode", {
      backendNodeId,
    });
    const objectId = resolved.object?.objectId;
    if (!objectId) return { code: "cdp_failed", message: "DOM.resolveNode returned no objectId" };

    const result = await cdp.send<{ result?: { value?: InspectedState } }>(
      tabId,
      "Runtime.callFunctionOn",
      {
        objectId,
        functionDeclaration: `function __tabstrideActionabilityV1() {
          const attached = this instanceof Element &&
            this.isConnected &&
            this.ownerDocument &&
            this.ownerDocument.documentElement.contains(this);
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
          const select = !!(attached && this instanceof HTMLSelectElement);
          const naturallyFocusable = !!(attached && (
            this.matches('input, textarea, select, button, a[href], area[href], iframe, object, embed') ||
            this.isContentEditable
          ));
          const focusable = !!(visible && enabled && (naturallyFocusable || this.tabIndex >= 0));
          let receivesEvents = false;
          let obscuredBy;
          if (visible && rect) {
            const x = Math.max(0, Math.min(innerWidth - 1, rect.left + rect.width / 2));
            const y = Math.max(0, Math.min(innerHeight - 1, rect.top + rect.height / 2));
            const hits = this.ownerDocument.elementsFromPoint(x, y)
              .filter((node) => !node.matches('[data-tabstride-overlay], [data-tabstride-internal]'));
            const hit = hits[0];
            receivesEvents = !!hit && (hit === this || this.contains(hit));
            if (!receivesEvents && hit) {
              obscuredBy = hit.id ? '#' + hit.id :
                hit.classList.length ? hit.tagName.toLowerCase() + '.' + Array.from(hit.classList).join('.') :
                hit.tagName.toLowerCase();
            }
          }
          return {
            attached,
            visible,
            enabled,
            editable,
            receives_events: receivesEvents,
            not_obscured: receivesEvents,
            focusable,
            select,
            rect: rect ? {
              x: rect.x,
              y: rect.y,
              width: rect.width,
              height: rect.height
            } : undefined,
            obscured_by: obscuredBy
          };
        }`,
        returnByValue: true,
      },
    );
    const state = result.result?.value;
    if (!state || typeof state.attached !== "boolean") {
      return { code: "cdp_failed", message: "actionability inspection returned no state" };
    }
    return state;
  } catch (error) {
    return {
      code: "cdp_failed",
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

function actionabilityError(
  action: ActionabilityAction,
  failedCheck: ActionabilityCheck,
  startedAt: number,
  state: ActionabilityState,
  timedOut: boolean,
): RpcError {
  const reason = reasonForCheck(failedCheck);
  return rpcError(
    timedOut ? "timeout" : "invalid_params",
    reason,
    `${action} target failed actionability check "${failedCheck}"${timedOut ? " before timeout" : ""}`,
    {
      failed_check: failedCheck,
      elapsed_ms: elapsed(startedAt),
      last_state: state,
    },
  );
}

function reasonForCheck(check: ActionabilityCheck) {
  switch (check) {
    case "attached":
      return "element_detached" as const;
    case "visible":
      return "element_not_visible" as const;
    case "stable":
      return "element_not_stable" as const;
    case "enabled":
      return "element_disabled" as const;
    case "editable":
      return "element_not_editable" as const;
    case "receives_events":
    case "not_obscured":
      return "element_obscured" as const;
    case "focusable":
      return "element_not_focusable" as const;
    case "select":
      return "target_not_select" as const;
  }
}

function unavailableState(): ActionabilityState {
  return {
    attached: false,
    visible: false,
    stable: false,
    enabled: false,
    editable: false,
    receives_events: false,
    not_obscured: false,
    focusable: false,
    select: false,
  };
}

function rectsEqual(left: ActionabilityRect, right: ActionabilityRect | undefined): boolean {
  if (!right) return false;
  const epsilon = 0.25;
  return (
    Math.abs(left.x - right.x) <= epsilon &&
    Math.abs(left.y - right.y) <= epsilon &&
    Math.abs(left.width - right.width) <= epsilon &&
    Math.abs(left.height - right.height) <= epsilon
  );
}

function elapsed(startedAt: number): number {
  return Math.max(0, Math.round(performance.now() - startedAt));
}

function abortableDelay(ms: number, signal: AbortSignal | undefined): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(done, ms);
    signal?.addEventListener("abort", done, { once: true });
    function done() {
      clearTimeout(timer);
      signal?.removeEventListener("abort", done);
      resolve();
    }
  });
}

export const __testing__ = {
  REQUIRED_CHECKS,
  firstFailedCheck,
  rectsEqual,
};
