import { afterEach, describe, expect, it, vi } from "vitest";
import { SessionManager } from "@/session-manager/manager";
import type { CdpRunner } from "@/tools/shared";
import type { Locator } from "@/transport/types";
import { resolveLocator, semanticLocatorExpression, validateLocator } from "../locator";

function fakeAgentWindow() {
  return {
    create: vi.fn(async () => 100),
    remove: vi.fn(async () => {}),
    ensureActiveTab: vi.fn(async () => {}),
  };
}

function fakeCdp(handlers: Record<string, (params?: object) => unknown>): CdpRunner {
  return {
    send: vi.fn(async (_tabId: number, method: string, params?: object) => {
      const handler = handlers[method];
      if (!handler) throw new Error(`unexpected CDP call ${method}`);
      return handler(params);
    }) as unknown as CdpRunner["send"],
  };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("validateLocator", () => {
  it("accepts every supported strategy", () => {
    for (const locator of [
      { ref: "@e1" },
      { css: "#save" },
      { role: "button", name: "Save" },
      { label: "Email" },
      { placeholder: "name@example.com" },
      { text: "Welcome" },
      { testId: "save-button" },
    ]) {
      expect(validateLocator(locator)).toBeNull();
    }
  });

  it("rejects missing, combined, and incomplete role strategies", () => {
    expect(validateLocator({})).toMatchObject({ code: "invalid_params" });
    expect(validateLocator({ css: "button", text: "Save" })).toMatchObject({
      code: "invalid_params",
    });
    expect(validateLocator({ role: "button" })).toMatchObject({ code: "invalid_params" });
    expect(validateLocator({ name: "Save" })).toMatchObject({ code: "invalid_params" });
    expect(validateLocator({ role: "button", name: "   " })).toMatchObject({
      code: "invalid_params",
    });
    expect(validateLocator({ css: "button", exact: true })).toMatchObject({
      code: "invalid_params",
    });
  });
});

describe("semantic locator expressions", () => {
  it.each([
    [{ role: "button", name: "Save", exact: true }, "save"],
    [{ label: "Email address", exact: true }, "email"],
    [{ placeholder: "name@example.com", exact: true }, "email"],
    [{ text: "Welcome back", exact: true }, "welcome"],
    [{ testId: "save-button", exact: true }, "save"],
  ] as Array<[Locator, string]>)("resolves %j", (locator, expectedId) => {
    document.body.innerHTML = `
      <label for="email">Email address</label>
      <input id="email" placeholder="name@example.com">
      <p id="welcome">Welcome back</p>
      <button id="save" data-testid="save-button">Save</button>
    `;
    const matches = globalThis.eval(semanticLocatorExpression(locator)) as Element[];
    expect(matches.map((element) => element.id)).toEqual([expectedId]);
  });

  it("uses substring matching by default and exact matching when requested", () => {
    document.body.innerHTML = `
      <button id="short">Save</button>
      <button id="long">Save package</button>
    `;
    const loose = globalThis.eval(
      semanticLocatorExpression({ role: "button", name: "Save" }),
    ) as Element[];
    const exact = globalThis.eval(
      semanticLocatorExpression({ role: "button", name: "Save", exact: true }),
    ) as Element[];
    expect(loose.map((element) => element.id)).toEqual(["short", "long"]);
    expect(exact.map((element) => element.id)).toEqual(["short"]);
  });
});

describe("resolveLocator strict matching", () => {
  it("resolves one CSS match", async () => {
    const manager = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = await manager.start("aa11");
    const cdp = fakeCdp({
      "DOM.getDocument": () => ({ root: { nodeId: 1 } }),
      "DOM.querySelectorAll": () => ({ nodeIds: [9] }),
      "DOM.describeNode": () => ({ node: { backendNodeId: 99 } }),
    });
    await expect(resolveLocator(cdp, ctx, 4, { css: "#save" })).resolves.toEqual({
      backendNodeId: 99,
      usedTarget: { css: "#save" },
      matchCount: 1,
    });
  });

  it("returns not_found for zero matches", async () => {
    const manager = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = await manager.start("aa11");
    const cdp = fakeCdp({
      "DOM.getDocument": () => ({ root: { nodeId: 1 } }),
      "DOM.querySelectorAll": () => ({ nodeIds: [] }),
    });
    await expect(resolveLocator(cdp, ctx, 4, { css: ".missing" })).resolves.toMatchObject({
      code: "not_found",
      data: { reason: "locator_not_found", match_count: 0 },
    });
  });

  it("returns ambiguous_target and match count for multiple matches", async () => {
    const manager = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = await manager.start("aa11");
    const cdp = fakeCdp({
      "DOM.getDocument": () => ({ root: { nodeId: 1 } }),
      "DOM.querySelectorAll": () => ({ nodeIds: [7, 8] }),
    });
    await expect(resolveLocator(cdp, ctx, 4, { css: ".save" })).resolves.toMatchObject({
      code: "ambiguous_target",
      data: { reason: "ambiguous_target", match_count: 2 },
    });
  });

  it("keeps snapshot refs scoped to their tab", async () => {
    const manager = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = await manager.start("aa11");
    ctx.refStore.set("e1", 55, { tabId: 4 });
    await expect(resolveLocator(fakeCdp({}), ctx, 5, { ref: "@e1" })).resolves.toMatchObject({
      code: "not_found",
      data: { reason: "ref_not_found", match_count: 0 },
    });
  });

  it("resolves one semantic remote object", async () => {
    const manager = new SessionManager({ agentWindow: fakeAgentWindow() });
    const ctx = await manager.start("aa11");
    const cdp = fakeCdp({
      "Runtime.evaluate": () => ({ result: { objectId: "array-1" } }),
      "Runtime.getProperties": () => ({
        result: [
          { name: "length", value: { value: 1 } },
          { name: "0", value: { objectId: "button-1" } },
        ],
      }),
      "DOM.describeNode": () => ({ node: { backendNodeId: 77 } }),
      "Runtime.releaseObjectGroup": () => ({}),
    });
    await expect(
      resolveLocator(cdp, ctx, 4, { role: "button", name: "Save", exact: true }),
    ).resolves.toEqual({
      backendNodeId: 77,
      usedTarget: { role: "button", name: "Save", exact: true },
      matchCount: 1,
    });
  });
});
