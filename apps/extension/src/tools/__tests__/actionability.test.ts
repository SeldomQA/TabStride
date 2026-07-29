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

  it("fails immediately with structured state for a non-editable fill target", async () => {
    const ctx = await context();
    const fake = actionabilityCdp([{ ...readyState, editable: false }]);

    const result = await waitForActionable(fake.cdp, ctx, 4, { css: "#title" }, "fill", {
      timeoutMs: 1_000,
    });

    expect(result).toMatchObject({
      code: "invalid_params",
      data: {
        reason: "element_not_editable",
        failed_check: "editable",
        last_state: { editable: false },
      },
    });
    expect(fake.counts.inspections).toBe(1);
  });

  it("requires a real select and a focusable press target", async () => {
    const ctx = await context();
    const notSelect = actionabilityCdp([{ ...readyState, select: false }]);
    const notFocusable = actionabilityCdp([{ ...readyState, focusable: false }]);

    await expect(
      waitForActionable(notSelect.cdp, ctx, 4, { css: "#country" }, "select", {
        timeoutMs: 100,
      }),
    ).resolves.toMatchObject({
      code: "invalid_params",
      data: { reason: "target_not_select", failed_check: "select" },
    });
    await expect(
      waitForActionable(notFocusable.cdp, ctx, 4, { css: "#title" }, "press", {
        timeoutMs: 100,
      }),
    ).resolves.toMatchObject({
      code: "invalid_params",
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
});
