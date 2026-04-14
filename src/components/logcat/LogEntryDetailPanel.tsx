import { onCleanup, onMount } from "solid-js";
import { showToast } from "@/components/ui";
import type { LogcatEntry } from "@/lib/tauri-api";
import { getLevelConfig } from "./logcat-levels";
import styles from "./LogEntryDetailPanel.module.css";

function formatEntry(e: LogcatEntry): string {
  const pkg = e.package ? `[${e.package}] ` : "";
  return `${e.timestamp}  ${e.level.toUpperCase()}  ${pkg}${e.tag}: ${e.message}`;
}

interface LogEntryDetailPanelProps {
  entry: LogcatEntry;
  onClose: () => void;
}

function MetaCell(props: {
  label: string;
  value: string;
  valueStyle?: Record<string, string>;
  borderRight?: boolean;
  borderLeft?: boolean;
}) {
  return (
    <div
      class={[
        styles.cell,
        props.borderRight ? styles.cellBorderRight : "",
        props.borderLeft ? styles.cellBorderLeft : "",
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <div class={styles.cellLabel}>{props.label}</div>
      <div class={styles.cellValue} style={props.valueStyle}>
        {props.value}
      </div>
    </div>
  );
}

export function LogEntryDetailPanel(props: LogEntryDetailPanelProps) {
  const cfg = () => getLevelConfig(props.entry.level);

  function copyEntry() {
    navigator.clipboard.writeText(formatEntry(props.entry)).then(() => {
      showToast("Copied to clipboard", "info");
    });
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") props.onClose();
  }

  onMount(() => document.addEventListener("keydown", handleKeyDown));
  onCleanup(() => document.removeEventListener("keydown", handleKeyDown));

  return (
    <div class={styles.panel}>
      <div class={styles.header}>
        <span class={styles.headerTitle}>Entry Detail</span>
        <div class={styles.headerActions}>
          <button class={styles.iconBtn} onClick={copyEntry} title="Copy">
            ⎘ Copy
          </button>
          <button class={styles.iconBtn} onClick={() => props.onClose()} title="Close">
            ✕
          </button>
        </div>
      </div>

      <div class={styles.grid}>
        <MetaCell label="Tag" value={props.entry.tag} borderRight />
        <MetaCell label="Package" value={props.entry.package ?? "—"} borderRight />
        <MetaCell
          label="Level"
          value={props.entry.level.toUpperCase()}
          valueStyle={{ color: cfg().color }}
        />
      </div>

      <div class={styles.grid}>
        <MetaCell label="PID" value={String(props.entry.pid ?? "—")} borderRight />
        <MetaCell label="TID" value={String(props.entry.tid ?? "—")} borderRight />
        <MetaCell label="Time" value={props.entry.timestamp} />
      </div>

      <div class={styles.messageArea}>
        <pre class={styles.messageValue}>{props.entry.message}</pre>
      </div>
    </div>
  );
}
