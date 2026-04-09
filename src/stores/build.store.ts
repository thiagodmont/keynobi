import { createStore, produce } from "solid-js/store";
import { createMemo, createSignal } from "solid-js";
import type { BuildError, BuildRecord, BuildLine, LogEntry } from "@/bindings";
import { createLogStore, type LogStore } from "@/stores/log.store";
import { clearBuildHistory as clearBuildHistoryApi } from "@/lib/tauri-api";

// ── Types ─────────────────────────────────────────────────────────────────────

export type BuildPhase = "idle" | "running" | "success" | "failed" | "cancelled";
export type DeployPhase = "building" | "installing" | "launching" | null;

export interface BuildStoreState {
  phase: BuildPhase;
  currentTask: string | null;
  startedAt: number | null;
  durationMs: number | null;
  errors: BuildError[];
  warnings: BuildError[];
  history: BuildRecord[];
  deployPhase: DeployPhase;
}

// ── Tick (1-second reactive heartbeat while a build is running) ───────────────

let _tickHandle: ReturnType<typeof setInterval> | null = null;
const [_tick, _setTick] = createSignal(0);

function _startTick(): void {
  _stopTick();
  _tickHandle = setInterval(() => _setTick((n) => n + 1), 1000);
}

function _stopTick(): void {
  if (_tickHandle !== null) {
    clearInterval(_tickHandle);
    _tickHandle = null;
  }
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
  deployPhase: null,
});

export { buildState };

/** Dedicated log store for build output (capped at 10,000 lines). */
export const buildLogStore: LogStore = createLogStore({ maxEntries: 10_000 });

// ── Derived ───────────────────────────────────────────────────────────────────

/** Live elapsed time in milliseconds (only meaningful while running). */
export const buildDurationMs = createMemo(() => {
  _tick(); // reactive dependency — forces recompute every second while building
  if (buildState.phase !== "running" || !buildState.startedAt) {
    return buildState.durationMs ?? 0;
  }
  return Date.now() - buildState.startedAt;
});

export const hasErrors = createMemo(() => buildState.errors.length > 0);
export const hasWarnings = createMemo(() => buildState.warnings.length > 0);
export const isBuilding = createMemo(() => buildState.phase === "running");
export const isDeploying = createMemo(() => buildState.deployPhase !== null);

// ── Batching ──────────────────────────────────────────────────────────────────

let _pendingLines: BuildLine[] = [];
let _flushTimer: ReturnType<typeof setTimeout> | null = null;
let buildLogIdCounter = 0;

export function lineToLogEntry(line: BuildLine): LogEntry {
  const level: LogEntry["level"] =
    line.kind === "error" ? "error"
    : line.kind === "warning" ? "warn"
    : line.kind === "summary" || line.kind === "info" ? "info"
    : "debug";

  const message = line.file
    ? `${line.file}:${line.line ?? "?"}:${line.col ?? "?"} — ${line.content}`
    : line.content;

  return {
    id: ++buildLogIdCounter,
    timestamp: new Date().toISOString(),
    level,
    source: "gradle",
    message,
  };
}

function _lineToError(line: BuildLine): BuildError {
  return {
    message: line.content,
    file: line.file ?? null,
    line: line.line ?? null,
    col: line.col ?? null,
    severity: line.kind === "error" ? "error" : "warning",
  };
}

function _executePendingFlush(): void {
  _flushTimer = null;
  if (_pendingLines.length === 0) return;
  const batch = _pendingLines.splice(0); // drain — _pendingLines is now []

  buildLogStore.pushEntries(batch.map(lineToLogEntry));

  const errors = batch.filter((l) => l.kind === "error");
  const warnings = batch.filter((l) => l.kind === "warning");
  if (errors.length > 0 || warnings.length > 0) {
    setBuildState(
      produce((s) => {
        for (const l of errors) s.errors.push(_lineToError(l));
        for (const l of warnings) s.warnings.push(_lineToError(l));
      })
    );
  }
}

/** Add a line to the pending buffer. Schedules a 50ms flush if not already scheduled. */
export function addBuildLine(line: BuildLine): void {
  _pendingLines.push(line);
  if (_flushTimer === null) {
    _flushTimer = setTimeout(_executePendingFlush, 50);
  }
}

/** Flush all buffered lines immediately (called before finalising build state). */
export function flushPendingLines(): void {
  if (_flushTimer !== null) {
    clearTimeout(_flushTimer);
    _flushTimer = null;
  }
  _executePendingFlush();
}

// ── Actions ───────────────────────────────────────────────────────────────────

export function startBuild(task: string): void {
  // Discard any pending lines from a previous build.
  _pendingLines = [];
  if (_flushTimer !== null) {
    clearTimeout(_flushTimer);
    _flushTimer = null;
  }
  setBuildState({
    phase: "running",
    currentTask: task,
    startedAt: Date.now(),
    durationMs: null,
    errors: [],
    warnings: [],
    deployPhase: null,
  });
  buildLogStore.clearEntries();
  _startTick();
}

export function setDeployPhase(phase: DeployPhase): void {
  setBuildState("deployPhase", phase);
}

export function setBuildResult(opts: {
  success: boolean;
  durationMs: number;
  errorCount: number;
  warningCount: number;
}): void {
  _stopTick();
  setBuildState({
    phase: opts.success ? "success" : "failed",
    durationMs: opts.durationMs,
    startedAt: null,
  });
}

export function cancelBuildState(): void {
  _stopTick();
  setBuildState({ phase: "cancelled", startedAt: null, deployPhase: null });
}

export function clearBuild(): void {
  _pendingLines = [];
  if (_flushTimer !== null) {
    clearTimeout(_flushTimer);
    _flushTimer = null;
  }
  _stopTick();
  setBuildState({
    phase: "idle",
    currentTask: null,
    startedAt: null,
    durationMs: null,
    errors: [],
    warnings: [],
    deployPhase: null,
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

export async function clearBuildHistory(): Promise<void> {
  await clearBuildHistoryApi();
  setBuildState("history", []);
}
