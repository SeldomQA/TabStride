import { RiArrowDownSLine, RiArrowUpSLine, RiFileList3Line } from "@remixicon/react";
import { useTranslation } from "@tabstride/i18n/react";
import { useState } from "react";
import type { OverlayOperationLogEntry } from "@/lib/overlay-bridge";

export function OperationLogPanel({ logs }: { logs: OverlayOperationLogEntry[] }) {
  const { t } = useTranslation("extension");
  const [expanded, setExpanded] = useState(false);
  const operationLabels: Record<string, string> = {
    "tool.session_start": t("controlOverlay.operations.session_start"),
    "tool.session_stop": t("controlOverlay.operations.session_stop"),
    "tool.navigate": t("controlOverlay.operations.navigate"),
    "tool.navigate_back": t("controlOverlay.operations.navigate_back"),
    "tool.navigate_forward": t("controlOverlay.operations.navigate_forward"),
    "tool.reload": t("controlOverlay.operations.reload"),
    "tool.snapshot": t("controlOverlay.operations.snapshot"),
    "tool.screenshot": t("controlOverlay.operations.screenshot"),
    "tool.get_html": t("controlOverlay.operations.get_html"),
    "tool.console": t("controlOverlay.operations.console"),
    "tool.click": t("controlOverlay.operations.click"),
    "tool.fill": t("controlOverlay.operations.fill"),
    "tool.press": t("controlOverlay.operations.press"),
    "tool.select": t("controlOverlay.operations.select"),
    "tool.assert": t("controlOverlay.operations.assert"),
    "tool.evaluate": t("controlOverlay.operations.evaluate"),
    "tool.wait_for_navigation": t("controlOverlay.operations.wait_for_navigation"),
    "tool.request_help": t("controlOverlay.operations.request_help"),
    "tool.tab_list": t("controlOverlay.operations.tab_list"),
    "tool.tab_create": t("controlOverlay.operations.tab_create"),
    "tool.tab_close": t("controlOverlay.operations.tab_close"),
    "tool.tab_select": t("controlOverlay.operations.tab_select"),
    "tool.tab_borrow": t("controlOverlay.operations.tab_borrow"),
    "tool.tab_return": t("controlOverlay.operations.tab_return"),
  };

  return (
    <section
      className="overflow-hidden rounded-xl border border-border/80 bg-card/60"
      data-slot="popup-operation-logs"
    >
      <button
        type="button"
        className="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left"
        aria-expanded={expanded}
        data-slot="popup-operation-logs-toggle"
        onClick={() => setExpanded((value) => !value)}
      >
        <span className="flex min-w-0 items-center gap-2">
          <RiFileList3Line className="size-4 shrink-0 text-primary" aria-hidden />
          <span className="truncate text-xs font-semibold">{t("controlOverlay.logTitle")}</span>
        </span>
        <span className="flex shrink-0 items-center gap-1.5">
          <span
            className="inline-flex min-w-5 items-center justify-center rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-semibold text-primary"
            data-slot="popup-operation-logs-count"
          >
            {logs.length}
          </span>
          {expanded ? (
            <RiArrowUpSLine className="size-4 text-muted-foreground" aria-hidden />
          ) : (
            <RiArrowDownSLine className="size-4 text-muted-foreground" aria-hidden />
          )}
        </span>
      </button>

      {expanded ? (
        <div
          className="max-h-52 overflow-y-auto border-t border-border/70"
          data-slot="popup-operation-logs-list"
        >
          {logs.length === 0 ? (
            <p
              className="px-3 py-5 text-center text-[11px] text-muted-foreground"
              data-slot="popup-operation-logs-empty"
            >
              {t("controlOverlay.logEmpty")}
            </p>
          ) : (
            [...logs]
              .reverse()
              .map((entry) => (
                <PopupOperationLogRow
                  key={`${entry.sessionId}:${entry.id}`}
                  entry={entry}
                  operationLabel={
                    operationLabels[entry.method] ?? entry.method.replace(/^tool\./, "")
                  }
                  statusLabel={
                    entry.status === "running"
                      ? t("controlOverlay.logStatus.running")
                      : entry.status === "succeeded"
                        ? t("controlOverlay.logStatus.succeeded")
                        : t("controlOverlay.logStatus.failed")
                  }
                />
              ))
          )}
        </div>
      ) : null}
    </section>
  );
}

function PopupOperationLogRow({
  entry,
  operationLabel,
  statusLabel,
}: {
  entry: OverlayOperationLogEntry;
  operationLabel: string;
  statusLabel: string;
}) {
  const statusClass =
    entry.status === "running"
      ? "bg-amber-500"
      : entry.status === "succeeded"
        ? "bg-emerald-500"
        : "bg-destructive";
  const statusTextClass =
    entry.status === "running"
      ? "text-amber-600 dark:text-amber-400"
      : entry.status === "succeeded"
        ? "text-emerald-600 dark:text-emerald-400"
        : "text-destructive";
  const time = new Date(entry.startedAtMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  return (
    <div
      className="grid grid-cols-[6px_minmax(0,1fr)_auto] items-start gap-2 border-b border-border/50 px-3 py-2 last:border-b-0"
      data-slot="popup-operation-log-row"
      data-status={entry.status}
    >
      <span className={`mt-1.5 size-1.5 rounded-full ${statusClass}`} aria-hidden />
      <div className="min-w-0">
        <div className="flex min-w-0 items-baseline gap-1.5">
          <span className="truncate text-[11px] font-semibold">{operationLabel}</span>
          <span className={`shrink-0 text-[9px] font-medium ${statusTextClass}`}>
            {statusLabel}
          </span>
          {entry.durationMs !== undefined ? (
            <span className="shrink-0 text-[9px] text-muted-foreground">{entry.durationMs} ms</span>
          ) : null}
        </div>
        {entry.detail ? (
          <div className="truncate font-mono text-[9px] text-muted-foreground" title={entry.detail}>
            {entry.detail}
          </div>
        ) : entry.errorCode ? (
          <div className="truncate font-mono text-[9px] text-destructive">{entry.errorCode}</div>
        ) : null}
      </div>
      <time
        className="text-[9px] text-muted-foreground"
        dateTime={new Date(entry.startedAtMs).toISOString()}
      >
        {time}
      </time>
    </div>
  );
}
