import { describe, expect, it, vi } from "vitest";
import { SessionManager } from "@/session-manager/manager";
import { enforceAgentWindow, lookupSession, resolveTargetTab } from "../shared";

function fakeAgentWindow() {
  return {
    create: vi.fn(async () => 100),
    remove: vi.fn(async () => {}),
    ensureActiveTab: vi.fn(async () => {}),
  };
}

describe("lookupSession", () => {
  it("returns invalid_params when session_id is missing or not a string", () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow() });
    expect(lookupSession(sm, {}, "tool.test")).toMatchObject({
      code: "invalid_params",
      message: "tool.test requires session_id",
    });
    expect(lookupSession(sm, { session_id: "" }, "tool.test")).toMatchObject({
      code: "invalid_params",
    });
    expect(lookupSession(sm, { session_id: 42 as unknown as string }, "tool.test")).toMatchObject({
      code: "invalid_params",
    });
  });

  it("returns not_found for unknown session_id", () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow() });
    expect(lookupSession(sm, { session_id: "zzzz" }, "tool.test")).toMatchObject({
      code: "not_found",
      message: "session zzzz unknown",
    });
  });

  it("returns SessionContext when the session exists", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = await sm.start("aa11");
    const result = lookupSession(sm, { session_id: "aa11" }, "tool.test");
    expect(result).toBe(ctx);
  });
});

describe("attach tab scope", () => {
  it("resolves the leased tab by default and rejects writes to sibling tabs", async () => {
    const sm = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = sm.startAttached("aa11", 77, 9);
    const api = {
      get: vi.fn(async (id: number) => ({ id, windowId: 9, active: id === 77 }) as chrome.tabs.Tab),
      query: vi.fn(async () => []),
    };

    await expect(resolveTargetTab(sm, ctx, undefined, api)).resolves.toEqual({
      tabId: 77,
      windowId: 9,
      active: true,
    });
    expect(enforceAgentWindow(ctx, { tabId: 77, windowId: 9 }, "click")).toBeNull();
    expect(enforceAgentWindow(ctx, { tabId: 78, windowId: 9 }, "click")).toMatchObject({
      code: "permission_denied",
      data: { reason: "attached_tab_scope" },
    });
  });
});
