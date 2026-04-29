import { type JSX, Show, createMemo, createSignal } from "solid-js";
import type { LogcatEntry } from "@/lib/tauri-api";
import { showToast } from "@/components/ui";
import { openInStudio } from "@/lib/tauri-api";
import { healthState } from "@/stores/health.store";
import { isProjectFrame, parseStackFrame } from "@/lib/logcat-query";
import { LEVEL_CONFIG } from "./logcat-levels";
import { rowFocusMarked, rowInSelectionRange } from "./logcat-row-selection";

export const ROW_HEIGHT = 20;

const ENTRY_FLAGS = {
  CRASH: 1 << 0,
  ANR: 1 << 1,
  JSON_BODY: 1 << 2,
  NATIVE_CRASH: 1 << 3,
} as const;

export function LogcatVirtualRow(props: {
  entry: LogcatEntry;
  index: number;
  getSelectionRange: () => [number, number] | null;
  getAnchor: () => number | null;
  getEnd: () => number | null;
  getDetailEntry: () => LogcatEntry | null;
  getJsonEntry: () => LogcatEntry | null;
  onRowClick: (e: MouseEvent) => void;
  onJsonClick: (e: MouseEvent) => void;
}): JSX.Element {
  const inSelectionRange = createMemo(() =>
    rowInSelectionRange(props.index, props.getSelectionRange())
  );

  const focusMarked = createMemo(() =>
    rowFocusMarked(
      props.index,
      props.getAnchor(),
      props.getEnd(),
      props.getDetailEntry(),
      props.entry.id
    )
  );

  const jsonSelected = createMemo(() => props.getJsonEntry()?.id === props.entry.id);

  return (
    <LogcatRow
      entry={props.entry}
      inSelectionRange={inSelectionRange()}
      focusMarked={focusMarked()}
      jsonSelected={jsonSelected()}
      onClick={props.onRowClick}
      onJsonClick={props.onJsonClick}
    />
  );
}

export function SeparatorRow(props: { entry: LogcatEntry }): JSX.Element {
  const isDied = () => props.entry.kind === "processDied";
  const pkg = () => props.entry.package ?? props.entry.tag;
  const label = () => (isDied() ? `⚠  ${pkg()} PROCESS DIED` : `▶  ${pkg()} PROCESS RESTARTED`);

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        height: `${ROW_HEIGHT}px`,
        "min-height": `${ROW_HEIGHT}px`,
        padding: "0 8px",
        background: isDied()
          ? "color-mix(in srgb, var(--error) 10%, transparent)"
          : "color-mix(in srgb, var(--success) 7%, transparent)",
        "border-top": `1px solid ${isDied() ? "color-mix(in srgb, var(--error) 30%, transparent)" : "color-mix(in srgb, var(--success) 20%, transparent)"}`,
        "border-bottom": `1px solid ${isDied() ? "color-mix(in srgb, var(--error) 30%, transparent)" : "color-mix(in srgb, var(--success) 20%, transparent)"}`,
        overflow: "hidden",
      }}
    >
      <span
        style={{
          flex: "1",
          "border-top": `1px dashed ${isDied() ? "color-mix(in srgb, var(--error) 30%, transparent)" : "color-mix(in srgb, var(--success) 20%, transparent)"}`,
        }}
      />
      <span
        style={{
          "font-size": "10px",
          color: isDied() ? "var(--error)" : "var(--success)",
          "font-weight": "600",
          "white-space": "nowrap",
          padding: "0 10px",
          "letter-spacing": "0.04em",
        }}
      >
        {label()}
      </span>
      <span style={{ "font-size": "10px", color: "var(--text-muted)", "flex-shrink": "0" }}>
        {props.entry.timestamp}
      </span>
      <span
        style={{
          flex: "1",
          "border-top": `1px dashed ${isDied() ? "color-mix(in srgb, var(--error) 30%, transparent)" : "color-mix(in srgb, var(--success) 20%, transparent)"}`,
        }}
      />
    </div>
  );
}

function StudioJumpButton(props: { message: string }): JSX.Element {
  const frame = () => {
    const f = parseStackFrame(props.message);
    if (f && !isProjectFrame(f.classPath)) return null;
    return f;
  };
  const studioReady = () => healthState.systemReport?.studioCommandFound === true;
  const [hovered, setHovered] = createSignal(false);
  const [opening, setOpening] = createSignal(false);

  const handleOpen = async (e: MouseEvent) => {
    e.stopPropagation();
    const f = frame();
    if (!f) return;
    if (!studioReady()) {
      showToast("Install the studio command — see Health Panel for setup instructions", "warning");
      return;
    }
    setOpening(true);
    try {
      await openInStudio(f.packagePath, f.filename, f.line);
    } catch (err: unknown) {
      showToast(String(err), "error");
    } finally {
      setOpening(false);
    }
  };

  return (
    <Show when={frame() !== null}>
      <button
        onClick={handleOpen}
        title={
          studioReady()
            ? `Open ${frame()!.filename}:${frame()!.line} in Android Studio`
            : "Install the studio command to enable jump-to-line (see Health Panel)"
        }
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        style={{
          "flex-shrink": "0",
          display: "inline-flex",
          "align-items": "center",
          gap: "3px",
          padding: "1px 6px",
          "border-radius": "3px",
          border: `1px solid ${studioReady() ? "color-mix(in srgb, var(--info) 40%, transparent)" : "color-mix(in srgb, var(--text-muted) 30%, transparent)"}`,
          background: hovered()
            ? studioReady()
              ? "color-mix(in srgb, var(--info) 15%, transparent)"
              : "color-mix(in srgb, var(--text-muted) 10%, transparent)"
            : "transparent",
          color: studioReady() ? "var(--info)" : "var(--text-disabled)",
          cursor: opening() ? "wait" : studioReady() ? "pointer" : "not-allowed",
          "font-size": "9px",
          "font-family": "var(--font-ui)",
          "font-weight": "500",
          opacity: opening() ? "0.6" : "1",
          transition: "background 0.1s, opacity 0.1s",
          "white-space": "nowrap",
        }}
      >
        {opening() ? "⟳" : "↗"} Studio
      </button>
    </Show>
  );
}

function LogcatRow(props: {
  entry: LogcatEntry;
  inSelectionRange: boolean;
  focusMarked: boolean;
  jsonSelected: boolean;
  onClick: (e: MouseEvent) => void;
  onJsonClick: (e: MouseEvent) => void;
}): JSX.Element {
  const cfg = () =>
    LEVEL_CONFIG[props.entry.level as keyof typeof LEVEL_CONFIG] ?? LEVEL_CONFIG.unknown;
  const hasJson = () => (props.entry.flags & ENTRY_FLAGS.JSON_BODY) !== 0;
  const hasAnr = () => (props.entry.flags & ENTRY_FLAGS.ANR) !== 0;
  const inCrashGroup = () => props.entry.crashGroupId !== null && !props.entry.isCrash;

  const semanticBorderColor = () => {
    if (props.entry.isCrash) return "var(--error)";
    if (hasAnr()) return "var(--warning)";
    if (inCrashGroup()) return "color-mix(in srgb, var(--error) 40%, transparent)";
    return "transparent";
  };

  const ACCENT_RANGE_BG = "rgba(var(--accent-rgb, 59,130,246),0.14)";
  const ACCENT_FOCUS_BG = "rgba(var(--accent-rgb, 59,130,246),0.28)";

  function defaultRowBackground(): string {
    if (props.focusMarked) return ACCENT_FOCUS_BG;
    if (props.inSelectionRange) return ACCENT_RANGE_BG;
    if (props.jsonSelected) return "color-mix(in srgb, var(--info) 12%, transparent)";
    if (props.entry.isCrash) return "color-mix(in srgb, var(--error) 12%, transparent)";
    if (hasAnr()) return "color-mix(in srgb, var(--warning) 8%, transparent)";
    return cfg().bg;
  }

  return (
    <div
      onClick={(e) => props.onClick(e)}
      title="Click to copy · Shift+click to select range"
      style={{
        display: "flex",
        "align-items": "center",
        gap: "6px",
        padding: "0 10px",
        height: `${ROW_HEIGHT}px`,
        "min-height": `${ROW_HEIGHT}px`,
        background: defaultRowBackground(),
        "border-left": props.focusMarked
          ? "4px solid var(--accent)"
          : `2px solid ${semanticBorderColor()}`,
        overflow: "hidden",
        cursor: "pointer",
      }}
      onMouseEnter={(e) => {
        if (!props.focusMarked && !props.inSelectionRange && !props.jsonSelected) {
          (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.04)";
        }
      }}
      onMouseLeave={(e) => {
        if (!props.focusMarked && !props.inSelectionRange && !props.jsonSelected) {
          (e.currentTarget as HTMLElement).style.background = props.entry.isCrash
            ? "color-mix(in srgb, var(--error) 12%, transparent)"
            : hasAnr()
              ? "color-mix(in srgb, var(--warning) 8%, transparent)"
              : cfg().bg;
        } else {
          (e.currentTarget as HTMLElement).style.background = defaultRowBackground();
        }
      }}
    >
      <span
        style={{
          color: "var(--text-disabled, #4b5563)",
          "white-space": "nowrap",
          "flex-shrink": "0",
          "font-size": "10px",
          opacity: "0.7",
        }}
      >
        {props.entry.timestamp}
      </span>

      <span
        style={{
          color: cfg().color,
          "font-weight": "700",
          "min-width": "12px",
          "text-align": "center",
          "flex-shrink": "0",
          "font-size": "11px",
        }}
      >
        {cfg().label}
      </span>

      <Show when={props.entry.package}>
        <span
          style={{
            "font-size": "9px",
            color: "var(--accent)",
            "flex-shrink": "0",
            "max-width": "90px",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
            opacity: "0.8",
          }}
          title={props.entry.package ?? ""}
        >
          {props.entry.package}
        </span>
      </Show>

      <span
        style={{
          color: "var(--text-secondary)",
          "min-width": "80px",
          "max-width": "120px",
          overflow: "hidden",
          "text-overflow": "ellipsis",
          "white-space": "nowrap",
          "flex-shrink": "0",
          "font-size": "10px",
        }}
        title={props.entry.tag}
      >
        {props.entry.tag}
      </span>

      <Show when={hasJson()}>
        <span
          onClick={(e) => props.onJsonClick(e)}
          title="Click to view formatted JSON"
          style={{
            "font-size": "9px",
            color: props.jsonSelected ? "var(--accent-fg)" : "var(--info)",
            "flex-shrink": "0",
            opacity: "1",
            "font-weight": "600",
            background: props.jsonSelected
              ? "color-mix(in srgb, var(--info) 50%, transparent)"
              : "color-mix(in srgb, var(--info) 10%, transparent)",
            padding: "0 4px",
            "border-radius": "2px",
            cursor: "pointer",
            border: "1px solid color-mix(in srgb, var(--info) 30%, transparent)",
          }}
        >
          {"{}"}
        </span>
      </Show>

      <Show when={hasAnr()}>
        <span
          style={{
            "font-size": "9px",
            color: "var(--warning)",
            "flex-shrink": "0",
            "font-weight": "600",
            background: "color-mix(in srgb, var(--warning) 10%, transparent)",
            padding: "0 3px",
            "border-radius": "2px",
          }}
        >
          ANR
        </span>
      </Show>

      <span
        style={{
          flex: "1",
          color: props.entry.isCrash
            ? "var(--error)"
            : hasAnr()
              ? "var(--warning)"
              : props.entry.level.toLowerCase() === "info"
                ? "var(--text-primary)"
                : cfg().color,
          "white-space": "nowrap",
          overflow: "hidden",
          "text-overflow": "ellipsis",
        }}
        title={props.entry.message}
      >
        {props.entry.message}
      </span>

      <Show when={inCrashGroup() || props.entry.isCrash}>
        <StudioJumpButton message={props.entry.message} />
      </Show>
    </div>
  );
}
