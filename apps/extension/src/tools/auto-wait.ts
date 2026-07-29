// Event-driven wait primitive shared by Locator re-resolution and
// Actionability. DOM mutations and relevant CDP events wake immediately; a
// short bounded fallback keeps CSS animations / geometry-only changes moving
// without relying on a long fixed sleep.

import type { CdpRunner } from "./shared";

export type AutoWaitWakeReason = "mutation" | "page_event" | "fallback" | "cancelled";

const DEFAULT_FALLBACK_MS = 100;
const PAGE_WAKE_EVENTS = new Set([
  "DOM.documentUpdated",
  "DOM.attributeModified",
  "DOM.characterDataModified",
  "DOM.childNodeCountUpdated",
  "DOM.childNodeInserted",
  "DOM.childNodeRemoved",
  "Page.frameNavigated",
  "Page.lifecycleEvent",
  "Runtime.executionContextsCleared",
]);

export async function waitForPageChange(
  cdp: CdpRunner,
  tabId: number,
  options: {
    maxWaitMs: number;
    signal?: AbortSignal;
    fallbackMs?: number;
  },
): Promise<AutoWaitWakeReason> {
  if (options.signal?.aborted) return "cancelled";

  const maxWaitMs = Math.max(1, options.maxWaitMs);
  const fallbackMs = Math.max(1, Math.min(options.fallbackMs ?? DEFAULT_FALLBACK_MS, maxWaitMs));
  let eventSubscription: { dispose(): void } | undefined;
  let abortHandler: (() => void) | undefined;

  const mutation = waitForMutation(cdp, tabId, fallbackMs);
  const pageEvent = new Promise<AutoWaitWakeReason>((resolve) => {
    if (!cdp.onEvent) return;
    eventSubscription = cdp.onEvent((source, method) => {
      if (source.tabId === tabId && PAGE_WAKE_EVENTS.has(method)) resolve("page_event");
    });
  });
  const cancelled = new Promise<AutoWaitWakeReason>((resolve) => {
    if (!options.signal) return;
    abortHandler = () => resolve("cancelled");
    options.signal.addEventListener("abort", abortHandler, { once: true });
  });

  try {
    return await Promise.race([mutation, pageEvent, cancelled]);
  } finally {
    eventSubscription?.dispose();
    if (abortHandler && options.signal) {
      options.signal.removeEventListener("abort", abortHandler);
    }
  }
}

async function waitForMutation(
  cdp: CdpRunner,
  tabId: number,
  fallbackMs: number,
): Promise<AutoWaitWakeReason> {
  try {
    const response = await cdp.send<{ result?: { value?: AutoWaitWakeReason } }>(
      tabId,
      "Runtime.evaluate",
      {
        expression: `(() => new Promise((resolve) => {
          let settled = false;
          let observer;
          let timer;
          const watchedEvents = ['resize', 'scroll', 'transitionend', 'animationend'];
          const finish = (reason) => {
            if (settled) return;
            settled = true;
            observer?.disconnect();
            clearTimeout(timer);
            for (const event of watchedEvents) {
              window.removeEventListener(event, onPageEvent, true);
            }
            resolve(reason);
          };
          const onPageEvent = () => finish('mutation');
          observer = new MutationObserver(() => finish('mutation'));
          const root = document.documentElement || document;
          observer.observe(root, {
            subtree: true,
            childList: true,
            attributes: true,
            characterData: true
          });
          for (const event of watchedEvents) {
            window.addEventListener(event, onPageEvent, true);
          }
          timer = setTimeout(() => finish('fallback'), ${fallbackMs});
        }))()`,
        awaitPromise: true,
        returnByValue: true,
      },
    );
    return response.result?.value === "mutation" ? "mutation" : "fallback";
  } catch {
    // Navigation can destroy the execution context while the observer is
    // pending. Keep the bounded fallback instead of spinning immediately;
    // a concurrent CDP page event can still win the outer race.
    return await new Promise<AutoWaitWakeReason>((resolve) => {
      setTimeout(() => resolve("fallback"), fallbackMs);
    });
  }
}

export const __testing__ = {
  DEFAULT_FALLBACK_MS,
  PAGE_WAKE_EVENTS,
};
