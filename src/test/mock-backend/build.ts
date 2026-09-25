import type {
  AppError,
  BuildActor,
  BuildError,
  BuildLine,
  BuildRecord,
  BuildStatus,
} from "@/bindings";
import { triggerEvent } from "./events";

let buildStatus: BuildStatus = { state: "idle" };
let nextBuildId = 1;

interface MockRun {
  id: number;
  task: string;
  origin: BuildActor;
  startedAt: string;
  timers: ReturnType<typeof setTimeout>[];
}

let activeRun: MockRun | null = null;

const MOCK_PROJECT_ROOT = "/mock/android-project";

/** A recorded build and its saved log; `lines: null` means log rotation removed it. */
interface MockHistoryEntry {
  record: BuildRecord;
  lines: BuildLine[] | null;
}

let history: MockHistoryEntry[] = [];
let nextRecordId = 1;
let appBuildLineDelayMs = 80;

/** Slows down builds the app starts, so a test can act while one runs. */
export function setMockAppBuildLineDelay(ms: number): void {
  appBuildLineDelayMs = ms;
}

const mockBuildLines: BuildLine[] = [
  { kind: "output", content: "> Configure project :app", file: null, line: null, col: null },
  { kind: "taskStart", content: "> Task :app:preBuild", file: null, line: null, col: null },
  {
    kind: "taskEnd",
    content: "> Task :app:preBuild UP-TO-DATE",
    file: null,
    line: null,
    col: null,
  },
  { kind: "taskStart", content: "> Task :app:assembleDebug", file: null, line: null, col: null },
  { kind: "taskEnd", content: "> Task :app:assembleDebug", file: null, line: null, col: null },
  { kind: "summary", content: "BUILD SUCCESSFUL in 4s", file: null, line: null, col: null },
];

function recordStatus(
  state: "success" | "failed" | "cancelled",
  errors: BuildError[]
): BuildStatus {
  if (state === "cancelled") return { state };
  return {
    state,
    success: state === "success",
    durationMs: 4000,
    errorCount: errors.filter((e) => e.severity === "error").length,
    warningCount: errors.filter((e) => e.severity === "warning").length,
  } as BuildStatus;
}

function recordBuild(
  build: Omit<BuildRecord, "id" | "status" | "projectRoot"> & {
    state: "success" | "failed" | "cancelled";
  },
  lines: BuildLine[] | null
): number {
  const { state, ...rest } = build;
  const id = nextRecordId++;
  history.push({
    record: {
      ...rest,
      id,
      status: recordStatus(state, build.errors),
      projectRoot: MOCK_PROJECT_ROOT,
    },
    lines,
  });
  return id;
}

/** A build recorded before the test ran, as the backend would load it from disk. */
export interface MockPastBuild {
  task: string;
  state: "success" | "failed" | "cancelled";
  errors?: BuildError[];
  /** The saved log; null when log rotation removed it. Default: the standard mock log. */
  lines?: BuildLine[] | null;
  origin?: BuildActor | null;
  cancelledBy?: BuildActor | null;
  minutesAgo?: number;
}

export function addMockPastBuild(build: MockPastBuild): number {
  return recordBuild(
    {
      task: build.task,
      state: build.state,
      errors: build.errors ?? [],
      startedAt: new Date(Date.now() - (build.minutesAgo ?? 5) * 60_000).toISOString(),
      origin: build.origin ?? { kind: "app" },
      cancelledBy: build.cancelledBy ?? null,
    },
    build.lines === undefined ? [...mockBuildLines] : build.lines
  );
}

/**
 * Starts a build the way the backend does for any client: build:started,
 * then build:lines, then build:complete. `lineDelayMs` slows it down so a
 * test can act while it runs.
 */
export function startMockBuild(task: string, origin: BuildActor, lineDelayMs = 80): number {
  if (activeRun) throw new Error("A build is already running");
  const startedAt = new Date().toISOString();
  const run: MockRun = { id: nextBuildId++, task, origin, startedAt, timers: [] };
  activeRun = run;
  buildStatus = { state: "running", task, started_at: startedAt };
  triggerEvent("build:started", {
    runId: run.id,
    task,
    origin,
    startedAt,
    projectRoot: MOCK_PROJECT_ROOT,
  });

  let delay = 100;
  for (const line of mockBuildLines) {
    run.timers.push(
      setTimeout(() => triggerEvent("build:lines", { runId: run.id, lines: [line] }), delay)
    );
    delay += lineDelayMs;
  }
  run.timers.push(
    setTimeout(() => {
      buildStatus = recordStatus("success", []);
      finish(run, { success: true, cancelled: false, cancelledBy: null });
    }, delay + 50)
  );
  return run.id;
}

function finish(
  run: MockRun,
  outcome: { success: boolean; cancelled: boolean; cancelledBy: BuildActor | null }
): void {
  run.timers.forEach(clearTimeout);
  if (activeRun === run) activeRun = null;
  // Like the backend, the history is recorded before build:complete.
  recordBuild(
    {
      task: run.task,
      state: outcome.cancelled ? "cancelled" : outcome.success ? "success" : "failed",
      errors: [],
      startedAt: run.startedAt,
      origin: run.origin,
      cancelledBy: outcome.cancelledBy,
    },
    [...mockBuildLines]
  );
  triggerEvent("build:complete", {
    runId: run.id,
    durationMs: 4000,
    errorCount: 0,
    warningCount: 0,
    task: run.task,
    origin: run.origin,
    ...outcome,
  });
}

export function buildHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    run_gradle_task: (args: unknown) => {
      const { task } = args as { task: string };
      return startMockBuild(task, { kind: "app" }, appBuildLineDelayMs);
    },
    cancel_build: () => {
      const run = activeRun;
      if (!run) return;
      buildStatus = { state: "cancelled" };
      finish(run, { success: false, cancelled: true, cancelledBy: { kind: "app" } });
    },
    get_build_status: () => ({ ...buildStatus }),
    get_build_errors: () => [],
    get_build_history: () => history.map((entry) => entry.record),
    clear_build_history: () => {
      history = [];
    },
    get_build_log_entries: (args: unknown) => {
      const { id } = args as { id: number };
      const lines = history.find((entry) => entry.record.id === id)?.lines;
      if (!lines) {
        const error: AppError = {
          kind: "notFound",
          message: `The log of build #${id} is no longer on disk`,
        };
        throw error;
      }
      return [...lines];
    },
  };
}
