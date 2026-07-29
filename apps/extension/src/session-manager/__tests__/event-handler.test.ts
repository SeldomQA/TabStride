import { describe, expect, it, vi } from "vitest";
import type { CdpUnexpectedDetachEvent } from "@/browser-driver/chromium-cdp";
import type { ConnectionStateHandler, FrameHandler, Transport } from "@/transport/transport";
import type { ConnectionState, ProtocolFrame } from "@/transport/types";
import type { TabRemovedListener } from "../event-handler";
import { attachSessionEventHandler, type WindowRemovedListener } from "../event-handler";
import { SessionManager } from "../manager";

function fakeWindowEvents() {
  const listeners = new Set<(windowId: number) => void>();
  const api: WindowRemovedListener = {
    addListener: (cb) => listeners.add(cb),
    removeListener: (cb) => listeners.delete(cb),
  };
  return {
    api,
    emit(id: number) {
      for (const l of listeners) l(id);
    },
    listenerCount: () => listeners.size,
  };
}

function fakeTabEvents() {
  const listeners = new Set<(tabId: number) => void>();
  const api: TabRemovedListener = {
    addListener: (cb) => listeners.add(cb),
    removeListener: (cb) => listeners.delete(cb),
  };
  return {
    api,
    emit(id: number) {
      for (const listener of listeners) listener(id);
    },
  };
}

function fakeTransport(): Transport & { sent: ProtocolFrame[] } {
  const sent: ProtocolFrame[] = [];
  const t: Transport & { sent: ProtocolFrame[] } = {
    state: "connected" as ConnectionState,
    sent,
    connect: () => Promise.resolve(),
    disconnect: () => Promise.resolve(),
    send: (msg) => sent.push(msg),
    onMessage: (_h: FrameHandler) => ({ dispose: () => {} }),
    onConnectionStateChange: (_h: ConnectionStateHandler) => ({ dispose: () => {} }),
  };
  return t;
}

function fakeCdp() {
  const listeners = new Set<(event: CdpUnexpectedDetachEvent) => void>();
  return {
    detachSession: vi.fn(async () => {}),
    onUnexpectedDetach: vi.fn((listener: (event: CdpUnexpectedDetachEvent) => void) => {
      listeners.add(listener);
      return { dispose: () => listeners.delete(listener) };
    }),
    emit(event: CdpUnexpectedDetachEvent) {
      for (const listener of listeners) listener(event);
    },
    listenerCount: () => listeners.size,
  };
}

describe("attachSessionEventHandler", () => {
  it("drops the local session and emits session.window_closed when the agent window closes", async () => {
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 4242),
        remove: vi.fn(async () => {}),
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    await manager.start("aa11");
    const transport = fakeTransport();
    const events = fakeWindowEvents();
    const cdp = { detachSession: vi.fn(async () => {}) };

    attachSessionEventHandler({
      manager,
      transport,
      windowEvents: events.api,
      cdp,
    });
    expect(events.listenerCount()).toBe(1);

    events.emit(4242);
    await vi.waitUntil(() => transport.sent.length > 0);

    expect(cdp.detachSession).toHaveBeenCalledWith("aa11");
    expect(manager.has("aa11")).toBe(false);
    expect(transport.sent).toEqual([
      {
        event: "session.window_closed",
        payload: { session_id: "aa11", reason: "user_closed_window" },
      },
    ]);
  });

  it("reports borrowed tabs as return failures when the Agent Window was already closed", async () => {
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 4242),
        remove: vi.fn(async () => {}),
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    const ctx = await manager.start("aa11");
    ctx.borrowedTabs.set(7, { tabId: 7, originalWindowId: 200, originalIndex: 0 });
    const transport = fakeTransport();
    const events = fakeWindowEvents();

    attachSessionEventHandler({
      manager,
      transport,
      windowEvents: events.api,
    });

    events.emit(4242);
    for (let i = 0; i < 4; i += 1) await Promise.resolve();

    expect(transport.sent).toEqual([
      {
        event: "session.window_closed",
        payload: {
          session_id: "aa11",
          reason: "user_closed_window",
          return_failures: [
            {
              tab_id: 7,
              code: "cdp_failed",
              message: "Agent Window was closed before borrowed tab could be returned",
            },
          ],
        },
      },
    ]);
  });

  it("ignores non-agent windows", async () => {
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 1),
        remove: vi.fn(),
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    const transport = fakeTransport();
    const events = fakeWindowEvents();
    attachSessionEventHandler({ manager, transport, windowEvents: events.api });
    events.emit(9999);
    await Promise.resolve();
    expect(transport.sent).toEqual([]);
  });

  it("drops an attach session when its leased tab closes without closing the user window", async () => {
    const remove = vi.fn(async () => {});
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 1),
        remove,
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    manager.startAttached("aa11", 77, 9);
    const transport = fakeTransport();
    const windowEvents = fakeWindowEvents();
    const tabEvents = fakeTabEvents();
    const cdp = { detachSession: vi.fn(async () => {}) };
    attachSessionEventHandler({
      manager,
      transport,
      windowEvents: windowEvents.api,
      tabEvents: tabEvents.api,
      cdp,
    });

    tabEvents.emit(77);
    await vi.waitUntil(() => transport.sent.length > 0);

    expect(cdp.detachSession).toHaveBeenCalledWith("aa11");
    expect(remove).not.toHaveBeenCalled();
    expect(manager.has("aa11")).toBe(false);
    expect(transport.sent).toEqual([
      {
        event: "session.window_closed",
        payload: { session_id: "aa11", reason: "attached_tab_closed" },
      },
    ]);
  });

  it("turns Chrome debugger Cancel into user_aborted and releases the attach session", async () => {
    const remove = vi.fn(async () => {});
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 1),
        remove,
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    manager.startAttached("aa11", 77, 9);
    const transport = fakeTransport();
    const cdp = fakeCdp();
    const resetAgentOverlays = vi.fn(async () => {});
    const onSessionsChanged = vi.fn();
    attachSessionEventHandler({
      manager,
      transport,
      windowEvents: fakeWindowEvents().api,
      tabEvents: fakeTabEvents().api,
      cdp,
      resetAgentOverlays,
      onSessionsChanged,
    });

    cdp.emit({ tabId: 77, reason: "canceled_by_user", sessionIds: ["aa11"] });
    await vi.waitUntil(() => transport.sent.length === 2);

    expect(transport.sent).toEqual([
      {
        event: "session.user_interrupt",
        payload: {
          session_id: "aa11",
          source: "chrome_debugger_infobar",
        },
      },
      {
        event: "session.window_closed",
        payload: {
          session_id: "aa11",
          reason: "debugger_cancelled_by_user",
        },
      },
    ]);
    expect(resetAgentOverlays).toHaveBeenCalledWith(77, "aa11");
    expect(cdp.detachSession).not.toHaveBeenCalled();
    expect(manager.has("aa11")).toBe(false);
    expect(remove).not.toHaveBeenCalled();
    expect(onSessionsChanged).toHaveBeenCalledOnce();
  });

  it("finds the attach session by leased tab when CDP has not tracked an owner yet", async () => {
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 1),
        remove: vi.fn(async () => {}),
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    manager.startAttached("aa11", 77, 9);
    const transport = fakeTransport();
    const cdp = fakeCdp();
    attachSessionEventHandler({
      manager,
      transport,
      windowEvents: fakeWindowEvents().api,
      tabEvents: fakeTabEvents().api,
      cdp,
    });

    cdp.emit({ tabId: 77, reason: "canceled_by_user", sessionIds: [] });
    await vi.waitUntil(() => transport.sent.length === 2);

    expect(transport.sent[0]).toMatchObject({
      event: "session.user_interrupt",
      payload: { session_id: "aa11" },
    });
    expect(manager.has("aa11")).toBe(false);
  });

  it("dispose() removes the listener", () => {
    const manager = new SessionManager({
      agentWindow: {
        create: vi.fn(async () => 1),
        remove: vi.fn(),
        ensureActiveTab: vi.fn(async () => {}),
      },
    });
    const transport = fakeTransport();
    const events = fakeWindowEvents();
    const cdp = fakeCdp();
    const handle = attachSessionEventHandler({
      manager,
      transport,
      windowEvents: events.api,
      cdp,
    });
    expect(events.listenerCount()).toBe(1);
    expect(cdp.listenerCount()).toBe(1);
    handle.dispose();
    expect(events.listenerCount()).toBe(0);
    expect(cdp.listenerCount()).toBe(0);
  });
});
