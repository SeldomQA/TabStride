import { describe, expect, it, vi } from "vitest";
import { SessionManager } from "@/session-manager/manager";
import type { CdpRunner } from "@/tools/shared";
import type { ActionabilityState } from "../actionability";
import { __testing__, waitForActionable } from "../actionability";

function agentWindow() {
  return {
    create: vi.fn(async () => 100),
    remove: vi.fn(async () => {}),
    ensureActiveTab: vi.fn(async () => {}),
  };
}

const readyState: Omit<ActionabilityState, "stable"> = {
  attached: true,
  visible: true,
  enabled: true,
  editable: true,
  receives_events: true,
  not_obscured: true,
  focusable: true,
  select: true,
  rect: { x: 10, y: 20, width: 100, height: 40 },
};

function actionabilityCdp(states: Array<Omit<ActionabilityState, "stable">>) {
  let inspections = 0;
  let resolutions = 0;
  const send = vi.fn(async (_tabId: number, method: string) => {
    switch (method) {
      case "DOM.getDocument":
        return { root: { nodeId: 1 } };
      case "DOM.querySelectorAll":
        resolutions += 1;
        return { nodeIds: [9] };
      case "DOM.describeNode":
        return { node: { backendNodeId: 99 } };
      case "DOM.scrollIntoViewIfNeeded":
        return {};
      case "DOM.resolveNode":
        return { object: { objectId: `node-${resolutions}` } };
      case "Runtime.callFunctionOn": {
        const state = states[Math.min(inspections, states.length - 1)];
        inspections += 1;
        return { result: { value: state } };
      }
      default:
        throw new Error(`unexpected CDP call ${method}`);
    }
  }) as unknown as CdpRunner["send"];
  return {
    cdp: { send } satisfies CdpRunner,
    counts: {
      get inspections() {
        return inspections;
      },
      get resolutions() {
        return resolutions;
      },
    },
  };
}

async function context() {
  const manager = new SessionManager({ agentWindow: agentWindow() });
  return manager.start("aa11");
}

describe("Actionability Engine v1", () => {
  it("uses the action-specific check sets", () => {
    expect(__testing__.REQUIRED_CHECKS.click).toEqual([
      "attached",
      "visible",
      "stable",
      "enabled",
      "receives_events",
      "not_obscured",
    ]);
    expect(__testing__.REQUIRED_CHECKS.fill).toEqual([
      "attached",
      "visible",
      "enabled",
      "editable",
    ]);
    expect(__testing__.REQUIRED_CHECKS.press).toEqual(["attached", "visible", "focusable"]);
    expect(__testing__.REQUIRED_CHECKS.select).toEqual([
      "attached",
      "visible",
      "enabled",
      "select",
    ]);
  });

  it("re-resolves a click target until its geometry is stable", async () => {
    const ctx = await context();
    const fake = actionabilityCdp([readyState, readyState]);

    const result = await waitForActionable(fake.cdp, ctx, 4, { css: "#save" }, "click", {
      timeoutMs: 200,
      pollIntervalMs: 1,
    });

    if ("code" in result) throw new Error(JSON.stringify(result));
    expect(result.actionability.stable).toBe(true);
    expect(fake.counts.resolutions).toBe(2);
    expect(fake.counts.inspections).toBe(2);
  });

  it("returns structured timeout data when a click stays obscured", async () => {
    const ctx = await context();
    const obscured = {
      ...readyState,
      receives_events: false,
      not_obscured: false,
      obscured_by: "#dialog",
    };
    const fake = actionabilityCdp([obscured]);

    const result = await waitForActionable(fake.cdp, ctx, 4, { css: "#save" }, "click", {
      timeoutMs: 12,
      pollIntervalMs: 1,
    });

    expect(result).toMatchObject({
      code: "timeout",
      data: {
        reason: "element_obscured",
        failed_check: "receives_events",
        last_state: {
          attached: true,
          visible: true,
          receives_events: false,
          not_obscured: false,
          obscured_by: "#dialog",
        },
      },
    });
    if (!("code" in result)) throw new Error("expected timeout");
    expect(result.data?.elapsed_ms).toEqual(expect.any(Number));
  });

  it("waits for editability and returns structured state on timeout", async () => {
    const ctx = await context();
    const fake = actionabilityCdp([{ ...readyState, editable: false }]);

    const result = await waitForActionable(fake.cdp, ctx, 4, { css: "#title" }, "fill", {
      timeoutMs: 12,
      pollIntervalMs: 1,
    });

    expect(result).toMatchObject({
      code: "timeout",
      data: {
        reason: "element_not_editable",
        failed_check: "editable",
        last_state: { editable: false },
      },
    });
    expect(fake.counts.inspections).toBeGreaterThan(1);
  });

  it("requires a real select and a focusable press target", async () => {
    const ctx = await context();
    const notSelect = actionabilityCdp([{ ...readyState, select: false }]);
    const notFocusable = actionabilityCdp([{ ...readyState, focusable: false }]);

    await expect(
      waitForActionable(notSelect.cdp, ctx, 4, { css: "#country" }, "select", {
        timeoutMs: 12,
        pollIntervalMs: 1,
      }),
    ).resolves.toMatchObject({
      code: "timeout",
      data: { reason: "target_not_select", failed_check: "select" },
    });
    await expect(
      waitForActionable(notFocusable.cdp, ctx, 4, { css: "#title" }, "press", {
        timeoutMs: 12,
        pollIntervalMs: 1,
      }),
    ).resolves.toMatchObject({
      code: "timeout",
      data: { reason: "element_not_focusable", failed_check: "focusable" },
    });
  });

  it("honours cancellation before issuing CDP calls", async () => {
    const ctx = await context();
    const fake = actionabilityCdp([readyState]);
    const abort = new AbortController();
    abort.abort();

    await expect(
      waitForActionable(fake.cdp, ctx, 4, { css: "#save" }, "click", {
        timeoutMs: 100,
        signal: abort.signal,
      }),
    ).resolves.toMatchObject({ code: "cancelled" });
    expect(fake.counts.resolutions).toBe(0);
  });

  it("waits for a delayed Locator match and then acts on the fresh node", async () => {
    const ctx = await context();
    let resolutions = 0;
    let waits = 0;
    const send = vi.fn(async (_tabId: number, method: string) => {
      switch (method) {
        case "DOM.getDocument":
          return { root: { nodeId: 1 } };
        case "DOM.querySelectorAll":
          resolutions += 1;
          return { nodeIds: resolutions === 1 ? [] : [9] };
        case "DOM.describeNode":
          return { node: { backendNodeId: 101 } };
        case "DOM.scrollIntoViewIfNeeded":
          return {};
        case "DOM.resolveNode":
          return { object: { objectId: "delayed-input" } };
        case "Runtime.callFunctionOn":
          return { result: { value: readyState } };
        default:
          throw new Error(`unexpected CDP call ${method}`);
      }
    }) as unknown as CdpRunner["send"];

    const result = await waitForActionable(
      { send },
      ctx,
      4,
      { css: "#delayed" },
      "fill",
      {
        timeoutMs: 100,
        waitForChange: async () => {
          waits += 1;
          return "mutation";
        },
      },
    );

    if ("code" in result) throw new Error(JSON.stringify(result));
    expect(result.backendNodeId).toBe(101);
    expect(resolutions).toBe(2);
    expect(waits).toBe(1);
  });

  it("re-resolves after DOM reconstruction and resets stability for the new node", async () => {
    const ctx = await context();
    let resolution = 0;
    let inspectedBackend = 0;
    const send = vi.fn(async (_tabId: number, method: string, params?: object) => {
      switch (method) {
        case "DOM.getDocument":
          return { root: { nodeId: 1 } };
        case "DOM.querySelectorAll":
          resolution += 1;
          return { nodeIds: [resolution] };
        case "DOM.describeNode": {
          const nodeId = (params as { nodeId?: number })?.nodeId;
          return { node: { backendNodeId: nodeId === 1 ? 99 : 100 } };
        }
        case "DOM.scrollIntoViewIfNeeded":
          return {};
        case "DOM.resolveNode":
          inspectedBackend = (params as { backendNodeId: number }).backendNodeId;
          return { object: { objectId: `node-${inspectedBackend}` } };
        case "Runtime.callFunctionOn":
          return {
            result: {
              value:
                inspectedBackend === 99
                  ? { ...readyState, enabled: false }
                  : readyState,
            },
          };
        default:
          throw new Error(`unexpected CDP call ${method}`);
      }
    }) as unknown as CdpRunner["send"];

    const result = await waitForActionable({ send }, ctx, 4, { css: "#save" }, "click", {
      timeoutMs: 100,
      waitForChange: async () => "mutation",
    });

    if ("code" in result) throw new Error(JSON.stringify(result));
    expect(result.backendNodeId).toBe(100);
    // Old disabled node, first sample of replacement, stable replacement.
    expect(resolution).toBe(3);
    expect(result.actionability.stable).toBe(true);
  });

  it("waits for disabled, moving, and obscured targets to become actionable", async () => {
    const ctx = await context();
    const disabled = actionabilityCdp([
      { ...readyState, enabled: false, editable: false },
      readyState,
    ]);
    const moving = actionabilityCdp([
      { ...readyState, rect: { x: 0, y: 20, width: 100, height: 40 } },
      { ...readyState, rect: { x: 12, y: 20, width: 100, height: 40 } },
      { ...readyState, rect: { x: 12, y: 20, width: 100, height: 40 } },
    ]);
    const obscured = actionabilityCdp([
      {
        ...readyState,
        receives_events: false,
        not_obscured: false,
        obscured_by: "#modal",
      },
      readyState,
    ]);
    const wake = async () => "mutation" as const;

    const fill = await waitForActionable(disabled.cdp, ctx, 4, { css: "#name" }, "fill", {
      timeoutMs: 100,
      waitForChange: wake,
    });
    const clickMoving = await waitForActionable(
      moving.cdp,
      ctx,
      4,
      { css: "#moving" },
      "click",
      { timeoutMs: 100, waitForChange: wake },
    );
    const clickObscured = await waitForActionable(
      obscured.cdp,
      ctx,
      4,
      { css: "#covered" },
      "click",
      { timeoutMs: 100, waitForChange: wake },
    );

    expect(fill).not.toHaveProperty("code");
    expect(clickMoving).not.toHaveProperty("code");
    expect(clickObscured).not.toHaveProperty("code");
    expect(disabled.counts.resolutions).toBe(2);
    expect(moving.counts.resolutions).toBe(3);
    expect(obscured.counts.resolutions).toBe(2);
  });

  it("keeps strict ambiguity immediate without entering Auto Wait", async () => {
    const ctx = await context();
    const waitForChange = vi.fn(async () => "mutation" as const);
    const send = vi.fn(async (_tabId: number, method: string) => {
      if (method === "DOM.getDocument") return { root: { nodeId: 1 } };
      if (method === "DOM.querySelectorAll") return { nodeIds: [7, 8] };
      throw new Error(`unexpected CDP call ${method}`);
    }) as unknown as CdpRunner["send"];

    await expect(
      waitForActionable({ send }, ctx, 4, { css: ".save" }, "click", {
        timeoutMs: 100,
        waitForChange,
      }),
    ).resolves.toMatchObject({
      code: "ambiguous_target",
      data: { match_count: 2 },
    });
    expect(waitForChange).not.toHaveBeenCalled();
  });

  it("returns the last Locator failure when a delayed target times out", async () => {
    const ctx = await context();
    const send = vi.fn(async (_tabId: number, method: string) => {
      if (method === "DOM.getDocument") return { root: { nodeId: 1 } };
      if (method === "DOM.querySelectorAll") return { nodeIds: [] };
      throw new Error(`unexpected CDP call ${method}`);
    }) as unknown as CdpRunner["send"];

    const result = await waitForActionable({ send }, ctx, 4, { css: "#late" }, "click", {
      timeoutMs: 8,
      pollIntervalMs: 1,
      waitForChange: async (ms) => {
        await new Promise((resolve) => setTimeout(resolve, ms));
        return "fallback";
      },
    });

    expect(result).toMatchObject({
      code: "timeout",
      data: {
        reason: "locator_not_found",
        failed_check: "attached",
        match_count: 0,
        locator: { css: "#late" },
        last_error: {
          code: "not_found",
          data: { reason: "locator_not_found" },
        },
      },
    });
  });

  it("interrupts an in-progress Auto Wait", async () => {
    const ctx = await context();
    const abort = new AbortController();
    const send = vi.fn(async (_tabId: number, method: string) => {
      if (method === "DOM.getDocument") return { root: { nodeId: 1 } };
      if (method === "DOM.querySelectorAll") return { nodeIds: [] };
      throw new Error(`unexpected CDP call ${method}`);
    }) as unknown as CdpRunner["send"];

    const result = await waitForActionable({ send }, ctx, 4, { css: "#late" }, "click", {
      timeoutMs: 100,
      signal: abort.signal,
      waitForChange: async () => {
        abort.abort();
        return "cancelled";
      },
    });

    expect(result).toMatchObject({ code: "cancelled" });
  });
});
