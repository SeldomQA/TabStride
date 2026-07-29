import { describe, expect, it, vi } from "vitest";
import { SessionManager } from "@/session-manager/manager";
import type { ConsoleResult, RpcError } from "@/transport/types";
import { enrichFailureEvidence } from "../failure-evidence";
import type { CdpRunner } from "../shared";

const TINY_PNG =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGBgAAAABQABh6FO1AAAAABJRU5ErkJggg==";

function agentWindow() {
  return {
    create: vi.fn(async () => 100),
    remove: vi.fn(async () => {}),
    ensureActiveTab: vi.fn(async () => {}),
  };
}

async function sessionManager() {
  const manager = new SessionManager({ agentWindow: agentWindow() });
  await manager.start("aa11");
  return manager;
}

function tabsApi() {
  return {
    get: vi.fn(
      async (tabId: number) => ({ id: tabId, windowId: 100, active: true }) as chrome.tabs.Tab,
    ),
    query: vi.fn(async () => [{ id: 7, windowId: 100, active: true } as chrome.tabs.Tab]),
  };
}

function evidenceCdp(): CdpRunner {
  const consoleResult: ConsoleResult = {
    tab_id: 7,
    entries: [
      {
        sequence: 1,
        kind: "console",
        level: "log",
        text: "ignored",
        truncated: false,
      },
      {
        sequence: 2,
        kind: "exception",
        level: "error",
        text: "boom",
        truncated: false,
      },
    ],
    next_since: 2,
    truncated: false,
  };
  return {
    send: vi.fn(async (_tabId: number, method: string) => {
      switch (method) {
        case "Runtime.evaluate":
          return { result: { value: "https://example.test/todos" } };
        case "Accessibility.enable":
          return {};
        case "Accessibility.getFullAXTree":
          return {
            nodes: [
              {
                nodeId: "root",
                role: { value: "RootWebArea" },
                name: { value: "Todos" },
              },
            ],
          };
        case "DOM.getDocument":
          return { root: { nodeId: 1 } };
        case "DOM.querySelector":
          return { nodeId: 0 };
        case "Page.captureScreenshot":
          return { data: TINY_PNG };
        default:
          throw new Error(`unexpected CDP call ${method}`);
      }
    }) as unknown as CdpRunner["send"],
    ensureConsoleCapture: vi.fn(async () => {}),
    consoleEntriesSince: vi.fn(() => consoleResult),
  };
}

describe("minimal failure evidence", () => {
  it("attaches page artifacts and keeps the original error", async () => {
    const manager = await sessionManager();
    const original: RpcError = {
      code: "timeout",
      message: "click target stayed obscured",
      data: {
        reason: "element_obscured",
        failed_check: "receives_events",
        match_count: 1,
        actionability_history: [
          {
            attempt: 1,
            elapsed_ms: 10,
            match_count: 1,
            failed_check: "receives_events",
            state: { visible: true, receives_events: false },
          },
        ],
        timing: { locator_ms: 4, wait_ms: 10, cdp_ms: 3 },
      },
    };

    const result = await enrichFailureEvidence(
      manager,
      { session_id: "aa11", target: { css: "#save" } },
      original,
      { cdp: evidenceCdp(), tabsApi: tabsApi() },
    );

    expect(result).toMatchObject({
      code: "timeout",
      message: "click target stayed obscured",
      data: {
        reason: "element_obscured",
        evidence: {
          locator: { css: "#save" },
          match_count: 1,
          last_failed_check: "receives_events",
          current_url: "https://example.test/todos",
          snapshot: { text: 'RootWebArea "Todos"', ref_count: 0, truncated: false },
          screenshot: { image_base64: TINY_PNG, width: 1, height: 1, format: "png" },
          recent_console_errors: [{ text: "boom", level: "error" }],
          timing: { locator_ms: 4, wait_ms: 10, cdp_ms: 3 },
          collection_errors: [],
        },
      },
    });
    expect(result.data?.evidence).toMatchObject({
      actionability_history: [{ attempt: 1, failed_check: "receives_events" }],
      timing: {
        evidence_ms: expect.any(Number),
        total_ms: expect.any(Number),
      },
    });
  });

  it("records artifact failures without replacing the business failure", async () => {
    const manager = await sessionManager();
    const cdp: CdpRunner = {
      send: vi.fn(async () => {
        throw new Error("debugger detached");
      }) as unknown as CdpRunner["send"],
    };
    const result = await enrichFailureEvidence(
      manager,
      { session_id: "aa11", target: { css: "#save" } },
      { code: "cdp_failed", message: "original failure" },
      { cdp, tabsApi: tabsApi() },
    );

    expect(result.code).toBe("cdp_failed");
    expect(result.message).toBe("original failure");
    expect(result.data?.evidence).toMatchObject({
      locator: { css: "#save" },
      collection_errors: expect.arrayContaining([
        expect.stringContaining("url:"),
        expect.stringContaining("snapshot:"),
        expect.stringContaining("screenshot:"),
        expect.stringContaining("console:"),
      ]),
    });
  });

  it("does not delay cancellation for evidence collection", async () => {
    const manager = await sessionManager();
    const cdp = evidenceCdp();
    const result = await enrichFailureEvidence(
      manager,
      { session_id: "aa11", target: { css: "#save" } },
      { code: "cancelled", message: "stopped" },
      { cdp, tabsApi: tabsApi() },
    );
    expect(result.data?.evidence).toBeUndefined();
    expect(cdp.send).not.toHaveBeenCalled();
  });
});
