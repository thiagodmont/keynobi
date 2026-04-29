import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { showToast } from "@/components/ui";
import type { LogcatEntry } from "@/lib/tauri-api";
import {
  buildLogEntryDetailFilterToken,
  type LogEntryDetailFilterField,
  type LogEntryDetailFilterMode,
} from "@/lib/logcat-query";
import { getLevelConfig } from "./logcat-levels";
import styles from "./LogEntryDetailPanel.module.css";

function formatEntry(e: LogcatEntry): string {
  const pkg = e.package ? `[${e.package}] ` : "";
  return `${e.timestamp}  ${e.level.toUpperCase()}  ${pkg}${e.tag}: ${e.message}`;
}

interface LogEntryDetailPanelProps {
  entry: LogcatEntry;
  onClose: () => void;
  onAddFilter?: (filter: { token: string; mode: LogEntryDetailFilterMode }) => void;
}

interface FilterMenuState {
  token: string;
  x: number;
  y: number;
}

function MetaCell(props: {
  label: string;
  value: string;
  valueStyle?: Record<string, string>;
  borderRight?: boolean;
  borderLeft?: boolean;
  filterField?: LogEntryDetailFilterField;
  filterValue?: unknown;
  onFilterClick?: (field: LogEntryDetailFilterField, value: unknown, target: HTMLElement) => void;
}) {
  const canFilter = () => props.filterField && props.value !== "—" && props.onFilterClick;

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
      <Show
        when={canFilter()}
        fallback={
          <div class={styles.cellValue} style={props.valueStyle}>
            {props.value}
          </div>
        }
      >
        <button
          type="button"
          class={styles.cellValueButton}
          style={props.valueStyle}
          title={`Filter by ${props.label}`}
          onClick={(e) => {
            e.stopPropagation();
            props.onFilterClick?.(
              props.filterField!,
              props.filterValue ?? props.value,
              e.currentTarget
            );
          }}
        >
          {props.value}
        </button>
      </Show>
    </div>
  );
}

export function LogEntryDetailPanel(props: LogEntryDetailPanelProps) {
  let menuRef: HTMLDivElement | undefined;
  const cfg = () => getLevelConfig(props.entry.level);
  const [filterMenu, setFilterMenu] = createSignal<FilterMenuState | null>(null);

  function menuPositionFor(target: HTMLElement): { x: number; y: number } {
    const rect = target.getBoundingClientRect();
    const menuWidth = 150;
    const menuHeight = 72;
    const maxX = Math.max(8, window.innerWidth - menuWidth - 8);
    const maxY = Math.max(8, window.innerHeight - menuHeight - 8);
    return {
      x: Math.min(Math.max(8, rect.left), maxX),
      y: Math.min(Math.max(8, rect.bottom + 4), maxY),
    };
  }

  function openFilterMenu(
    field: LogEntryDetailFilterField,
    value: unknown,
    target: HTMLElement
  ): void {
    if (!props.onAddFilter) return;
    const token = buildLogEntryDetailFilterToken(field, value);
    if (!token) return;
    setFilterMenu({ token, ...menuPositionFor(target) });
  }

  function selectedMessageText(target: HTMLElement): string | null {
    const selection = window.getSelection?.();
    const selected = selection?.toString().trim();
    if (!selection || !selected) return null;

    const anchorInside = selection.anchorNode ? target.contains(selection.anchorNode) : false;
    const focusInside = selection.focusNode ? target.contains(selection.focusNode) : false;
    return anchorInside || focusInside ? selected : null;
  }

  function openMessageFilterMenu(target: HTMLElement): void {
    openFilterMenu("message", selectedMessageText(target) ?? props.entry.message, target);
  }

  function addFilter(mode: LogEntryDetailFilterMode): void {
    const menu = filterMenu();
    if (!menu) return;
    props.onAddFilter?.({ token: menu.token, mode });
    setFilterMenu(null);
  }

  function copyEntry() {
    navigator.clipboard.writeText(formatEntry(props.entry)).then(() => {
      showToast("Copied to clipboard", "info");
    });
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    if (filterMenu()) {
      setFilterMenu(null);
      return;
    }
    props.onClose();
  }

  function handleDocumentMouseDown(e: MouseEvent) {
    if (!filterMenu()) return;
    if (menuRef?.contains(e.target as globalThis.Node)) return;
    setFilterMenu(null);
  }

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleDocumentMouseDown);
  });
  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
    document.removeEventListener("mousedown", handleDocumentMouseDown);
  });

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
        <MetaCell
          label="Tag"
          value={props.entry.tag}
          borderRight
          filterField="tag"
          onFilterClick={openFilterMenu}
        />
        <MetaCell
          label="Package"
          value={props.entry.package ?? "—"}
          borderRight
          filterField="package"
          filterValue={props.entry.package}
          onFilterClick={openFilterMenu}
        />
        <MetaCell
          label="Level"
          value={props.entry.level.toUpperCase()}
          valueStyle={{ color: cfg().color }}
          filterField="level"
          filterValue={props.entry.level}
          onFilterClick={openFilterMenu}
        />
      </div>

      <div class={styles.grid}>
        <MetaCell
          label="PID"
          value={String(props.entry.pid ?? "—")}
          borderRight
          filterField="pid"
          filterValue={props.entry.pid}
          onFilterClick={openFilterMenu}
        />
        <MetaCell
          label="TID"
          value={String(props.entry.tid ?? "—")}
          borderRight
          filterField="tid"
          filterValue={props.entry.tid}
          onFilterClick={openFilterMenu}
        />
        <MetaCell
          label="Time"
          value={props.entry.timestamp}
          filterField="time"
          onFilterClick={openFilterMenu}
        />
      </div>

      <div class={styles.messageArea}>
        <pre
          class={styles.messageValue}
          role={props.onAddFilter ? "button" : undefined}
          tabIndex={props.onAddFilter ? 0 : undefined}
          title={props.onAddFilter ? "Filter by message" : undefined}
          onClick={(e) => {
            e.stopPropagation();
            openMessageFilterMenu(e.currentTarget);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Enter" && e.key !== " ") return;
            e.preventDefault();
            openMessageFilterMenu(e.currentTarget);
          }}
        >
          {props.entry.message}
        </pre>
      </div>

      <Show when={filterMenu()}>
        {(menu) => (
          <div
            ref={menuRef}
            class={styles.filterMenu}
            role="menu"
            style={{ left: `${menu().x}px`, top: `${menu().y}px` }}
          >
            <button
              type="button"
              role="menuitem"
              class={styles.filterMenuItem}
              onClick={() => addFilter("and")}
            >
              Add as AND
            </button>
            <button
              type="button"
              role="menuitem"
              class={styles.filterMenuItem}
              onClick={() => addFilter("or")}
            >
              Add as OR
            </button>
          </div>
        )}
      </Show>
    </div>
  );
}
