import { Match, Show, Switch, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import {
  Alert,
  type AlertVariant,
  Button,
  DockedPanel,
  Icon,
  MenuList,
  MenuListItem,
  MetadataCell,
  MetadataGrid,
  showToast,
} from "@/components/ui";
import type { RetraceStatus } from "@/bindings";
import { formatError, retraceCrash, type LogcatEntry, type RetraceOutcome } from "@/lib/tauri-api";
import {
  buildLogEntryDetailFilterToken,
  type LogEntryDetailFilterField,
  type LogEntryDetailFilterMode,
} from "@/lib/logcat-query";
import { copyToClipboard } from "@/utils/clipboard";
import { formatLogcatEntry } from "./logcat-entry-format";
import { getLevelConfig } from "./logcat-levels";
import styles from "./LogEntryDetailPanel.module.css";

interface LogEntryDetailPanelProps {
  entry: LogcatEntry;
  onClose: () => void;
  onAddFilter?: (filter: { token: string; mode: LogEntryDetailFilterMode }) => void;
}

type RetraceState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "done"; outcome: RetraceOutcome }
  | { kind: "error"; message: string };

const RETRACE_TITLES: Record<RetraceStatus, string> = {
  retraced: "Deobfuscated",
  unavailable: "Retrace not available",
  refused: "Not deobfuscated",
  failed: "Retrace failed",
};

const RETRACE_VARIANTS: Record<RetraceStatus, AlertVariant> = {
  retraced: "success",
  unavailable: "info",
  refused: "warning",
  failed: "error",
};

interface FilterMenuState {
  token: string;
  x: number;
  y: number;
}

function FilterableMetadataCell(props: {
  label: string;
  value: string;
  valueStyle?: Record<string, string>;
  filterField?: LogEntryDetailFilterField;
  filterValue?: unknown;
  onFilterClick?: (field: LogEntryDetailFilterField, value: unknown, target: HTMLElement) => void;
}) {
  const canFilter = () => props.filterField && props.value !== "—" && props.onFilterClick;

  return (
    <MetadataCell
      label={props.label}
      value={props.value}
      valueStyle={props.valueStyle}
      title={`Filter by ${props.label}`}
      onClick={
        canFilter()
          ? (target) =>
              props.onFilterClick?.(props.filterField!, props.filterValue ?? props.value, target)
          : undefined
      }
    />
  );
}

export function LogEntryDetailPanel(props: LogEntryDetailPanelProps) {
  let menuRef: HTMLDivElement | undefined;
  const cfg = () => getLevelConfig(props.entry.level);
  const [filterMenu, setFilterMenu] = createSignal<FilterMenuState | null>(null);
  const [retrace, setRetrace] = createSignal<RetraceState>({ kind: "idle" });
  let retraceRequest = 0;

  // A result belongs to one crash; another crash starts over and drops any
  // answer still on its way.
  createEffect(
    on(
      () => props.entry.crashGroupId,
      () => {
        retraceRequest++;
        setRetrace({ kind: "idle" });
      },
      { defer: true }
    )
  );

  async function deobfuscate(): Promise<void> {
    const crashGroupId = props.entry.crashGroupId;
    if (crashGroupId === null) return;
    const request = ++retraceRequest;
    setRetrace({ kind: "loading" });
    try {
      const outcome = await retraceCrash(crashGroupId);
      if (request === retraceRequest) setRetrace({ kind: "done", outcome });
    } catch (e) {
      if (request === retraceRequest) setRetrace({ kind: "error", message: formatError(e) });
    }
  }

  function copyStack(trace: string): void {
    copyToClipboard(trace).then(() => {
      showToast("Deobfuscated stack copied", "info");
    });
  }

  const retraceOutcome = () => {
    const state = retrace();
    return state.kind === "done" ? state.outcome : null;
  };
  const retraceError = () => {
    const state = retrace();
    return state.kind === "error" ? state.message : null;
  };

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
    return anchorInside && focusInside ? selected : null;
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
    copyToClipboard(formatLogcatEntry(props.entry)).then(() => {
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
    <DockedPanel
      title="Entry Detail"
      maxHeight="30vh"
      actions={
        <>
          <Show when={props.entry.crashGroupId !== null}>
            <Button
              variant="ghost"
              size="xs"
              tone="accent"
              loading={retrace().kind === "loading"}
              onClick={() => void deobfuscate()}
              title="Deobfuscate this crash with the R8 mapping of the build Keynobi installed"
            >
              {retrace().kind === "loading" ? "Deobfuscating…" : "Deobfuscate"}
            </Button>
          </Show>
          <Button variant="ghost" size="xs" tone="muted" onClick={copyEntry} title="Copy">
            <Icon name="copy" size={12} /> Copy
          </Button>
          <Button
            variant="ghost"
            size="xs"
            tone="muted"
            onClick={() => props.onClose()}
            title="Close"
          >
            <Icon name="close" size={12} />
          </Button>
        </>
      }
      bodyClass={styles.body}
    >
      <MetadataGrid>
        <FilterableMetadataCell
          label="Tag"
          value={props.entry.tag}
          filterField="tag"
          onFilterClick={openFilterMenu}
        />
        <FilterableMetadataCell
          label="Package"
          value={props.entry.package ?? "—"}
          filterField="package"
          filterValue={props.entry.package}
          onFilterClick={openFilterMenu}
        />
        <FilterableMetadataCell
          label="Level"
          value={props.entry.level.toUpperCase()}
          valueStyle={{ color: cfg().color }}
          filterField="level"
          filterValue={props.entry.level}
          onFilterClick={openFilterMenu}
        />
      </MetadataGrid>

      <MetadataGrid>
        <FilterableMetadataCell
          label="PID"
          value={String(props.entry.pid ?? "—")}
          filterField="pid"
          filterValue={props.entry.pid}
          onFilterClick={openFilterMenu}
        />
        <FilterableMetadataCell
          label="TID"
          value={String(props.entry.tid ?? "—")}
          filterField="tid"
          filterValue={props.entry.tid}
          onFilterClick={openFilterMenu}
        />
        <FilterableMetadataCell
          label="Time"
          value={props.entry.timestamp}
          filterField="time"
          onFilterClick={openFilterMenu}
        />
      </MetadataGrid>

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
        <Switch>
          <Match when={retraceOutcome()}>
            {(outcome) => (
              <div class={styles.retrace}>
                <Alert
                  variant={RETRACE_VARIANTS[outcome().status]}
                  title={RETRACE_TITLES[outcome().status]}
                  action={
                    outcome().status === "retraced" ? (
                      <Button
                        variant="ghost"
                        size="xs"
                        tone="muted"
                        onClick={() => copyStack(outcome().trace)}
                      >
                        <Icon name="copy" size={12} /> Copy stack
                      </Button>
                    ) : undefined
                  }
                >
                  {outcome().status === "retraced" ? outcome().summary : outcome().reason}
                </Alert>
                <Show when={outcome().status === "retraced"}>
                  <pre class={styles.messageValue} aria-label="Deobfuscated stack">
                    {outcome().trace}
                  </pre>
                </Show>
              </div>
            )}
          </Match>
          <Match when={retraceError()}>
            {(message) => (
              <div class={styles.retrace}>
                <Alert variant="error" title="Retrace failed">
                  {message()}
                </Alert>
              </div>
            )}
          </Match>
        </Switch>
      </div>

      <Show when={filterMenu()}>
        {(menu) => (
          <MenuList
            listRef={(el) => {
              menuRef = el;
            }}
            class={styles.filterMenu}
            role="menu"
            surface="floating"
            style={{ left: `${menu().x}px`, top: `${menu().y}px` }}
          >
            <MenuListItem role="menuitem" onClick={() => addFilter("and")}>
              Add as AND
            </MenuListItem>
            <MenuListItem role="menuitem" onClick={() => addFilter("or")}>
              Add as OR
            </MenuListItem>
          </MenuList>
        )}
      </Show>
    </DockedPanel>
  );
}
