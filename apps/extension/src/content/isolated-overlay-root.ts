import {
  TABSTRIDE_INTERNAL_ATTRIBUTE,
  TABSTRIDE_OVERLAY_HOST_ATTRIBUTE,
  TABSTRIDE_OVERLAY_TAG_NAME,
} from "@/lib/overlay-dom";

export interface IsolatedOverlayRoot {
  host: HTMLElement;
  container: HTMLDivElement;
  remove(): void;
}

/**
 * Mount the TabStride overlay without adding a second `body` to the page's
 * composed tree.
 *
 * WXT's createShadowRootUi intentionally creates an HTML-shaped shadow tree
 * containing head/body elements. The body-less shadow root keeps TabStride
 * UI from changing the page's document structure, while the open mode keeps
 * the overlay inspectable and accessible for TabStride development and
 * extension UI testing. TabStride's own snapshots exclude the internally
 * marked subtree separately.
 */
export function createIsolatedOverlayRoot(
  document: Document,
  cssText: string,
): IsolatedOverlayRoot {
  const host = document.createElement(TABSTRIDE_OVERLAY_TAG_NAME);
  host.setAttribute(TABSTRIDE_OVERLAY_HOST_ATTRIBUTE, "");
  host.setAttribute(TABSTRIDE_INTERNAL_ATTRIBUTE, "overlay");
  // Inline important styles protect the zero-layout host from application CSS.
  // Overlay children establish their own fixed positioning and pointer events.
  host.style.setProperty("all", "initial", "important");
  host.style.setProperty("display", "block", "important");
  host.style.setProperty("position", "static", "important");

  const shadow = host.attachShadow({ mode: "open" });
  const style = document.createElement("style");
  style.setAttribute(TABSTRIDE_INTERNAL_ATTRIBUTE, "overlay-style");
  style.textContent = cssText.replaceAll(":root", ":host");

  const container = document.createElement("div");
  container.setAttribute(TABSTRIDE_INTERNAL_ATTRIBUTE, "overlay-root");
  container.className = "tabstride-overlay-root";
  shadow.append(style, container);

  document.documentElement.append(host);

  return {
    host,
    container,
    remove() {
      host.remove();
    },
  };
}
