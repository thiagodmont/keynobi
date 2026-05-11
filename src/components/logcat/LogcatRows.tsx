import { type JSX, Show, createMemo, createSignal } from "solid-js";
import type { LogcatEntry } from "@/lib/tauri-api";
import { Badge, Icon, showToast } from "@/components/ui";
import { openInStudio } from "@/lib/tauri-api";
import { healthState } from "@/stores/health.store";
import { isProjectFrame, parseStackFrame } from "@/lib/logcat-stack-frame";
import { LEVEL_CONFIG } from "./logcat-levels";
import { rowFocusMarked, rowInSelectionRange } from "./logcat-row-selection";
import styles from "./LogcatRows.module.css";

const ENTRY_FLAGS = {
  CRASH: 1 << 0,
  ANR: 1 << 1,
  JSON_BODY: 1 << 2,
  NATIVE_CRASH: 1 << 3,
} as const;

export function LogcatVirtualRow(props: {
  entry: LogcatEntry;
  getIndex: () => number;
  getSelectionRange: () => [number, number] | null;
  getAnchor: () => number | null;
  getEnd: () => number | null;
  getDetailEntry: () => LogcatEntry | null;
  getJsonEntry: () => LogcatEntry | null;
  expandedContext: boolean;
  onRowClick: (e: MouseEvent) => void;
  onContextMenu: (e: MouseEvent) => void;
  onJsonClick: (e: MouseEvent) => void;
}): JSX.Element {
  const index = createMemo(() => props.getIndex());
  const inSelectionRange = createMemo(() =>
    rowInSelectionRange(index(), props.getSelectionRange())
  );

  const focusMarked = createMemo(() =>
    rowFocusMarked(
      index(),
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
      expandedContext={props.expandedContext}
      onClick={props.onRowClick}
      onContextMenu={props.onContextMenu}
      onJsonClick={props.onJsonClick}
    />
  );
}

export function SeparatorRow(props: {
  entry: LogcatEntry;
  expandedContext?: boolean;
  onContextMenu?: (e: MouseEvent) => void;
}): JSX.Element {
  const isDied = () => props.entry.kind === "processDied";
  const pkg = () => props.entry.package ?? props.entry.tag;
  const label = () => (isDied() ? `${pkg()} PROCESS DIED` : `${pkg()} PROCESS RESTARTED`);

  return (
    <div
      class={[styles.separatorRow, isDied() ? styles.separatorDied : styles.separatorRestarted]
        .concat(props.expandedContext ? styles.expandedContext : [])
        .filter(Boolean)
        .join(" ")}
      data-expanded-context={props.expandedContext ? "true" : undefined}
      onContextMenu={(e) => props.onContextMenu?.(e)}
    >
      <span class={styles.separatorRule} />
      <span class={styles.separatorLabel}>
        <Icon name={isDied() ? "warning" : "play"} size={10} /> {label()}
      </span>
      <span class={styles.separatorTimestamp}>{props.entry.timestamp}</span>
      <span class={styles.separatorRule} />
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
        type="button"
        onClick={handleOpen}
        title={
          studioReady()
            ? `Open ${frame()!.filename}:${frame()!.line} in Android Studio`
            : "Install the studio command to enable jump-to-line (see Health Panel)"
        }
        class={[
          styles.studioJump,
          studioReady() ? styles.studioReady : "",
          opening() ? styles.studioOpening : "",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        <Icon name={opening() ? "spinner" : "external-link"} size={10} /> Studio
      </button>
    </Show>
  );
}

function LogcatRow(props: {
  entry: LogcatEntry;
  inSelectionRange: boolean;
  focusMarked: boolean;
  jsonSelected: boolean;
  expandedContext: boolean;
  onClick: (e: MouseEvent) => void;
  onContextMenu: (e: MouseEvent) => void;
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
    if (props.expandedContext) return "color-mix(in srgb, var(--success) 10%, transparent)";
    if (props.entry.isCrash) return "color-mix(in srgb, var(--error) 12%, transparent)";
    if (hasAnr()) return "color-mix(in srgb, var(--warning) 8%, transparent)";
    return cfg().bg;
  }

  return (
    <div
      onClick={(e) => props.onClick(e)}
      onContextMenu={(e) => props.onContextMenu(e)}
      title="Click to copy · Shift+click to select range"
      class={styles.row}
      data-expanded-context={props.expandedContext ? "true" : undefined}
      style={{
        background: defaultRowBackground(),
        "border-left": props.focusMarked
          ? "4px solid var(--accent)"
          : props.expandedContext
            ? "4px solid var(--success)"
            : `2px solid ${semanticBorderColor()}`,
      }}
      onMouseEnter={(e) => {
        if (!props.focusMarked && !props.inSelectionRange && !props.jsonSelected) {
          (e.currentTarget as HTMLElement).style.background = "rgba(255,255,255,0.04)";
        }
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = defaultRowBackground();
      }}
    >
      <span class={styles.timestamp}>{props.entry.timestamp}</span>

      <span
        class={styles.level}
        style={{
          color: cfg().color,
        }}
      >
        {cfg().label}
      </span>

      <Show when={props.entry.package}>
        <span class={styles.packageName} title={props.entry.package ?? ""}>
          {props.entry.package}
        </span>
      </Show>

      <span class={styles.tag} title={props.entry.tag}>
        {props.entry.tag}
      </span>

      <Show when={hasJson()}>
        <Badge
          size="xs"
          variant="info"
          onClick={(e) => props.onJsonClick(e)}
          title="Click to view formatted JSON"
        >
          {"{}"}
        </Badge>
      </Show>

      <Show when={hasAnr()}>
        <Badge size="xs" variant="warning">
          ANR
        </Badge>
      </Show>

      <span
        class={styles.message}
        style={{
          color: props.entry.isCrash
            ? "var(--error)"
            : hasAnr()
              ? "var(--warning)"
              : props.entry.level.toLowerCase() === "info"
                ? "var(--text-primary)"
                : cfg().color,
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
