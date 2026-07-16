import { afterEach, describe, expect, it } from "vitest";
import { createIsolatedOverlayRoot } from "../isolated-overlay-root";

describe("createIsolatedOverlayRoot", () => {
  afterEach(() => {
    document.documentElement.innerHTML = "<head></head><body></body>";
  });

  it("does not add a second body to the page document structure", () => {
    document.body.innerHTML = '<main><button type="button">Business action</button></main>';

    const overlay = createIsolatedOverlayRoot(document, ":root { color: red; }");
    const internalButton = document.createElement("button");
    internalButton.textContent = "Stop agent";
    overlay.container.append(internalButton);

    expect(document.querySelectorAll("body")).toHaveLength(1);
    expect(document.querySelector("tabstride-overlay body")).toBeNull();
    expect(document.querySelectorAll("button")).toHaveLength(1);
    expect(document.body.textContent).toContain("Business action");
    expect(document.body.textContent).not.toContain("Stop agent");
  });

  it("uses an open, body-less, internally marked shadow root", () => {
    const overlay = createIsolatedOverlayRoot(document, "");

    expect(overlay.host.shadowRoot).not.toBeNull();
    expect(overlay.host.shadowRoot?.querySelector("body")).toBeNull();
    expect(overlay.host.shadowRoot?.querySelector("style")).not.toBeNull();
    expect(overlay.host.shadowRoot?.querySelector(".tabstride-overlay-root")).toBe(
      overlay.container,
    );
    expect(overlay.host.hasAttribute("aria-hidden")).toBe(false);
    expect(overlay.host.hasAttribute("role")).toBe(false);
    expect(overlay.host.getAttribute("data-tabstride-overlay")).toBe("");
    expect(overlay.host.getAttribute("data-tabstride-internal")).toBe("overlay");
    expect(overlay.container.getAttribute("data-tabstride-internal")).toBe("overlay-root");
  });

  it("removes the complete isolated tree", () => {
    const overlay = createIsolatedOverlayRoot(document, "");

    overlay.remove();

    expect(overlay.host.isConnected).toBe(false);
    expect(document.querySelector("tabstride-overlay")).toBeNull();
  });
});
