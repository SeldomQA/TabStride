import { describe, expect, it, vi } from "vitest";
import { SessionManager } from "@/session-manager/manager";
import type { AssertParams } from "@/transport/types";
import { handleAssert } from "../assertion";
import type { CdpRunner } from "../shared";

function agentWindow() {
  return {
    create: vi.fn(async () => 100),
    remove: vi.fn(async () => {}),
    ensureActiveTab: vi.fn(async () => {}),
  };
}

function tabs(windowId = 100) {
  return {
    get: vi.fn(async (tabId: number) => ({ id: tabId, windowId, active: true }) as chrome.tabs.Tab),
    query: vi.fn(async () => [{ id: 4, windowId, active: true } as chrome.tabs.Tab]),
  };
}

const ready = {
  attached: true,
  visible: true,
  text: "Write code",
  value: "done",
  enabled: true,
  editable: true,
  checked: true,
};

function cssCdp(options?: { nodeCounts?: number[]; states?: Array<typeof ready>; url?: string }) {
  let resolutions = 0;
  let inspections = 0;
  const functionDeclarations: string[] = [];
  const nodeCounts = options?.nodeCounts ?? [1];
  const states = options?.states ?? [ready];
  const send = vi.fn(async (_tabId: number, method: string, params?: object) => {
    switch (method) {
      case "DOM.getDocument":
        return { root: { nodeId: 1 } };
      case "DOM.querySelectorAll": {
        const count = nodeCounts[Math.min(resolutions, nodeCounts.length - 1)];
        resolutions += 1;
        return { nodeIds: Array.from({ length: count }, (_, index) => index + 1) };
      }
      case "DOM.describeNode":
        return { node: { backendNodeId: 99 + resolutions } };
      case "DOM.resolveNode":
        return { object: { objectId: "assertion-node" } };
      case "Runtime.callFunctionOn": {
        functionDeclarations.push(
          (params as { functionDeclaration?: string } | undefined)?.functionDeclaration ?? "",
        );
        const state = states[Math.min(inspections, states.length - 1)];
        inspections += 1;
        return { result: { value: state } };
      }
      case "Runtime.evaluate":
        if ((params as { expression?: string })?.expression === "location.href") {
          return { result: { value: options?.url ?? "https://example.com/todos" } };
        }
        throw new Error("unexpected Runtime.evaluate expression");
      default:
        throw new Error(`unexpected CDP call ${method}`);
    }
  }) as unknown as CdpRunner["send"];
  return {
    cdp: { send } satisfies CdpRunner,
    functionDeclarations,
    counts: {
      get resolutions() {
        return resolutions;
      },
    },
  };
}

async function manager() {
  const value = new SessionManager({ agentWindow: agentWindow() });
  await value.start("aa11");
  return value;
}

function params(expectation: Partial<AssertParams>): AssertParams {
  return {
    session_id: "aa11",
    target: { css: "#target" },
    timeout_ms: 100,
    ...expectation,
  };
}

describe("Web-first Assertions v1", () => {
  it("uses the same opacity-independent visible semantics as Actionability", async () => {
    const sm = await manager();
    const fake = cssCdp();

    const result = await handleAssert(sm, params({ visible: true }), {
      cdp: fake.cdp,
      tabsApi: tabs(),
    });

    expect(result).toMatchObject({ assertion: "visible", passed: true });
    expect(fake.functionDeclarations).not.toHaveLength(0);
    expect(fake.functionDeclarations.every((declaration) => !declaration.includes("opacity"))).toBe(
      true,
    );
  });

  it("re-resolves until a delayed element becomes visible", async () => {
    const sm = await manager();
    const fake = cssCdp({
      nodeCounts: [0, 1, 1],
      states: [{ ...ready, visible: false }, ready],
    });
    const result = await handleAssert(sm, params({ visible: true }), {
      cdp: fake.cdp,
      tabsApi: tabs(),
      waitForChange: async () => "mutation",
    });

    expect(result).toMatchObject({
      assertion: "visible",
      passed: true,
      actual: true,
      expected: true,
      match_count: 1,
    });
    expect(fake.counts.resolutions).toBe(3);
  });

  it("treats no match as hidden and supports non-strict count", async () => {
    const sm = await manager();
    await expect(
      handleAssert(sm, params({ hidden: true }), {
        cdp: cssCdp({ nodeCounts: [0] }).cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ passed: true, actual: true, match_count: 0 });

    await expect(
      handleAssert(sm, params({ count: 3 }), {
        cdp: cssCdp({ nodeCounts: [3] }).cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ passed: true, actual: 3, match_count: 3 });
  });

  it("waits for detachment without requiring a strict match", async () => {
    const sm = await manager();
    const fake = cssCdp({ nodeCounts: [1, 0] });
    await expect(
      handleAssert(sm, params({ detached: true }), {
        cdp: fake.cdp,
        tabsApi: tabs(),
        waitForChange: async () => "mutation",
      }),
    ).resolves.toMatchObject({
      assertion: "detached",
      passed: true,
      actual: true,
      match_count: 0,
    });
    expect(fake.counts.resolutions).toBe(2);
  });

  it.each([
    [{ attached: true }, "attached"],
    [{ text_equals: "Write   code" }, "text_equals"],
    [{ text_contains: "ite co" }, "text_contains"],
    [{ value_equals: "done" }, "value_equals"],
    [{ enabled: true }, "enabled"],
    [{ disabled: false }, "disabled"],
    [{ editable: true }, "editable"],
    [{ checked: true }, "checked"],
    [{ unchecked: false }, "unchecked"],
    [{ populated: true }, "populated"],
  ] as Array<[Partial<AssertParams>, string]>)("passes %s", async (expectation, name) => {
    const sm = await manager();
    await expect(
      handleAssert(sm, params(expectation), {
        cdp: cssCdp().cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ assertion: name, passed: true });
  });

  it("retries URL equality and supports regular expressions", async () => {
    const sm = await manager();
    const urlParams = params({ url_equals: "https://example.com/todos" });
    delete urlParams.target;
    await expect(
      handleAssert(sm, urlParams, {
        cdp: cssCdp().cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ assertion: "url_equals", passed: true });

    const regexParams = params({ url_matches: "/todos$".slice(1) });
    delete regexParams.target;
    await expect(
      handleAssert(sm, regexParams, {
        cdp: cssCdp().cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ assertion: "url_matches", passed: true });
  });

  it("returns ambiguous targets immediately and structured timeout state", async () => {
    const sm = await manager();
    await expect(
      handleAssert(sm, params({ visible: true }), {
        cdp: cssCdp({ nodeCounts: [2] }).cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({
      code: "ambiguous_target",
      data: { match_count: 2 },
    });

    await expect(
      handleAssert(
        sm,
        { ...params({ value_equals: "ready" }), timeout_ms: 1 },
        {
          cdp: cssCdp().cdp,
          tabsApi: tabs(),
          waitForChange: async () => "fallback",
        },
      ),
    ).resolves.toMatchObject({
      code: "timeout",
      data: {
        reason: "assertion_failed",
        assertion: "value_equals",
        expected: "ready",
        actual: "done",
        match_count: 1,
        timing: {
          locator_ms: expect.any(Number),
          wait_ms: expect.any(Number),
          cdp_ms: expect.any(Number),
        },
      },
    });
  });

  it("keeps attach assertions inside the leased tab and honours cancellation", async () => {
    const sm = new SessionManager({ agentWindow: agentWindow() });
    sm.startAttached("aa11", 77, 9);
    const cdp = cssCdp().cdp;
    await expect(
      handleAssert(sm, params({ visible: true, tab_id: 78 }), {
        cdp,
        tabsApi: tabs(9),
      }),
    ).resolves.toMatchObject({
      code: "permission_denied",
      data: { reason: "attached_tab_scope" },
    });
    expect(cdp.send).not.toHaveBeenCalled();

    const abort = new AbortController();
    abort.abort();
    await expect(
      handleAssert(sm, params({ visible: true, tab_id: 77 }), {
        cdp,
        tabsApi: tabs(9),
        signal: abort.signal,
      }),
    ).resolves.toMatchObject({ code: "cancelled" });
  });

  it("rejects invalid assertion shapes and URL regexes", async () => {
    const sm = await manager();
    await expect(
      handleAssert(sm, params({ visible: true, hidden: true }), {
        cdp: cssCdp().cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ code: "invalid_params" });

    const invalidRegex = params({ url_matches: "[" });
    delete invalidRegex.target;
    await expect(
      handleAssert(sm, invalidRegex, {
        cdp: cssCdp().cdp,
        tabsApi: tabs(),
      }),
    ).resolves.toMatchObject({ code: "invalid_params" });
  });
});
