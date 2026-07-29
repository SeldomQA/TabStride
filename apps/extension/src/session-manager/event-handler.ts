import type { CdpUnexpectedDetachEvent } from "@/browser-driver/chromium-cdp";
import type { Transport } from "@/transport/transport";
import type { EventFrame } from "@/transport/types";
import type { SessionManager } from "./manager";

/**
 * Listener interface that mirrors `chrome.windows.onRemoved` so vitest
 * can drive the handler without a real Chrome runtime.
 */
export interface WindowRemovedListener {
  addListener(cb: (windowId: number) => void): void;
  removeListener(cb: (windowId: number) => void): void;
}

export interface TabRemovedListener {
  addListener(cb: (tabId: number) => void): void;
  removeListener(cb: (tabId: number) => void): void;
}

export interface SessionEventHandlerOptions {
  manager: SessionManager;
  transport: Transport;
  windowEvents?: WindowRemovedListener;
  tabEvents?: TabRemovedListener;
  cdp?: {
    detachSession(sessionId: string): Promise<void>;
    onUnexpectedDetach?(handler: (event: CdpUnexpectedDetachEvent) => void): { dispose(): void };
  };
  resetAgentOverlays?: (tabId: number, sessionId: string) => Promise<void>;
  /**
   * Invoked after a user-closed Agent Window has been removed from the
   * SessionManager. Lets the caller refresh side caches such as the
   * `chrome.storage.session` "sessions live" flag (review M4/M5 I3).
   */
  onSessionsChanged?: () => void;
}

function chromeWindowEvents(): WindowRemovedListener {
  return {
    addListener: (cb) => chrome.windows.onRemoved.addListener(cb),
    removeListener: (cb) => chrome.windows.onRemoved.removeListener(cb),
  };
}

function chromeTabEvents(): TabRemovedListener {
  if (typeof chrome === "undefined" || !chrome.tabs?.onRemoved) {
    return { addListener: () => {}, removeListener: () => {} };
  }
  return {
    addListener: (cb) => chrome.tabs.onRemoved.addListener(cb),
    removeListener: (cb) => chrome.tabs.onRemoved.removeListener(cb),
  };
}

/**
 * Watch for the user closing an Agent Window. When that happens we:
 *  1. Drop the local SessionContext (without trying to close the window
 *     again — it's already gone).
 *  2. Emit a `session.window_closed` event to the daemon so it can
 *     remove the session from its registry too.
 *
 * Returns a disposer that detaches the listener (used in tests).
 */
export function attachSessionEventHandler(options: SessionEventHandlerOptions): {
  dispose: () => void;
} {
  const { manager, transport, onSessionsChanged } = options;
  const events = options.windowEvents ?? chromeWindowEvents();
  const tabEvents = options.tabEvents ?? chromeTabEvents();

  const onRemoved = (windowId: number): void => {
    const ctx = manager.findByWindowId(windowId);
    if (!ctx) return;
    const returnFailures = Array.from(ctx.borrowedTabs.keys()).map((tabId) => ({
      tab_id: tabId,
      code: "cdp_failed",
      message: "Agent Window was closed before borrowed tab could be returned",
    }));
    if (returnFailures.length > 0) {
      console.warn(
        `[bh] Agent Window ${windowId} closed with borrowed tabs that could not be returned`,
        returnFailures,
      );
    }
    const detach = options.cdp
      ? options.cdp.detachSession(ctx.sessionId).catch((err) => {
          console.debug("[bh] session-event cdp detach failed", err);
        })
      : Promise.resolve();
    void detach
      .then(() => manager.stop(ctx.sessionId, { dropOnly: true }))
      .then(() => {
        onSessionsChanged?.();
        const event: EventFrame = {
          event: "session.window_closed",
          payload: {
            session_id: ctx.sessionId,
            reason: "user_closed_window",
            ...(returnFailures.length > 0 ? { return_failures: returnFailures } : {}),
          },
        };
        try {
          transport.send(event);
        } catch (err) {
          console.warn("[bh] could not push session.window_closed event", err);
        }
      })
      .catch((err) => {
        console.warn("[bh] session-event handler failed", err);
      });
  };

  const onTabRemoved = (tabId: number): void => {
    const ctx = manager.findByAttachedTabId(tabId);
    if (!ctx) return;
    const detach = options.cdp
      ? options.cdp.detachSession(ctx.sessionId).catch((err) => {
          console.debug("[tabstride] attach-session cdp detach failed", err);
        })
      : Promise.resolve();
    void detach
      .then(() => manager.stop(ctx.sessionId, { dropOnly: true }))
      .then(() => {
        onSessionsChanged?.();
        const event: EventFrame = {
          event: "session.window_closed",
          payload: { session_id: ctx.sessionId, reason: "attached_tab_closed" },
        };
        try {
          transport.send(event);
        } catch (err) {
          console.warn("[tabstride] could not report attached tab close", err);
        }
      })
      .catch((err) => {
        console.warn("[tabstride] attach-session tab removal handler failed", err);
      });
  };

  const onUnexpectedDetach = (detachEvent: CdpUnexpectedDetachEvent): void => {
    if (detachEvent.reason !== "canceled_by_user") return;
    const sessionIds = new Set(detachEvent.sessionIds);
    const attached = manager.findByAttachedTabId(detachEvent.tabId);
    if (attached) sessionIds.add(attached.sessionId);
    const owning = manager.findByTabId(detachEvent.tabId);
    if (owning) sessionIds.add(owning.sessionId);

    for (const sessionId of sessionIds) {
      const ctx = manager.get(sessionId);
      if (!ctx) continue;

      // Send the interrupt before asynchronous cleanup. WebSocket frames are
      // ordered, so the daemon cancels the in-flight request as user_aborted
      // before session.window_closed removes the session registry entry.
      try {
        transport.send({
          event: "session.user_interrupt",
          payload: {
            session_id: sessionId,
            source: "chrome_debugger_infobar",
          },
        });
      } catch (err) {
        console.warn("[tabstride] could not report debugger user cancellation", err);
      }

      const reset =
        ctx.mode === "attach" && options.resetAgentOverlays
          ? options.resetAgentOverlays(detachEvent.tabId, sessionId).catch((err) => {
              console.debug("[tabstride] debugger cancel overlay reset failed", err);
            })
          : Promise.resolve();
      void reset
        .then(() => {
          ctx.refStore.clear();
          return manager.stop(sessionId, { dropOnly: true });
        })
        .then(() => {
          onSessionsChanged?.();
          const event: EventFrame = {
            event: "session.window_closed",
            payload: {
              session_id: sessionId,
              reason: "debugger_cancelled_by_user",
            },
          };
          try {
            transport.send(event);
          } catch (err) {
            console.warn("[tabstride] could not report debugger cancellation cleanup", err);
          }
        })
        .catch((err) => {
          console.warn("[tabstride] debugger cancellation cleanup failed", err);
        });
    }
  };

  events.addListener(onRemoved);
  tabEvents.addListener(onTabRemoved);
  const cdpDetachSubscription = options.cdp?.onUnexpectedDetach?.(onUnexpectedDetach);
  return {
    dispose: () => {
      events.removeListener(onRemoved);
      tabEvents.removeListener(onTabRemoved);
      cdpDetachSubscription?.dispose();
    },
  };
}
