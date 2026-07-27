import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ControlOverlay } from "../ControlOverlay";

describe("ControlOverlay", () => {
  afterEach(() => {
    cleanup();
  });

  it("sets pointer-events none on blocker and pill when automationBypass is true", () => {
    const { container } = render(
      <ControlOverlay
        visible={true}
        interrupting={false}
        automationBypass={true}
        onInterrupt={() => {}}
      />,
    );

    const blocker = container.querySelector("[data-slot='control-overlay']")?.nextElementSibling;
    expect(blocker).toBeTruthy();
    expect((blocker as HTMLElement).style.pointerEvents).toBe("none");

    const stopBtn = container.querySelector("[data-slot='control-overlay-stop-all']");
    expect(stopBtn).toBeTruthy();
    expect((stopBtn as HTMLElement).style.pointerEvents).toBe("none");
  });

  it("uses pointer-events auto on blocker when automationBypass is false", () => {
    const { container } = render(
      <ControlOverlay
        visible={true}
        interrupting={false}
        automationBypass={false}
        onInterrupt={() => {}}
      />,
    );

    const blocker = container.querySelector("[data-slot='control-overlay']")?.nextElementSibling;
    expect(blocker).toBeTruthy();
    expect((blocker as HTMLElement).style.pointerEvents).toBe("auto");
  });

  it("expands and collapses AI operation logs", () => {
    const { container } = render(
      <ControlOverlay
        visible={true}
        interrupting={false}
        automationBypass={false}
        operationLogs={[
          {
            id: "rpc-1",
            sessionId: "abcd",
            method: "tool.click",
            status: "succeeded",
            detail: "role=button name=Save",
            startedAtMs: Date.now(),
            durationMs: 12,
          },
        ]}
        onInterrupt={() => {}}
      />,
    );

    expect(container.querySelector("[data-slot='control-overlay-log-panel']")).toBeNull();
    const toggle = container.querySelector("[data-slot='control-overlay-log-toggle']");
    expect(toggle?.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle!);
    expect(toggle?.getAttribute("aria-expanded")).toBe("true");
    expect(container.querySelector("[data-slot='control-overlay-log-panel']")).toBeTruthy();
    expect(screen.getByText("role=button name=Save")).toBeTruthy();
    expect(
      container.querySelector("[data-slot='control-overlay-log-row']")?.getAttribute("data-status"),
    ).toBe("succeeded");

    fireEvent.click(toggle!);
    expect(container.querySelector("[data-slot='control-overlay-log-panel']")).toBeNull();
  });

  it("shows an empty state when the log panel has no operations", () => {
    const { container } = render(
      <ControlOverlay
        visible={true}
        interrupting={false}
        automationBypass={false}
        onInterrupt={() => {}}
      />,
    );
    fireEvent.click(container.querySelector("[data-slot='control-overlay-log-toggle']")!);
    expect(container.querySelector("[data-slot='control-overlay-log-empty']")).toBeTruthy();
  });
});
