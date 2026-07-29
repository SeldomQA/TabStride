import type { OverlayOperationLogEntry } from "./overlay-bridge";

export const MAX_OPERATION_LOGS_PER_SESSION = 50;
export const MAX_RECENT_OPERATION_LOGS = 100;

export class OperationLogStore {
  private readonly logs = new Map<string, OverlayOperationLogEntry[]>();
  private recent: OverlayOperationLogEntry[] = [];

  upsert(entry: OverlayOperationLogEntry): void {
    const current = this.logs.get(entry.sessionId) ?? [];
    const index = current.findIndex((item) => item.id === entry.id);
    const next =
      index >= 0
        ? current.map((item, itemIndex) => (itemIndex === index ? { ...entry } : item))
        : [...current, { ...entry }];
    this.logs.set(entry.sessionId, next.slice(-MAX_OPERATION_LOGS_PER_SESSION));
    const recentIndex = this.recent.findIndex(
      (item) => item.sessionId === entry.sessionId && item.id === entry.id,
    );
    this.recent =
      recentIndex >= 0
        ? this.recent.map((item, itemIndex) => (itemIndex === recentIndex ? { ...entry } : item))
        : [...this.recent, { ...entry }];
    this.recent = this.recent.slice(-MAX_RECENT_OPERATION_LOGS);
  }

  list(sessionId: string): OverlayOperationLogEntry[] {
    return (this.logs.get(sessionId) ?? []).map((entry) => ({ ...entry }));
  }

  listRecent(): OverlayOperationLogEntry[] {
    return this.recent.map((entry) => ({ ...entry }));
  }

  clear(sessionId: string): void {
    this.logs.delete(sessionId);
  }
}

export function operationSessionId(params: unknown): string | null {
  if (!params || typeof params !== "object") return null;
  const sessionId = (params as { session_id?: unknown }).session_id;
  return typeof sessionId === "string" && sessionId.length > 0 ? sessionId : null;
}

export function summarizeOperation(method: string, params: unknown): string | undefined {
  if (!params || typeof params !== "object") return undefined;
  const value = params as Record<string, unknown>;
  switch (method) {
    case "tool.navigate":
      return safeUrl(value.url);
    case "tool.click":
    case "tool.fill":
    case "tool.select":
    case "tool.assert":
      return summarizeTarget(value.target);
    case "tool.press": {
      const key = safeText(value.key);
      const target = summarizeTarget(value.target);
      return [key, target].filter(Boolean).join(" · ") || undefined;
    }
    case "tool.tab_create":
      return safeUrl(value.url);
    case "tool.tab_close":
    case "tool.tab_select":
    case "tool.tab_borrow":
    case "tool.tab_return":
      return typeof value.tab_id === "number" ? `tab ${value.tab_id}` : undefined;
    case "tool.wait_ms":
      return typeof value.duration_ms === "number" ? `${value.duration_ms} ms` : undefined;
    default:
      return undefined;
  }
}

function summarizeTarget(target: unknown): string | undefined {
  if (!target || typeof target !== "object") return undefined;
  const value = target as Record<string, unknown>;
  if (safeText(value.ref)) return `ref=${safeText(value.ref)}`;
  if (safeText(value.css)) return `css=${safeText(value.css)}`;
  if (safeText(value.role) && safeText(value.name)) {
    return `role=${safeText(value.role)} name=${safeText(value.name)}`;
  }
  for (const key of ["label", "placeholder", "text", "testId"] as const) {
    const text = safeText(value[key]);
    if (text) return `${key}=${text}`;
  }
  return undefined;
}

function safeText(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.replace(/\s+/g, " ").trim();
  if (!normalized) return undefined;
  return normalized.length > 120 ? `${normalized.slice(0, 117)}…` : normalized;
}

function safeUrl(value: unknown): string | undefined {
  const text = safeText(value);
  if (!text) return undefined;
  try {
    const url = new URL(text);
    return safeText(`${url.origin}${url.pathname}`);
  } catch {
    return text;
  }
}
