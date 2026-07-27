import { describe, expect, it } from "vitest";
import {
  isOverlayAgentOverlayResetMessage,
  isOverlayOperationLogsMessage,
  OVERLAY_AGENT_OVERLAY_RESET,
  OVERLAY_MSG_INTERRUPT,
  OVERLAY_OPERATION_LOGS,
  type OverlayInterruptRequest,
  type OverlayInterruptResponse,
} from "@/lib/overlay-bridge";

describe("OVERLAY_MSG_INTERRUPT", () => {
  it("constant matches the wire string content scripts will send", () => {
    expect(OVERLAY_MSG_INTERRUPT).toBe("overlay.interrupt");
  });

  it("OverlayInterruptRequest type carries kind + sessionId", () => {
    const req: OverlayInterruptRequest = {
      kind: OVERLAY_MSG_INTERRUPT,
      sessionId: "sess-1",
    };
    expect(req.kind).toBe("overlay.interrupt");
    expect(req.sessionId).toBe("sess-1");
  });

  it("OverlayInterruptResponse carries ok flag", () => {
    const res: OverlayInterruptResponse = { ok: true };
    expect(res.ok).toBe(true);
  });
});

describe("isOverlayOperationLogsMessage", () => {
  it("accepts a valid session log snapshot", () => {
    expect(
      isOverlayOperationLogsMessage({
        type: OVERLAY_OPERATION_LOGS,
        sessionId: "sess-1",
        logs: [
          {
            id: "rpc-1",
            sessionId: "sess-1",
            method: "tool.click",
            status: "succeeded",
            startedAtMs: 1,
          },
        ],
      }),
    ).toBe(true);
  });

  it("rejects malformed entries", () => {
    expect(
      isOverlayOperationLogsMessage({
        type: OVERLAY_OPERATION_LOGS,
        sessionId: "sess-1",
        logs: [{ id: "rpc-1", status: "unknown" }],
      }),
    ).toBe(false);
  });
});

describe("isOverlayAgentOverlayResetMessage", () => {
  it("accepts reset messages with a session id", () => {
    expect(
      isOverlayAgentOverlayResetMessage({
        type: OVERLAY_AGENT_OVERLAY_RESET,
        sessionId: "sess-1",
      }),
    ).toBe(true);
  });

  it("rejects reset messages without a session id", () => {
    expect(
      isOverlayAgentOverlayResetMessage({
        type: OVERLAY_AGENT_OVERLAY_RESET,
      }),
    ).toBe(false);
  });
});
