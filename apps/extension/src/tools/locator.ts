// Unified strict Locator resolver shared by click/fill/press/select.

import type { SessionContext } from "@/session-manager/manager";
import type { Locator, RpcError } from "@/transport/types";
import { rpcError } from "./errors";
import { type CdpRunner, isRpcError } from "./shared";
import { resolveSnapshotRef } from "./snapshot-ref";

export interface ResolvedLocator {
  backendNodeId: number;
  usedTarget: Locator;
  matchCount: 1;
}

const STRATEGY_KEYS = ["ref", "css", "role", "label", "placeholder", "text", "testId"] as const;

export function validateLocator(locator: Locator | undefined): RpcError | null {
  if (!locator || typeof locator !== "object") {
    return { code: "invalid_params", message: "target must be a locator object" };
  }
  const selected = STRATEGY_KEYS.filter((key) => {
    const value = locator[key];
    return typeof value === "string" && value.trim().length > 0;
  });
  if (selected.length !== 1) {
    return {
      code: "invalid_params",
      message: "target requires exactly one non-empty locator strategy",
    };
  }
  if ((typeof locator.role === "string") !== (typeof locator.name === "string")) {
    return {
      code: "invalid_params",
      message: "role locators require both role and name",
    };
  }
  if (
    typeof locator.role === "string" &&
    (locator.role.trim().length === 0 || locator.name?.trim().length === 0)
  ) {
    return {
      code: "invalid_params",
      message: "role locators require non-empty role and name",
    };
  }
  if ((locator.ref !== undefined || locator.css !== undefined) && locator.exact !== undefined) {
    return {
      code: "invalid_params",
      message: "exact is only valid for semantic locators",
    };
  }
  return null;
}

export async function resolveLocator(
  cdp: CdpRunner,
  ctx: SessionContext,
  tabId: number,
  locator: Locator | undefined,
): Promise<ResolvedLocator | RpcError> {
  const invalid = validateLocator(locator);
  if (invalid) return invalid;
  const target = locator as Locator;
  if (target.ref) {
    const resolved = resolveSnapshotRef(ctx, target.ref, tabId);
    if (isRpcError(resolved)) {
      return {
        ...resolved,
        data: {
          ...resolved.data,
          locator: target,
          match_count: 0,
        },
      };
    }
    return {
      backendNodeId: resolved.backendNodeId,
      usedTarget: { ref: resolved.refKey },
      matchCount: 1,
    };
  }
  if (target.css) {
    return resolveCss(cdp, tabId, target);
  }
  return resolveSemantic(cdp, tabId, target);
}

async function resolveCss(
  cdp: CdpRunner,
  tabId: number,
  locator: Locator,
): Promise<ResolvedLocator | RpcError> {
  try {
    const doc = await cdp.send<{ root?: { nodeId?: number } }>(tabId, "DOM.getDocument", {
      depth: 0,
    });
    const rootNodeId = doc.root?.nodeId;
    if (typeof rootNodeId !== "number") {
      return { code: "cdp_failed", message: "DOM.getDocument returned no root nodeId" };
    }
    const found = await cdp.send<{ nodeIds?: number[] }>(tabId, "DOM.querySelectorAll", {
      nodeId: rootNodeId,
      selector: locator.css,
    });
    const nodeIds = found.nodeIds ?? [];
    const strictError = strictMatchError(locator, nodeIds.length);
    if (strictError) return strictError;
    const described = await cdp.send<{ node?: { backendNodeId?: number } }>(
      tabId,
      "DOM.describeNode",
      { nodeId: nodeIds[0] },
    );
    const backendNodeId = described.node?.backendNodeId;
    if (typeof backendNodeId !== "number") {
      return { code: "cdp_failed", message: "DOM.describeNode returned no backendNodeId" };
    }
    return { backendNodeId, usedTarget: { css: locator.css }, matchCount: 1 };
  } catch (error) {
    return {
      code: "invalid_params",
      message: `invalid CSS locator: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

async function resolveSemantic(
  cdp: CdpRunner,
  tabId: number,
  locator: Locator,
): Promise<ResolvedLocator | RpcError> {
  const objectGroup = `tabstride-locator-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  try {
    const evaluated = await cdp.send<{
      result?: { objectId?: string };
      exceptionDetails?: { text?: string };
    }>(tabId, "Runtime.evaluate", {
      expression: semanticLocatorExpression(locator),
      returnByValue: false,
      objectGroup,
    });
    if (evaluated.exceptionDetails) {
      return {
        code: "cdp_failed",
        message: evaluated.exceptionDetails.text ?? "semantic locator evaluation failed",
      };
    }
    const arrayObjectId = evaluated.result?.objectId;
    if (!arrayObjectId) {
      return { code: "cdp_failed", message: "semantic locator returned no remote array" };
    }
    const properties = await cdp.send<{
      result?: Array<{ name?: string; value?: { objectId?: string } }>;
    }>(tabId, "Runtime.getProperties", {
      objectId: arrayObjectId,
      ownProperties: true,
    });
    const matches = (properties.result ?? [])
      .filter((property) => /^(0|[1-9]\d*)$/.test(property.name ?? ""))
      .sort((left, right) => Number(left.name) - Number(right.name))
      .map((property) => property.value?.objectId)
      .filter((objectId): objectId is string => typeof objectId === "string");
    const strictError = strictMatchError(locator, matches.length);
    if (strictError) return strictError;
    const described = await cdp.send<{ node?: { backendNodeId?: number } }>(
      tabId,
      "DOM.describeNode",
      { objectId: matches[0] },
    );
    const backendNodeId = described.node?.backendNodeId;
    if (typeof backendNodeId !== "number") {
      return { code: "cdp_failed", message: "DOM.describeNode returned no backendNodeId" };
    }
    return { backendNodeId, usedTarget: { ...locator }, matchCount: 1 };
  } catch (error) {
    return {
      code: "cdp_failed",
      message: error instanceof Error ? error.message : String(error),
    };
  } finally {
    try {
      await cdp.send(tabId, "Runtime.releaseObjectGroup", { objectGroup });
    } catch {
      // Best effort: the inspected page may have navigated during resolution.
    }
  }
}

function strictMatchError(locator: Locator, matchCount: number): RpcError | null {
  if (matchCount === 0) {
    return rpcError("not_found", "locator_not_found", "locator did not match any element", {
      locator,
      match_count: 0,
    });
  }
  if (matchCount > 1) {
    return rpcError(
      "ambiguous_target",
      "ambiguous_target",
      `locator matched ${matchCount} elements; strict matching requires exactly one`,
      { locator, match_count: matchCount },
    );
  }
  return null;
}

/**
 * Return a JavaScript expression whose value is an array of matching Elements.
 * Locator matching is intentionally independent of actionability: hidden or
 * disabled nodes still count, and the actionability phase diagnoses them later.
 */
export function semanticLocatorExpression(locator: Locator): string {
  return `((locator) => {
    const normalise = (value) => String(value ?? "").replace(/\\s+/g, " ").trim();
    const equals = (actual, expected) => {
      const left = normalise(actual);
      const right = normalise(expected);
      if (locator.exact) return left === right;
      return left.toLocaleLowerCase().includes(right.toLocaleLowerCase());
    };
    const implicitRole = (element) => {
      const explicit = element.getAttribute("role");
      if (explicit) return explicit.split(/\\s+/)[0].toLowerCase();
      const tag = element.tagName.toLowerCase();
      const type = (element.getAttribute("type") || "").toLowerCase();
      if (tag === "button" || (tag === "input" && ["button", "submit", "reset"].includes(type))) return "button";
      if (tag === "a" && element.hasAttribute("href")) return "link";
      if (tag === "textarea" || (tag === "input" && !["button", "submit", "reset", "checkbox", "radio", "hidden"].includes(type))) return "textbox";
      if (tag === "input" && type === "checkbox") return "checkbox";
      if (tag === "input" && type === "radio") return "radio";
      if (tag === "select") return "combobox";
      if (/^h[1-6]$/.test(tag)) return "heading";
      if (tag === "img") return "img";
      if (tag === "ul" || tag === "ol") return "list";
      if (tag === "li") return "listitem";
      return "";
    };
    const accessibleName = (element) => {
      const labelledBy = element.getAttribute("aria-labelledby");
      if (labelledBy) {
        const value = labelledBy.split(/\\s+/).map((id) => document.getElementById(id)?.textContent || "").join(" ");
        if (normalise(value)) return value;
      }
      const aria = element.getAttribute("aria-label");
      if (aria) return aria;
      if ("labels" in element && element.labels?.length) {
        return Array.from(element.labels).map((label) => label.textContent || "").join(" ");
      }
      if (element instanceof HTMLInputElement && ["button", "submit", "reset"].includes(element.type)) return element.value;
      return element.getAttribute("alt") || element.getAttribute("title") || element.textContent || "";
    };
    const all = Array.from(document.querySelectorAll("*"));
    if (locator.role) {
      return all.filter((element) =>
        implicitRole(element) === String(locator.role).toLowerCase() &&
        equals(accessibleName(element), locator.name)
      );
    }
    if (locator.label) {
      const controls = all.filter((element) =>
        "labels" in element &&
        Array.from(element.labels || []).some((label) => equals(label.textContent, locator.label))
      );
      const aria = all.filter((element) => equals(element.getAttribute("aria-label"), locator.label));
      return Array.from(new Set([...controls, ...aria]));
    }
    if (locator.placeholder) {
      return all.filter((element) => element.hasAttribute("placeholder") && equals(element.getAttribute("placeholder"), locator.placeholder));
    }
    if (locator.testId) {
      return all.filter((element) => element.hasAttribute("data-testid") && equals(element.getAttribute("data-testid"), locator.testId));
    }
    if (locator.text) {
      const matching = all.filter((element) => equals(element.textContent, locator.text));
      return matching.filter((element) =>
        !Array.from(element.children).some((child) => equals(child.textContent, locator.text))
      );
    }
    return [];
  })(${JSON.stringify(locator)})`;
}

export const __testing__ = { strictMatchError };
