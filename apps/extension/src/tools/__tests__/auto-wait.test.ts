import { describe, expect, it, vi } from "vitest";
import { waitForPageChange } from "../auto-wait";
import type { CdpRunner } from "../shared";

describe("waitForPageChange", () => {
  it("uses a page MutationObserver as the primary wake-up", async () => {
    const send = vi.fn(async (_tabId: number, method: string, params?: object) => {
      expect(method).toBe("Runtime.evaluate");
      expect(params).toMatchObject({ awaitPromise: true, returnByValue: true });
      expect(String((params as { expression?: string }).expression)).toContain("MutationObserver");
      return { result: { value: "mutation" } };
    }) as unknown as CdpRunner["send"];

    await expect(waitForPageChange({ send }, 7, { maxWaitMs: 100 })).resolves.toBe("mutation");
  });

  it("wakes on a relevant CDP event for the same tab", async () => {
    let listener:
      | ((source: chrome.debugger.Debuggee, method: string, params: unknown) => void)
      | undefined;
    const dispose = vi.fn();
    const send = vi.fn(
      async () =>
        await new Promise(() => {
          // Mutation wait intentionally stays pending; the CDP event wins.
        }),
    ) as unknown as CdpRunner["send"];
    const cdp: CdpRunner = {
      send,
      onEvent: (handler) => {
        listener = handler;
        return { dispose };
      },
    };

    const waiting = waitForPageChange(cdp, 7, { maxWaitMs: 100 });
    listener?.({ tabId: 8 }, "Page.frameNavigated", {});
    listener?.({ tabId: 7 }, "Page.frameNavigated", {});

    await expect(waiting).resolves.toBe("page_event");
    expect(dispose).toHaveBeenCalledOnce();
  });

  it("wakes immediately when the user cancels", async () => {
    const abort = new AbortController();
    const send = vi.fn(
      async () =>
        await new Promise(() => {
          // Cancellation wins this race.
        }),
    ) as unknown as CdpRunner["send"];
    const waiting = waitForPageChange({ send }, 7, {
      maxWaitMs: 100,
      signal: abort.signal,
    });
    abort.abort();
    await expect(waiting).resolves.toBe("cancelled");
  });

  it("keeps a bounded fallback when the page execution context disappears", async () => {
    const send = vi.fn(async () => {
      throw new Error("Execution context was destroyed");
    }) as unknown as CdpRunner["send"];
    const started = performance.now();

    await expect(waitForPageChange({ send }, 7, { maxWaitMs: 20, fallbackMs: 5 })).resolves.toBe(
      "fallback",
    );
    expect(performance.now() - started).toBeGreaterThanOrEqual(4);
  });
});
