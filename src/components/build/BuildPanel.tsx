import {
  type JSX,
  Show,
  For,
  createMemo,
  createSignal,
  createEffect,
} from "solid-js";
import { buildState, buildLogStore, isBuilding, isDeploying } from "@/stores/build.store";
import { runBuild, runAndDeploy, cancelBuild, jumpToBuildError } from "@/services/build.service";
import { LogViewer } from "@/components/common/LogViewer";
import type { BuildError } from "@/bindings";
import Icon from "@/components/common/Icon";
import { projectState } from "@/stores/project.store";

type ViewMode = "log" | "problems";

export function BuildPanel(): JSX.Element {
  const [viewMode, setViewMode] = createSignal<ViewMode>("log");
  const [running, setRunning] = createSignal(false);

  const phase = () => buildState.phase;
  const deployPhase = () => buildState.deployPhase;
  const errorCount = () => buildState.errors.length;
  const warnCount = () => buildState.warnings.length;
  const durationMs = () => buildState.durationMs;

  // Auto-switch to Problems tab when the build fails and has diagnostics.
  createEffect(() => {
    if (phase() === "failed" && (errorCount() + warnCount()) > 0) {
      setViewMode("problems");
    }
  });

  const summaryColor = createMemo(() => {
    if (deployPhase() === "installing" || deployPhase() === "launching") return "#60a5fa";
    switch (phase()) {
      case "success": return "var(--success, #4ade80)";
      case "failed":  return "var(--error, #f87171)";
      case "cancelled": return "var(--text-muted)";
      default:        return "var(--text-secondary)";
    }
  });

  const summaryLabel = createMemo(() => {
    if (deployPhase() === "installing") return "Installing APK…";
    if (deployPhase() === "launching") return "Launching app…";
    switch (phase()) {
      case "running": return "Building…";
      case "success": return `Build successful${formatDuration(durationMs())}`;
      case "failed":  return `Build failed${formatDuration(durationMs())} — ${errorCount()} error${errorCount() !== 1 ? "s" : ""}`;
      case "cancelled": return "Build cancelled";
      default:        return null;
    }
  });

  const busy = () => running() || isDeploying();

  /** Full run: build → install → launch on the selected device. */
  async function handleRunApp() {
    setRunning(true);
    setViewMode("log");
    try {
      await runAndDeploy();
    } catch (_e) {
      // Error is already logged to the build log inside runAndDeploy().
      // Nothing more to do here; the log is the source of truth.
    } finally {
      setRunning(false);
    }
  }

  /** Build only — no install/launch. */
  async function handleBuildOnly() {
    setRunning(true);
    setViewMode("log");
    try {
      await runBuild();
    } catch (_e) {
      // Spawn-level errors (e.g. gradlew not found) are logged in runBuild().
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
      {/* ── No-project empty state ── */}
      <Show when={!projectState.projectRoot}>
        <div
          style={{
            flex: "1",
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            "justify-content": "center",
            gap: "12px",
            color: "var(--text-muted)",
          }}
        >
          <Icon name="folder" size={36} color="var(--text-muted)" />
          <div style={{ "text-align": "center" }}>
            <div style={{ "font-size": "14px", "font-weight": "500", "margin-bottom": "4px", color: "var(--text-secondary)" }}>
              No project selected
            </div>
            <div style={{ "font-size": "12px", "line-height": "1.5" }}>
              Select a project from the sidebar to start building.
            </div>
          </div>
        </div>
      </Show>

      {/* ── Normal build UI (only when a project is open) ── */}
      <Show when={!!projectState.projectRoot}>
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
        {/* Run App button (build + install + launch) */}
        <Show
          when={isBuilding() || isDeploying()}
          fallback={
            <ToolbarBtn title="Run App — build, install & launch (Cmd+R)" onClick={handleRunApp} disabled={busy()}>
              <Icon name="play" size={13} color="#4ade80" />
            </ToolbarBtn>
          }
        >
          <ToolbarBtn title="Cancel" onClick={handleCancel}>
            <Icon name="stop" size={13} color="#f87171" />
          </ToolbarBtn>
        </Show>

        {/* Build Only button */}
        <Show when={!isBuilding() && !isDeploying()}>
          <ToolbarBtn title="Build only — no install (Cmd+Shift+R)" onClick={handleBuildOnly} disabled={busy()}>
            <Icon name="hammer" size={13} color="var(--text-secondary)" />
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
            hasErrors={errorCount() > 0}
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
            emptyMessage="No build output yet — press the run button or Cmd+Shift+R"
          />
        </Show>
        <Show when={viewMode() === "problems"}>
          <ProblemsView
            errors={buildState.errors}
            warnings={buildState.warnings}
          />
        </Show>
      </div>
      </Show>
    </div>
  );
}

// ── Problems view ─────────────────────────────────────────────────────────────

type DiagnosticItem = BuildError & { _kind: "error" | "warning" };

function ProblemsView(props: {
  errors: BuildError[];
  warnings: BuildError[];
}): JSX.Element {
  const all = createMemo<DiagnosticItem[]>(() => [
    ...props.errors.map((e) => ({ ...e, _kind: "error" as const })),
    ...props.warnings.map((w) => ({ ...w, _kind: "warning" as const })),
  ]);

  // Split: items with a file location vs. general/unlocated errors.
  const withFile = createMemo(() => all().filter((i) => !!i.file));
  const general = createMemo(() => all().filter((i) => !i.file));

  // Group located items by file.
  const byFile = createMemo(() => {
    const map = new Map<string, DiagnosticItem[]>();
    for (const item of withFile()) {
      const key = item.file!;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(item);
    }
    return Array.from(map.entries());
  });

  return (
    <Show
      when={all().length > 0}
      fallback={
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
      }
    >
      <div style={{ "overflow-y": "auto", height: "100%", "font-size": "12px" }}>
        {/* File-located diagnostics grouped by file */}
        <For each={byFile()}>
          {([file, items]) => (
            <FileErrorGroup file={file} items={items} />
          )}
        </For>
        {/* General/unlocated errors (dependency failures, AAPT bare errors, etc.) */}
        <Show when={general().length > 0}>
          <GeneralErrorGroup items={general()} />
        </Show>
      </div>
    </Show>
  );
}

function FileErrorGroup(props: {
  file: string;
  items: DiagnosticItem[];
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
          {(item) => <DiagnosticRow item={item} showLocation />}
        </For>
      </Show>
    </div>
  );
}

function GeneralErrorGroup(props: { items: DiagnosticItem[] }): JSX.Element {
  const [collapsed, setCollapsed] = createSignal(false);
  const errorCount = () => props.items.filter((i) => i._kind === "error").length;
  const warnCount = () => props.items.filter((i) => i._kind === "warning").length;

  const label = () => {
    const parts: string[] = [];
    if (errorCount() > 0) parts.push(`${errorCount()} error${errorCount() !== 1 ? "s" : ""}`);
    if (warnCount() > 0) parts.push(`${warnCount()} warning${warnCount() !== 1 ? "s" : ""}`);
    return parts.join(", ");
  };

  return (
    <div style={{ "border-bottom": "1px solid var(--border)" }}>
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
        <span>General ({label()})</span>
        <span style={{ opacity: "0.6", "margin-left": "auto" }}>
          {props.items.length}
        </span>
      </button>

      <Show when={!collapsed()}>
        <For each={props.items}>
          {(item) => <DiagnosticRow item={item} showLocation={false} />}
        </For>
      </Show>
    </div>
  );
}

function DiagnosticRow(props: { item: DiagnosticItem; showLocation: boolean }): JSX.Element {
  return (
    <button
      onClick={() => jumpToBuildError(props.item).catch(console.error)}
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
        "border-left": `3px solid ${props.item._kind === "error" ? "var(--error, #f87171)" : "var(--warning, #fbbf24)"}`,
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.05))"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
    >
      <Show when={props.showLocation && props.item.line !== null && props.item.line !== undefined}>
        <span
          style={{
            "font-size": "10px",
            "white-space": "nowrap",
            color: "var(--text-muted)",
            "flex-shrink": "0",
            "padding-top": "1px",
          }}
        >
          {props.item.line}:{props.item.col ?? 1}
        </span>
      </Show>
      <span
        style={{
          color: props.item._kind === "error" ? "var(--error, #f87171)" : "var(--warning, #fbbf24)",
          "word-break": "break-word",
          "white-space": "pre-wrap",
          "font-size": "11px",
          "line-height": "1.4",
        }}
      >
        {props.item.message}
      </span>
    </button>
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
      onClick={() => props.onClick()}
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
  hasErrors?: boolean;
}): JSX.Element {
  return (
    <button
      onClick={() => props.onClick()}
      style={{
        padding: "2px 8px",
        "border-radius": "3px",
        border: "none",
        background: props.active ? "var(--bg-active, rgba(255,255,255,0.12))" : "transparent",
        color: props.active
          ? "var(--text-primary)"
          : props.hasErrors
            ? "var(--error, #f87171)"
            : "var(--text-muted)",
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
