import type { CdpRunner } from "@/tools/shared";

export interface DocumentIdentity {
  id: string;
  version: number;
  url?: string;
}

export interface CachedValue<T> {
  document: DocumentIdentity;
  value: T;
}

/** Per-session, per-tab cache. Entries are only reusable at an exact DOM version. */
export class DocumentCache {
  readonly locators = new Map<string, CachedValue<unknown>>();
  readonly snapshots = new Map<string, CachedValue<unknown>>();
  readonly overlayBackendNodeIds = new Map<string, Set<number>>();

  clear(): void {
    this.locators.clear();
    this.snapshots.clear();
    this.overlayBackendNodeIds.clear();
  }
}

const DOCUMENT_VERSION_EXPRESSION = `(() => {
  const key = "__tabstrideDocumentVersion";
  let state = globalThis[key];
  if (
    !state ||
    typeof state !== "object" ||
    typeof state.id !== "string" ||
    !Number.isSafeInteger(state.version)
  ) {
    state = {
      id: String(performance.timeOrigin) + ":" + Math.random().toString(36).slice(2),
      version: 1
    };
    Object.defineProperty(globalThis, key, { value: state, configurable: true });
    const ignored = (node) => {
      const element = node instanceof Element ? node : node?.parentElement;
      return Boolean(element?.closest?.("tabstride-overlay, [data-tabstride-overlay]"));
    };
    const observer = new MutationObserver((mutations) => {
      if (mutations.some((mutation) =>
        !ignored(mutation.target) ||
        Array.from(mutation.addedNodes).some((node) => !ignored(node)) ||
        Array.from(mutation.removedNodes).some((node) => !ignored(node))
      )) state.version += 1;
    });
    observer.observe(document, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true
    });
  }
  return { id: state.id, version: state.version, url: location.href };
})()`;

/**
 * Read the current document identity. Returns null for old/test CDP runners
 * that do not support Runtime.evaluate; callers then safely bypass caching.
 */
export async function readDocumentIdentity(
  cdp: CdpRunner,
  tabId: number,
): Promise<DocumentIdentity | null> {
  try {
    const response = await cdp.send<{ result?: { value?: unknown } }>(tabId, "Runtime.evaluate", {
      expression: DOCUMENT_VERSION_EXPRESSION,
      returnByValue: true,
      awaitPromise: false,
    });
    const value = response.result?.value;
    if (
      value &&
      typeof value === "object" &&
      typeof (value as DocumentIdentity).id === "string" &&
      Number.isSafeInteger((value as DocumentIdentity).version)
    ) {
      return value as DocumentIdentity;
    }
  } catch {
    // Cache is an optimisation. Never turn unsupported instrumentation into
    // a tool failure.
  }
  return null;
}

export function sameDocument(a: DocumentIdentity, b: DocumentIdentity): boolean {
  return a.id === b.id && a.version === b.version;
}
