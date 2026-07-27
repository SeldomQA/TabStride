import { describe, expect, it } from "vitest";
import {
  MAX_OPERATION_LOGS_PER_SESSION,
  MAX_RECENT_OPERATION_LOGS,
  OperationLogStore,
  operationSessionId,
  summarizeOperation,
} from "../operation-log";

describe("OperationLogStore", () => {
  it("updates one operation in place and caps each session", () => {
    const store = new OperationLogStore();
    for (let index = 0; index < MAX_OPERATION_LOGS_PER_SESSION + 3; index += 1) {
      store.upsert({
        id: `rpc-${index}`,
        sessionId: "abcd",
        method: "tool.snapshot",
        status: "running",
        startedAtMs: index,
      });
    }
    expect(store.list("abcd")).toHaveLength(MAX_OPERATION_LOGS_PER_SESSION);
    expect(store.list("abcd")[0]?.id).toBe("rpc-3");

    store.upsert({
      id: "rpc-52",
      sessionId: "abcd",
      method: "tool.snapshot",
      status: "succeeded",
      startedAtMs: 52,
      durationMs: 8,
    });
    expect(store.list("abcd").at(-1)).toMatchObject({
      id: "rpc-52",
      status: "succeeded",
      durationMs: 8,
    });
  });

  it("returns copies and clears one session", () => {
    const store = new OperationLogStore();
    store.upsert({
      id: "rpc-1",
      sessionId: "abcd",
      method: "tool.click",
      status: "running",
      startedAtMs: 1,
    });
    const listed = store.list("abcd");
    listed[0]!.status = "failed";
    expect(store.list("abcd")[0]?.status).toBe("running");
    store.clear("abcd");
    expect(store.list("abcd")).toEqual([]);
    expect(store.listRecent()).toHaveLength(1);
  });

  it("keeps a capped cross-session history for the popup", () => {
    const store = new OperationLogStore();
    for (let index = 0; index < MAX_RECENT_OPERATION_LOGS + 2; index += 1) {
      store.upsert({
        id: `rpc-${index}`,
        sessionId: `session-${index}`,
        method: "tool.snapshot",
        status: "succeeded",
        startedAtMs: index,
      });
    }
    expect(store.listRecent()).toHaveLength(MAX_RECENT_OPERATION_LOGS);
    expect(store.listRecent()[0]?.id).toBe("rpc-2");
  });
});

describe("operation log summaries", () => {
  it("keeps useful targets while excluding fill values and scripts", () => {
    expect(
      summarizeOperation("tool.fill", {
        target: { label: "Email" },
        value: "secret@example.com",
      }),
    ).toBe("label=Email");
    expect(
      summarizeOperation("tool.evaluate", {
        expression: "document.cookie",
      }),
    ).toBeUndefined();
    expect(
      summarizeOperation("tool.click", {
        target: { role: "button", name: "Save" },
      }),
    ).toBe("role=button name=Save");
    expect(
      summarizeOperation("tool.navigate", {
        url: "https://example.com/account?token=secret#private",
      }),
    ).toBe("https://example.com/account");
  });

  it("extracts only valid session ids", () => {
    expect(operationSessionId({ session_id: "abcd" })).toBe("abcd");
    expect(operationSessionId({ session_id: "" })).toBeNull();
    expect(operationSessionId({})).toBeNull();
  });
});
