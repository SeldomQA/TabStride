import { describe, expect, it, vi } from "vitest";
import { SessionManager } from "@/session-manager/manager";
import { type CdpAxNode, type CdpRunner, handleSnapshot } from "../observation";
import { handleSessionStart, handleSessionStop } from "../session";
import { type AgentOverlayResetApi, type ChromeWindowsApi, type TabMutationApi } from "../tabs";

function fakeAgentWindow(ids: number[]) {
  let i = 0;
  const create = vi.fn(async () => {
    const id = ids[i++];
    if (id === undefined) throw new Error("ran out of fake ids");
    return id;
  });
  const remove = vi.fn(async () => {});
  const ensureActiveTab = vi.fn(async () => {});
  return { create, remove, ensureActiveTab };
}

describe("handleSessionStart attach mode", () => {
  it("leases the active tab in the last-focused user window without creating a window", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const active = { id: 77, windowId: 9, active: true } as chrome.tabs.Tab;
    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active" },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: { get: vi.fn(async () => active) },
      },
    );

    expect(result).toEqual({ attached_tab_id: 77 });
    expect(sm.get("aa11")).toMatchObject({ mode: "attach", attachedTabId: 77 });
    expect(aw.create).not.toHaveBeenCalled();
  });

  it("attaches an explicit tab id and refuses a duplicate lease", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const target = { id: 77, windowId: 9, active: false } as chrome.tabs.Tab;
    const deps = {
      tabs: { get: vi.fn(async () => target) },
      windows: { getLastFocused: vi.fn(), getAll: vi.fn() },
    };

    expect(
      await handleSessionStart(sm, { session_id: "aa11", mode: "attach", tab_id: 77 }, deps),
    ).toEqual({ attached_tab_id: 77 });
    const conflict = await handleSessionStart(
      sm,
      { session_id: "bb22", mode: "attach", tab_id: 77 },
      deps,
    );
    expect(conflict).toMatchObject({ code: "permission_denied" });
  });

  it("requires an explicit attach target", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    expect(await handleSessionStart(sm, { session_id: "aa11", mode: "attach" })).toMatchObject({
      code: "invalid_params",
    });
  });
});

// ---------------------------------------------------------------------------
// A-2: merged attach+snapshot — session_start returns initial page state
// ---------------------------------------------------------------------------

const AX_TREE: CdpAxNode[] = [
  {
    nodeId: "1",
    childIds: ["2", "3"],
    role: { type: "role", value: "RootWebArea" },
    name: { type: "name", value: "Example Page" },
  },
  {
    nodeId: "2",
    parentId: "1",
    backendDOMNodeId: 10,
    role: { type: "role", value: "heading" },
    name: { type: "name", value: "Welcome" },
  },
  {
    nodeId: "3",
    parentId: "1",
    backendDOMNodeId: 11,
    role: { type: "role", value: "button" },
    name: { type: "name", value: "Submit" },
  },
];

/** Minimal fake CdpRunner that answers AX-tree + document-identity calls. */
function fakeCdp(axNodes: CdpAxNode[] = AX_TREE, documentVersion = 3, axError?: Error): CdpRunner {
  const send = vi.fn(async <T>(_tabId: number, method: string): Promise<T> => {
    if (method === "Accessibility.getFullAXTree" && axError) throw axError;
    switch (method) {
      case "Accessibility.enable":
        return {} as T;
      case "Accessibility.getFullAXTree":
        return { nodes: axNodes } as T;
      case "Runtime.evaluate":
        return { result: { value: { id: "doc-1", version: documentVersion } } } as T;
      default:
        throw new Error(`unexpected CDP method in fake: ${method}`);
    }
  }) as CdpRunner["send"];
  return { send, trackSessionTab: vi.fn() };
}

describe("handleSessionStart A-2 merged attach+snapshot", () => {
  it("attach with snapshot=true returns url/title/document_version/snapshot_text in one reply", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    const cdp = fakeCdp();
    const active = {
      id: 77,
      windowId: 9,
      active: true,
      url: "https://example.com/page",
      title: "Example Page",
    } as chrome.tabs.Tab;

    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active", snapshot: true },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: { get: vi.fn(async () => active) },
        cdp,
      },
    );

    expect(result).toEqual({
      attached_tab_id: 77,
      url: "https://example.com/page",
      title: "Example Page",
      document_version: 3,
      snapshot_available: true,
      snapshot_text: expect.stringContaining('@e1 heading "Welcome"'),
      snapshot_ref_count: 2,
      snapshot_truncated: false,
    });
    // Initial snapshot refs are registered on the session RefStore so
    // @e1/@e2 resolve in later interaction commands.
    const ctx = sm.get("aa11");
    expect(ctx?.refStore.isEmpty()).toBe(false);
    expect(ctx?.refStore.resolve("e1", { tabId: 77 })).not.toBeNull();
  });

  it("uses the initial snapshot as the cache baseline for the next observation", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    const cdp = fakeCdp();
    const active = {
      id: 77,
      windowId: 9,
      active: true,
      url: "https://example.com/page",
      title: "Example Page",
    } as chrome.tabs.Tab;
    const tabsApi = {
      get: vi.fn(async () => active),
      query: vi.fn(async () => [active]),
    };

    const started = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active", snapshot: true },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: tabsApi,
        cdp,
      },
    );
    expect(started).not.toHaveProperty("code");

    const next = await handleSnapshot(
      sm,
      { session_id: "aa11", incremental: true },
      { cdp, tabsApi },
    );
    if ("code" in next) throw new Error(`unexpected snapshot error: ${next.message}`);

    expect(next.snapshot_kind).toBe("cached");
    expect(next.text).toBe("");
    expect(next.base_document_version).toBe(3);
    expect(
      vi.mocked(cdp.send).mock.calls.filter((call) => call[1] === "Accessibility.getFullAXTree"),
    ).toHaveLength(1);
  });

  it("excludes TabStride overlay controls from the initial snapshot", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    const active = {
      id: 77,
      windowId: 9,
      active: true,
      url: "https://example.com/page",
      title: "Example Page",
    } as chrome.tabs.Tab;
    const nodes: CdpAxNode[] = [
      {
        nodeId: "1",
        childIds: ["2", "3"],
        role: { type: "role", value: "RootWebArea" },
        name: { type: "name", value: "Example Page" },
      },
      {
        nodeId: "2",
        parentId: "1",
        backendDOMNodeId: 11,
        role: { type: "role", value: "button" },
        name: { type: "name", value: "Save" },
      },
      {
        nodeId: "3",
        parentId: "1",
        backendDOMNodeId: 902,
        role: { type: "role", value: "button" },
        name: { type: "name", value: "Stop agent" },
      },
    ];
    const send = vi.fn(async <T>(_tabId: number, method: string): Promise<T> => {
      if (method === "Runtime.evaluate") {
        return { result: { value: { id: "doc-1", version: 1 } } } as T;
      }
      if (method === "Accessibility.enable") return {} as T;
      if (method === "Accessibility.getFullAXTree") return { nodes } as T;
      if (method === "DOM.getDocument") return { root: { nodeId: 1 } } as T;
      if (method === "DOM.querySelector") return { nodeId: 9 } as T;
      if (method === "DOM.describeNode") {
        return {
          node: {
            nodeId: 9,
            backendNodeId: 900,
            shadowRoots: [
              {
                nodeId: 10,
                backendNodeId: 901,
                children: [{ nodeId: 11, backendNodeId: 902 }],
              },
            ],
          },
        } as T;
      }
      throw new Error(`unexpected CDP method ${method}`);
    }) as CdpRunner["send"];
    const cdp: CdpRunner = { send, runtimeCounters: {} };

    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active", snapshot: true },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: { get: vi.fn(async () => active) },
        cdp,
      },
    );

    expect(result).not.toHaveProperty("code");
    if ("code" in result) throw new Error(`unexpected session error: ${result.message}`);
    expect(result.snapshot_text).toContain('button "Save"');
    expect(result.snapshot_text).not.toContain("Stop agent");
    expect(result.snapshot_ref_count).toBe(1);
    expect(sm.get("aa11")?.refStore.resolve("e1", { tabId: 77 })).toBe(11);
    expect(sm.get("aa11")?.refStore.resolve("e2", { tabId: 77 })).toBeNull();
  });

  it("computes an incremental snapshot against the session-start baseline", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    const active = {
      id: 77,
      windowId: 9,
      active: true,
      url: "https://example.com/page",
      title: "Example Page",
    } as chrome.tabs.Tab;
    let version = 1;
    let nodes = AX_TREE;
    const send = vi.fn(async <T>(_tabId: number, method: string): Promise<T> => {
      if (method === "Runtime.evaluate") {
        return { result: { value: { id: "doc-1", version } } } as T;
      }
      if (method === "Accessibility.enable") return {} as T;
      if (method === "Accessibility.getFullAXTree") return { nodes } as T;
      if (method === "DOM.getDocument") return { root: { nodeId: 1 } } as T;
      if (method === "DOM.querySelector") return { nodeId: 0 } as T;
      throw new Error(`unexpected CDP method ${method}`);
    }) as CdpRunner["send"];
    const cdp: CdpRunner = { send, runtimeCounters: {} };
    const tabsApi = {
      get: vi.fn(async () => active),
      query: vi.fn(async () => [active]),
    };

    const started = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active", snapshot: true },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: tabsApi,
        cdp,
      },
    );
    expect(started).not.toHaveProperty("code");

    version = 2;
    nodes = [
      {
        nodeId: "1",
        childIds: ["2", "3", "4"],
        role: { type: "role", value: "RootWebArea" },
        name: { type: "name", value: "Example Page" },
      },
      AX_TREE[1],
      AX_TREE[2],
      {
        nodeId: "4",
        parentId: "1",
        backendDOMNodeId: 12,
        role: { type: "role", value: "textbox" },
        name: { type: "name", value: "Email" },
      },
    ];
    const delta = await handleSnapshot(
      sm,
      { session_id: "aa11", incremental: true },
      { cdp, tabsApi },
    );
    if ("code" in delta) throw new Error(`unexpected snapshot error: ${delta.message}`);

    expect(delta.snapshot_kind).toBe("incremental");
    expect(delta.base_document_version).toBe(1);
    expect(delta.document_version).toBe(2);
    expect(delta.text).toContain('textbox "Email"');
    expect(delta.removed_refs).toEqual([]);
    expect(
      vi.mocked(cdp.send).mock.calls.filter((call) => call[1] === "Accessibility.getFullAXTree"),
    ).toHaveLength(2);
  });

  it("attach without snapshot skips CDP entirely and returns only the lease", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    const cdp = fakeCdp();
    const active = { id: 77, windowId: 9, active: true } as chrome.tabs.Tab;

    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active" },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: { get: vi.fn(async () => active) },
        cdp,
      },
    );

    expect(result).toEqual({ attached_tab_id: 77 });
    expect(cdp.send).not.toHaveBeenCalled();
  });

  it("attach with snapshot but no CDP returns a registered session with structured failure", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    const active = { id: 77, windowId: 9, active: true } as chrome.tabs.Tab;

    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active", snapshot: true },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: { get: vi.fn(async () => active) },
      },
    );

    expect(result).toMatchObject({
      attached_tab_id: 77,
      snapshot_available: false,
      snapshot_error: {
        code: "cdp_failed",
        message: "snapshot requested but CDP is unavailable",
      },
    });
    expect(sm.has("aa11")).toBe(true);
  });

  it("isolated session with snapshot=true reads url/title from the agent tab", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const cdp = fakeCdp();
    const agentTab = {
      id: 777,
      windowId: 100,
      active: true,
      url: "about:blank",
      title: "",
    } as chrome.tabs.Tab;

    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", snapshot: true },
      {
        tabs: {
          get: vi.fn(async () => agentTab),
          query: vi.fn(async () => [agentTab]),
        },
        cdp,
      },
    );

    expect(result).toMatchObject({
      agent_window_id: 100,
      url: "about:blank",
      snapshot_available: true,
      snapshot_ref_count: 2,
    });
    expect(cdp.send).toHaveBeenCalledWith(777, "Accessibility.getFullAXTree", {});
    expect(aw.create).toHaveBeenCalled();
  });

  it("isolated snapshot failure remains a tracked, stoppable session", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });

    const result = await handleSessionStart(sm, { session_id: "aa11", snapshot: true });

    expect(result).toEqual({
      agent_window_id: 100,
      snapshot_available: false,
      snapshot_error: {
        code: "cdp_failed",
        message: "snapshot requested but CDP is unavailable",
      },
    });
    expect(sm.has("aa11")).toBe(true);
    await expect(handleSessionStop(sm, { session_id: "aa11" })).resolves.toEqual({});
    expect(sm.has("aa11")).toBe(false);
    expect(aw.remove).toHaveBeenCalledWith(100);
  });

  it("snapshot capture failure is non-fatal and still returns page metadata", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    // AX tree call throws → captureInitialSnapshot degrades to metadata-only.
    const cdp = fakeCdp(AX_TREE, 3, new Error("CDP exploded"));
    const active = {
      id: 77,
      windowId: 9,
      active: true,
      url: "https://example.com/page",
      title: "Example Page",
    } as chrome.tabs.Tab;

    const result = await handleSessionStart(
      sm,
      { session_id: "aa11", mode: "attach", tab: "active", snapshot: true },
      {
        windows: {
          getLastFocused: vi.fn(async () => ({ id: 9, tabs: [active] }) as chrome.windows.Window),
          getAll: vi.fn(async () => []),
        },
        tabs: { get: vi.fn(async () => active) },
        cdp,
      },
    );

    expect(result).toEqual({
      attached_tab_id: 77,
      url: "https://example.com/page",
      title: "Example Page",
      document_version: 3,
      snapshot_available: false,
      snapshot_error: { code: "cdp_failed", message: "CDP exploded" },
    });
    // Session stays alive so the agent can retry with tool.snapshot.
    expect(sm.has("aa11")).toBe(true);
  });
});

interface FakeState {
  tabs: Map<number, chrome.tabs.Tab>;
  windowsClosed: Set<number>;
  moves: Array<{ tabId: number; windowId: number; index: number }>;
}

function makeApis(
  state: FakeState,
  opts?: { moveThrowsFor?: Set<number> },
): {
  tabs: TabMutationApi;
  windows: ChromeWindowsApi;
} {
  const tabs: TabMutationApi = {
    create: vi.fn(),
    remove: vi.fn(async () => {}),
    update: vi.fn(async (_id, _p) => undefined),
    get: vi.fn(async (id) => {
      const t = state.tabs.get(id);
      if (!t) throw new Error(`tab ${id} not found`);
      return t;
    }),
    move: vi.fn(async (id, props) => {
      if (opts?.moveThrowsFor?.has(id)) throw new Error("simulated move failure");
      state.moves.push({
        tabId: id,
        windowId: typeof props.windowId === "number" ? props.windowId : -1,
        index: typeof props.index === "number" ? props.index : 0,
      });
      const t = state.tabs.get(id);
      if (t && typeof props.windowId === "number") {
        (t as { windowId?: number }).windowId = props.windowId;
      }
      return t!;
    }),
  };
  const windows: ChromeWindowsApi = {
    get: vi.fn(async (windowId: number) => {
      if (state.windowsClosed.has(windowId)) {
        throw new Error(`window ${windowId} closed`);
      }
      return { id: windowId } as chrome.windows.Window;
    }),
    getLastFocused: vi.fn(async () => ({ id: 500 }) as chrome.windows.Window),
    create: vi.fn(async () => ({ id: 999 }) as chrome.windows.Window),
  };
  return { tabs, windows };
}

describe("handleSessionStop with auto-return", () => {
  it("clears the attached-tab control overlay before detaching CDP", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    sm.startAttached("aa11", 77, 9);
    const order: string[] = [];
    const agentOverlayReset = {
      resetAgentOverlays: vi.fn(async () => {
        order.push("overlay-reset");
      }),
    } satisfies AgentOverlayResetApi;
    const cdp = {
      detachSession: vi.fn(async () => {
        order.push("cdp-detach");
      }),
    };

    const res = await handleSessionStop(sm, { session_id: "aa11" }, { cdp, agentOverlayReset });

    if ("code" in res) throw new Error(`unexpected error: ${JSON.stringify(res)}`);
    expect(agentOverlayReset.resetAgentOverlays).toHaveBeenCalledWith(77, "aa11");
    expect(cdp.detachSession).toHaveBeenCalledWith("aa11");
    expect(order).toEqual(["overlay-reset", "cdp-detach"]);
    expect(sm.has("aa11")).toBe(false);
  });

  it("still releases an attach session when its content script is unavailable", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow([100]) });
    sm.startAttached("aa11", 77, 9);
    const agentOverlayReset = {
      resetAgentOverlays: vi.fn(async () => {
        throw new Error("Receiving end does not exist");
      }),
    } satisfies AgentOverlayResetApi;

    const res = await handleSessionStop(sm, { session_id: "aa11" }, { agentOverlayReset });

    if ("code" in res) throw new Error(`unexpected error: ${JSON.stringify(res)}`);
    expect(sm.has("aa11")).toBe(false);
  });

  it("returns every borrowed tab and closes the Agent Window in the right order", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const ctx = await sm.start("aa11");
    ctx.borrowedTabs.set(1, { tabId: 1, originalWindowId: 200, originalIndex: 0 });
    ctx.borrowedTabs.set(2, { tabId: 2, originalWindowId: 200, originalIndex: 1 });
    ctx.borrowedTabs.set(3, { tabId: 3, originalWindowId: 201, originalIndex: 2 });

    const state: FakeState = {
      tabs: new Map([
        [1, { id: 1, windowId: 100 } as chrome.tabs.Tab],
        [2, { id: 2, windowId: 100 } as chrome.tabs.Tab],
        [3, { id: 3, windowId: 100 } as chrome.tabs.Tab],
      ]),
      windowsClosed: new Set(),
      moves: [],
    };
    const { tabs, windows } = makeApis(state);
    const cdp = { detachSession: vi.fn(async () => {}) };
    const order: string[] = [];
    aw.remove.mockImplementation(async () => {
      order.push("remove-window");
    });
    cdp.detachSession.mockImplementation(async () => {
      order.push("cdp-detach");
    });

    // Wrap move to record when each tab was moved.
    const baseMove = tabs.move;
    tabs.move = vi.fn(async (id: number, p: chrome.tabs.MoveProperties) => {
      order.push(`move-${id}`);
      return baseMove(id, p);
    }) as unknown as TabMutationApi["move"];

    const res = await handleSessionStop(
      sm,
      { session_id: "aa11" },
      { cdp, tabManagement: { tabs, windows } },
    );

    if ("code" in res) throw new Error(`unexpected error: ${JSON.stringify(res)}`);
    expect(res.returned_tab_ids?.sort((a, b) => a - b)).toEqual([1, 2, 3]);
    expect(res.return_failures).toBeUndefined();
    expect(ctx.borrowedTabs.size).toBe(0);
    expect(sm.has("aa11")).toBe(false);

    // Order: every tab move happens before cdp.detach and window remove.
    const detachIdx = order.indexOf("cdp-detach");
    const removeIdx = order.indexOf("remove-window");
    for (const id of [1, 2, 3]) {
      const moveIdx = order.indexOf(`move-${id}`);
      expect(moveIdx).toBeGreaterThanOrEqual(0);
      expect(moveIdx).toBeLessThan(detachIdx);
      expect(moveIdx).toBeLessThan(removeIdx);
    }
    expect(detachIdx).toBeLessThan(removeIdx);
  });

  it("resets agent overlays for tabs returned during session_stop auto-cleanup", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const ctx = await sm.start("aa11");
    ctx.borrowedTabs.set(7, { tabId: 7, originalWindowId: 200, originalIndex: 3 });

    const state: FakeState = {
      tabs: new Map([[7, { id: 7, windowId: 100 } as chrome.tabs.Tab]]),
      windowsClosed: new Set(),
      moves: [],
    };
    const { tabs, windows } = makeApis(state);
    const agentOverlayReset = {
      resetAgentOverlays: vi.fn(async () => {}),
    } satisfies AgentOverlayResetApi;

    const res = await handleSessionStop(
      sm,
      { session_id: "aa11" },
      {
        tabManagement: { tabs, windows, agentOverlayReset },
      },
    );

    if ("code" in res) throw new Error(`unexpected error: ${JSON.stringify(res)}`);
    expect(agentOverlayReset.resetAgentOverlays).toHaveBeenCalledWith(7, "aa11");
  });

  it("falls back when the original window is gone but still completes stop", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const ctx = await sm.start("aa11");
    ctx.borrowedTabs.set(7, { tabId: 7, originalWindowId: 200, originalIndex: 3 });

    const state: FakeState = {
      tabs: new Map([[7, { id: 7, windowId: 100 } as chrome.tabs.Tab]]),
      windowsClosed: new Set([200]),
      moves: [],
    };
    const { tabs, windows } = makeApis(state);
    const res = await handleSessionStop(
      sm,
      { session_id: "aa11" },
      { tabManagement: { tabs, windows } },
    );
    if ("code" in res) throw new Error(`unexpected error: ${JSON.stringify(res)}`);
    expect(res.returned_tab_ids).toEqual([7]);
    expect(state.moves[0].windowId).toBe(500); // lastFocused fallback
    expect(sm.has("aa11")).toBe(false);
  });

  it("keeps the session open when any borrowed tab cannot be returned", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const ctx = await sm.start("aa11");
    ctx.borrowedTabs.set(1, { tabId: 1, originalWindowId: 200, originalIndex: 0 });
    ctx.borrowedTabs.set(2, { tabId: 2, originalWindowId: 200, originalIndex: 1 });

    const state: FakeState = {
      tabs: new Map([
        [1, { id: 1, windowId: 100 } as chrome.tabs.Tab],
        [2, { id: 2, windowId: 100 } as chrome.tabs.Tab],
      ]),
      windowsClosed: new Set(),
      moves: [],
    };
    const { tabs, windows } = makeApis(state, { moveThrowsFor: new Set([1]) });

    const res = await handleSessionStop(
      sm,
      { session_id: "aa11" },
      { tabManagement: { tabs, windows } },
    );
    if ("code" in res) throw new Error(`unexpected error: ${JSON.stringify(res)}`);
    expect(res.return_failures?.map((f) => f.tab_id)).toEqual([1]);
    expect(res.returned_tab_ids).toEqual([2]);
    expect(sm.has("aa11")).toBe(true);
    expect(ctx.borrowedTabs.has(1)).toBe(true);
    expect(ctx.borrowedTabs.has(2)).toBe(false);
    expect(aw.remove).not.toHaveBeenCalled();
  });

  it("clears the RefStore before window teardown", async () => {
    const aw = fakeAgentWindow([100]);
    const sm = new SessionManager({ agentWindow: aw });
    const ctx = await sm.start("aa11");
    // Insert a fake ref so we can verify clear() ran.
    ctx.refStore.set("e1", 123, { tabId: 7 });
    const state: FakeState = {
      tabs: new Map(),
      windowsClosed: new Set(),
      moves: [],
    };
    const { tabs, windows } = makeApis(state);
    await handleSessionStop(sm, { session_id: "aa11" }, { tabManagement: { tabs, windows } });
    expect(ctx.refStore.isEmpty()).toBe(true);
  });
});
