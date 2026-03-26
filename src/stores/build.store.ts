import { createStore, produce } from "solid-js/store";
import { createMemo } from "solid-js";
import type { BuildError, BuildRecord, BuildLine, LogEntry } from "@/bindings";
import { createLogStore, type LogStore } from "@/stores/log.store";

// ── Types ─────────────────────────────────────────────────────────────────────

export type BuildPhase = "idle" | "running" | "success" | "failed" | "cancelled";

export interface BuildStoreState {
  phase: BuildPhase;
  currentTask: string | null;
  startedAt: number | null;
  durationMs: number | null;
  errors: BuildError[];
  warnings: BuildError[];
  history: BuildRecord[];
}

// ── State ─────────────────────────────────────────────────────────────────────

const [buildState, setBuildState] = createStore<BuildStoreState>({
  phase: "idle",
  currentTask: null,
  startedAt: null,
  durationMs: null,
  errors: [],
  warnings: [],
  history: [],
});

export { buildState };

/** Dedicated log store for build output (capped at 10,000 lines). */
export const buildLogStore: LogStore = createLogStore({ maxEntries: 10_000 });

// ── Derived ───────────────────────────────────────────────────────────────────

/** Live elapsed time in milliseconds (only meaningful while running). */
export const buildDurationMs = createMemo(() => {
  if (buildState.phase !== "running" || !buildState.startedAt) {
    return buildState.durationMs ?? 0;
  }
  return Date.now() - buildState.startedAt;
});

export const hasErrors = createMemo(() => buildState.errors.length > 0);
export const hasWarnings = createMemo(() => buildState.warnings.length > 0);
export const isBuilding = createMemo(() => buildState.phase === "running");

// ── Actions ───────────────────────────────────────────────────────────────────

export function startBuild(task: string): void {
  setBuildState({
    phase: "running",
    currentTask: task,
    startedAt: Date.now(),
    durationMs: null,
    errors: [],
    warnings: [],
  });
  buildLogStore.clearEntries();
}

// Counter for LogEntry IDs within the build log.
let buildLogIdCounter = 0;

/** Convert a BuildLine received from the backend into a LogEntry for the viewer. */
export function addBuildLine(line: BuildLine): void {
  const level: LogEntry["level"] =
    line.kind === "error" ? "error"
    : line.kind === "warning" ? "warn"
    : line.kind === "summary" ? "info"
    : "debug";

  buildLogStore.pushEntry({
    id: ++buildLogIdCounter,
    timestamp: new Date().toISOString(),
    level,
    source: "gradle",
    message: line.file
      ? `${line.file}:${line.line ?? "?"}:${line.col ?? "?"} — ${line.content}`
      : line.content,
  });

  // Accumulate structured errors and warnings.
  if (line.kind === "error" && line.file && line.line != null) {
    setBuildState(
      produce((s) => {
        s.errors.push({
          message: line.content,
          file: line.file!,
          line: line.line!,
          col: line.col ?? null,
          severity: "error",
        });
      })
    );
  } else if (line.kind === "warning" && line.file && line.line != null) {
    setBuildState(
      produce((s) => {
        s.warnings.push({
          message: line.content,
          file: line.file!,
          line: line.line!,
          col: line.col ?? null,
          severity: "warning",
        });
      })
    );
  }
}

export function setBuildResult(opts: {
  success: boolean;
  durationMs: number;
  errorCount: number;
  warningCount: number;
}): void {
  setBuildState({
    phase: opts.success ? "success" : "failed",
    durationMs: opts.durationMs,
    startedAt: null,
  });
}

export function cancelBuildState(): void {
  setBuildState({ phase: "cancelled", startedAt: null });
}

export function clearBuild(): void {
  setBuildState({
    phase: "idle",
    currentTask: null,
    startedAt: null,
    durationMs: null,
    errors: [],
    warnings: [],
  });
  buildLogStore.clearEntries();
}

export function setBuildHistory(records: BuildRecord[]): void {
  setBuildState("history", records);
}

export function resetBuildState(): void {
  clearBuild();
  setBuildState("history", []);
}
