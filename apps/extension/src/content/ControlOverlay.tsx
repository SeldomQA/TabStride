import {
  RiArrowDownSLine,
  RiArrowUpSLine,
  RiFileList3Line,
  RiStopCircleLine,
} from "@remixicon/react";
import { useTranslation } from "@tabstride/i18n/react";
import { useEffect, useRef, useState } from "react";
import type { OverlayOperationLogEntry } from "@/lib/overlay-bridge";
import logoUrl from "../../assets/logo.png";

export interface ControlOverlayProps {
  visible: boolean;
  interrupting: boolean;
  automationBypass: boolean;
  operationLogs?: OverlayOperationLogEntry[];
  onInterrupt: () => void;
}

export function ControlOverlay({
  visible,
  interrupting,
  automationBypass,
  operationLogs = [],
  onInterrupt,
}: ControlOverlayProps) {
  const { t } = useTranslation("extension");
  const [show, setShow] = useState(false);
  const [logsExpanded, setLogsExpanded] = useState(false);
  const logEndRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (visible) {
      const raf = requestAnimationFrame(() => setShow(true));
      return () => cancelAnimationFrame(raf);
    }
    setShow(false);
    setLogsExpanded(false);
  }, [visible]);

  useEffect(() => {
    if (logsExpanded) {
      logEndRef.current?.scrollIntoView?.({ block: "nearest" });
    }
  }, [logsExpanded, operationLogs]);

  if (!visible) return null;

  const pointerEvents = automationBypass ? "none" : "auto";
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
    <>
      <style>{`
        @keyframes tabstride-breathe {
          0%, 100% {
            box-shadow: inset 0 0 20px 4px rgba(249,115,22,0.25);
          }
          50% {
            box-shadow: inset 0 0 40px 8px rgba(249,115,22,0.5);
          }
        }
      `}</style>

      <div
        data-slot="control-overlay"
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 2147483646,
          pointerEvents: "none",
          animation: "tabstride-breathe 3s ease-in-out infinite",
          opacity: show ? 1 : 0,
          transition: "opacity 300ms ease-out",
        }}
      />

      <div
        onPointerDown={(event) => {
          if (automationBypass) return;
          event.preventDefault();
          event.stopPropagation();
        }}
        onClick={(event) => {
          if (automationBypass) return;
          event.preventDefault();
          event.stopPropagation();
        }}
        style={{
          position: "fixed",
          inset: 0,
          zIndex: 2147483646,
          pointerEvents,
          background: "transparent",
          opacity: show ? 1 : 0,
          transition: "opacity 300ms ease-out",
        }}
      />

      <div
        style={{
          position: "fixed",
          bottom: 32,
          left: "50%",
          transform: "translateX(-50%)",
          zIndex: 2147483647,
          pointerEvents,
          width: "min(440px, calc(100vw - 32px))",
          display: "flex",
          flexDirection: "column",
          alignItems: "stretch",
          gap: 10,
          opacity: show ? 1 : 0,
          transition: "opacity 300ms ease-out",
          fontFamily:
            '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif',
        }}
      >
        {logsExpanded ? (
          <div
            data-slot="control-overlay-log-panel"
            style={{
              overflow: "hidden",
              border: "1px solid rgba(249, 115, 22, 0.18)",
              borderRadius: 18,
              background: "rgba(255, 255, 255, 0.98)",
              boxShadow: "0 18px 48px rgba(124,45,18,0.18), 0 4px 12px rgba(0,0,0,0.08)",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "12px 14px 10px",
                borderBottom: "1px solid #f3f4f6",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <RiFileList3Line size={17} color="#ea580c" aria-hidden />
                <span style={{ color: "#292524", fontSize: 14, fontWeight: 650 }}>
                  {t("controlOverlay.logTitle")}
                </span>
              </div>
              <span style={{ color: "#9a3412", fontSize: 12 }}>
                {t("controlOverlay.logCount", { count: operationLogs.length })}
              </span>
            </div>
            <div
              data-slot="control-overlay-log-list"
              style={{
                maxHeight: 250,
                overflowY: "auto",
                overscrollBehavior: "contain",
                padding: operationLogs.length > 0 ? "4px 0" : "24px 16px",
              }}
            >
              {operationLogs.length === 0 ? (
                <div
                  data-slot="control-overlay-log-empty"
                  style={{ color: "#78716c", fontSize: 13, textAlign: "center" }}
                >
                  {t("controlOverlay.logEmpty")}
                </div>
              ) : (
                operationLogs.map((entry) => (
                  <OperationLogRow
                    key={entry.id}
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
              <div ref={logEndRef} />
            </div>
          </div>
        ) : null}

        <div
          style={{
            alignSelf: "center",
            display: "flex",
            alignItems: "center",
            gap: 10,
            maxWidth: "100%",
            backgroundColor: "#fff",
            borderRadius: 9999,
            padding: "10px 10px 10px 18px",
            boxShadow: "0 8px 32px rgba(124,45,18,0.16), 0 2px 8px rgba(0,0,0,0.1)",
          }}
        >
          <img
            src={logoUrl}
            alt="tabstride"
            style={{ width: 24, height: 24, borderRadius: 4, flexShrink: 0 }}
          />
          <span
            style={{
              fontSize: 15,
              fontWeight: 550,
              color: "#333",
              whiteSpace: "nowrap",
              userSelect: "none",
            }}
          >
            {t("controlOverlay.status")}
          </span>
          <button
            type="button"
            data-slot="control-overlay-log-toggle"
            aria-expanded={logsExpanded}
            onClick={() => setLogsExpanded((expanded) => !expanded)}
            style={{
              pointerEvents,
              display: "flex",
              alignItems: "center",
              gap: 4,
              border: "none",
              borderRadius: 9999,
              padding: "8px 10px",
              color: "#9a3412",
              background: "#fff7ed",
              cursor: "pointer",
              fontSize: 13,
              fontWeight: 600,
              whiteSpace: "nowrap",
              lineHeight: 1,
            }}
          >
            <RiFileList3Line size={16} aria-hidden />
            {t("controlOverlay.log")}
            {operationLogs.length > 0 ? (
              <span
                data-slot="control-overlay-log-badge"
                style={{
                  minWidth: 18,
                  height: 18,
                  padding: "0 5px",
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  borderRadius: 999,
                  color: "#fff",
                  background: "#f97316",
                  fontSize: 11,
                  fontWeight: 700,
                }}
              >
                {operationLogs.length}
              </span>
            ) : null}
            {logsExpanded ? (
              <RiArrowDownSLine size={16} aria-hidden />
            ) : (
              <RiArrowUpSLine size={16} aria-hidden />
            )}
          </button>
          <button
            type="button"
            data-slot="control-overlay-stop-all"
            disabled={interrupting}
            onClick={onInterrupt}
            style={{
              pointerEvents,
              display: "flex",
              alignItems: "center",
              gap: 6,
              border: "none",
              borderRadius: 9999,
              padding: "8px 18px 8px 14px",
              fontSize: 14,
              fontWeight: 600,
              color: "#fff",
              backgroundColor: interrupting ? "#9ca3af" : "#f97316",
              cursor: interrupting ? "default" : "pointer",
              opacity: interrupting ? 0.7 : 1,
              transition: "background-color 150ms ease-out, opacity 150ms ease-out",
              whiteSpace: "nowrap",
              lineHeight: 1,
            }}
          >
            <RiStopCircleLine size={18} color="#fff" />
            {interrupting ? t("controlOverlay.interrupting") : t("controlOverlay.interrupt")}
          </button>
        </div>
      </div>
    </>
  );
}

function OperationLogRow({
  entry,
  operationLabel,
  statusLabel,
}: {
  entry: OverlayOperationLogEntry;
  operationLabel: string;
  statusLabel: string;
}) {
  const statusColor =
    entry.status === "running" ? "#f59e0b" : entry.status === "succeeded" ? "#10b981" : "#ef4444";
  const time = new Date(entry.startedAtMs).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  return (
    <div
      data-slot="control-overlay-log-row"
      data-status={entry.status}
      style={{
        display: "grid",
        gridTemplateColumns: "8px minmax(0, 1fr) auto",
        alignItems: "start",
        gap: 9,
        padding: "9px 14px",
        borderBottom: "1px solid #f5f5f4",
      }}
    >
      <span
        aria-hidden
        style={{
          width: 8,
          height: 8,
          marginTop: 5,
          borderRadius: 999,
          background: statusColor,
          boxShadow: entry.status === "running" ? `0 0 0 3px ${statusColor}22` : "none",
        }}
      />
      <div style={{ minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "baseline", gap: 7 }}>
          <span style={{ color: "#292524", fontSize: 13, fontWeight: 650 }}>{operationLabel}</span>
          <span style={{ color: statusColor, fontSize: 11, fontWeight: 600 }}>{statusLabel}</span>
          {entry.errorCode ? (
            <span style={{ color: "#b91c1c", fontSize: 11 }}>{entry.errorCode}</span>
          ) : null}
          {entry.durationMs !== undefined ? (
            <span style={{ color: "#a8a29e", fontSize: 11 }}>{entry.durationMs} ms</span>
          ) : null}
        </div>
        {entry.detail ? (
          <div
            title={entry.detail}
            style={{
              marginTop: 2,
              overflow: "hidden",
              color: "#78716c",
              fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
              fontSize: 11,
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {entry.detail}
          </div>
        ) : null}
      </div>
      <time
        dateTime={new Date(entry.startedAtMs).toISOString()}
        style={{ color: "#a8a29e", fontSize: 11 }}
      >
        {time}
      </time>
    </div>
  );
}
