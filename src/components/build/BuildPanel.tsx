import {
  type JSX,
  Show,
  For,
  createMemo,
  createSignal,
} from "solid-js";
import { buildState, buildLogStore, isBuilding } from "@/stores/build.store";
import { runBuild, cancelBuild, jumpToBuildError } from "@/services/build.service";
import { LogViewer } from "@/components/common/LogViewer";
import type { BuildError } from "@/bindings";
import Icon from "@/components/common/Icon";

type ViewMode = "log" | "problems";

export function BuildPanel(): JSX.Element {
  const [viewMode, setViewMode] = createSignal<ViewMode>("log");
  const [running, setRunning] = createSignal(false);

  const phase = () => buildState.phase;
  const errorCount = () => buildState.errors.length;
  const warnCount = () => buildState.warnings.length;
  const durationMs = () => buildState.durationMs;

  const summaryColor = createMemo(() => {
    switch (phase()) {
      case "success": return "var(--success, #4ade80)";
      case "failed":  return "var(--error, #f87171)";
      case "cancelled": return "var(--text-muted)";
      default:        return "var(--text-secondary)";
    }
  });

  const summaryLabel = createMemo(() => {
    switch (phase()) {
      case "running": return "Building…";
      case "success": return `Build successful${formatDuration(durationMs())}`;
      case "failed":  return `Build failed${formatDuration(durationMs())} — ${errorCount()} error${errorCount() !== 1 ? "s" : ""}`;
      case "cancelled": return "Build cancelled";
      default:        return null;
    }
  });

  async function handleRun() {
    setRunning(true);
    try {
      await runBuild();
    } catch (e) {
      console.error("Build error:", e);
    } finally {
      setRunning(false);
    }
  }

  async function handleCancel() {
    await cancelBuild().catch(console.error);
  }

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        overflow: "hidden",
        background: "var(--bg-secondary)",
      }}
    >
      {/* ── Toolbar ── */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: "4px 8px",
          background: "var(--bg-tertiary)",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        {/* Run button */}
        <Show
          when={isBuilding()}
          fallback={
            <ToolbarBtn title="Run build (Cmd+R)" onClick={handleRun} disabled={running()}>
              <Icon name="play" size={13} color="#4ade80" />
            </ToolbarBtn>
          }
        >
          <ToolbarBtn title="Cancel build" onClick={handleCancel}>
            <Icon name="stop" size={13} color="#f87171" />
          </ToolbarBtn>
        </Show>

        {/* View toggle */}
        <div
          style={{
            display: "flex",
            gap: "2px",
            "margin-left": "4px",
          }}
        >
          <ViewToggleBtn
            label="Log"
            active={viewMode() === "log"}
            onClick={() => setViewMode("log")}
          />
          <ViewToggleBtn
            label={`Problems${errorCount() + warnCount() > 0 ? ` (${errorCount() + warnCount()})` : ""}`}
            active={viewMode() === "problems"}
            onClick={() => setViewMode("problems")}
          />
        </div>

        {/* Build summary */}
        <Show when={summaryLabel()}>
          <span
            style={{
              "margin-left": "8px",
              color: summaryColor(),
              "font-size": "11px",
              "white-space": "nowrap",
              overflow: "hidden",
              "text-overflow": "ellipsis",
              "flex-shrink": "1",
            }}
          >
            {summaryLabel()}
          </span>
        </Show>
      </div>

      {/* ── Content ── */}
      <div style={{ flex: "1", overflow: "hidden" }}>
        <Show when={viewMode() === "log"}>
          <LogViewer
            entries={buildLogStore.entries}
            onClear={() => buildLogStore.clearEntries()}
            showSource={false}
            emptyMessage="No build output yet — press the run button or Cmd+R"
          />
        </Show>
        <Show when={viewMode() === "problems"}>
          <ProblemsView
            errors={buildState.errors}
            warnings={buildState.warnings}
          />
        </Show>
      </div>
    </div>
  );
}

// ── Problems view ─────────────────────────────────────────────────────────────

function ProblemsView(props: {
  errors: BuildError[];
  warnings: BuildError[];
}): JSX.Element {
  const all = createMemo(() => [
    ...props.errors.map((e) => ({ ...e, _kind: "error" as const })),
    ...props.warnings.map((w) => ({ ...w, _kind: "warning" as const })),
  ]);

  // Group by file.
  const byFile = createMemo(() => {
    const map = new Map<string, typeof all extends () => Array<infer T> ? T[] : never[]>();
    for (const item of all()) {
      const key = item.file;
      if (!map.has(key)) map.set(key, [] as any);
      map.get(key)!.push(item as any);
    }
    return Array.from(map.entries());
  });

  if (all().length === 0) {
    return (
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          height: "100%",
          color: "var(--text-muted)",
          "font-size": "12px",
        }}
      >
        No build errors or warnings
      </div>
    );
  }

  return (
    <div style={{ "overflow-y": "auto", height: "100%", "font-size": "12px" }}>
      <For each={byFile()}>
        {([file, items]) => (
          <FileErrorGroup file={file} items={items as any} />
        )}
      </For>
    </div>
  );
}

function FileErrorGroup(props: {
  file: string;
  items: Array<BuildError & { _kind: "error" | "warning" }>;
}): JSX.Element {
  const [collapsed, setCollapsed] = createSignal(false);
  const shortFile = () => {
    const parts = props.file.split("/");
    return parts.slice(-2).join("/");
  };

  return (
    <div style={{ "border-bottom": "1px solid var(--border)" }}>
      {/* File header */}
      <button
        onClick={() => setCollapsed((v) => !v)}
        style={{
          display: "flex",
          "align-items": "center",
          gap: "6px",
          padding: "4px 8px",
          width: "100%",
          background: "var(--bg-tertiary)",
          border: "none",
          cursor: "pointer",
          color: "var(--text-secondary)",
          "font-size": "11px",
          "text-align": "left",
        }}
      >
        <span>{collapsed() ? "▶" : "▼"}</span>
        <span title={props.file}>{shortFile()}</span>
        <span style={{ opacity: "0.6", "margin-left": "auto" }}>
          {props.items.length}
        </span>
      </button>

      {/* Error rows */}
      <Show when={!collapsed()}>
        <For each={props.items}>
          {(item) => (
            <button
              onClick={() => jumpToBuildError(item).catch(console.error)}
              style={{
                display: "flex",
                "align-items": "flex-start",
                gap: "8px",
                padding: "4px 8px 4px 24px",
                width: "100%",
                background: "transparent",
                border: "none",
                cursor: "pointer",
                "text-align": "left",
                "border-left": `3px solid ${item._kind === "error" ? "var(--error, #f87171)" : "var(--warning, #fbbf24)"}`,
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.05))"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
            >
              <span
                style={{
                  "font-size": "10px",
                  "white-space": "nowrap",
                  color: "var(--text-muted)",
                  "flex-shrink": "0",
                  "padding-top": "1px",
                }}
              >
                {item.line}:{item.col ?? 1}
              </span>
              <span
                style={{
                  color: item._kind === "error" ? "var(--error, #f87171)" : "var(--warning, #fbbf24)",
                  "word-break": "break-word",
                  "white-space": "pre-wrap",
                  "font-size": "11px",
                  "line-height": "1.4",
                }}
              >
                {item.message}
              </span>
            </button>
          )}
        </For>
      </Show>
    </div>
  );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function ToolbarBtn(props: {
  title: string;
  onClick: () => void;
  disabled?: boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <button
      title={props.title}
      onClick={props.onClick}
      disabled={props.disabled ?? false}
      style={{
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        width: "24px",
        height: "22px",
        "border-radius": "3px",
        border: "none",
        background: "transparent",
        cursor: props.disabled ? "not-allowed" : "pointer",
        opacity: props.disabled ? "0.4" : "1",
        color: "var(--text-secondary)",
        transition: "background 0.1s",
        "flex-shrink": "0",
      }}
      onMouseEnter={(e) => {
        if (!props.disabled) (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.08))";
      }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
    >
      {props.children}
    </button>
  );
}

function ViewToggleBtn(props: {
  label: string;
  active: boolean;
  onClick: () => void;
}): JSX.Element {
  return (
    <button
      onClick={props.onClick}
      style={{
        padding: "2px 8px",
        "border-radius": "3px",
        border: "none",
        background: props.active ? "var(--bg-active, rgba(255,255,255,0.12))" : "transparent",
        color: props.active ? "var(--text-primary)" : "var(--text-muted)",
        "font-size": "11px",
        cursor: "pointer",
        transition: "background 0.1s",
        "white-space": "nowrap",
      }}
    >
      {props.label}
    </button>
  );
}

function formatDuration(ms: number | null): string {
  if (!ms) return "";
  if (ms < 1000) return ` in ${ms}ms`;
  const s = (ms / 1000).toFixed(1);
  return ` in ${s}s`;
}
