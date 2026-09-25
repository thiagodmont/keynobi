import type { BuildActor, BuildLine, BuildStatus } from "@/bindings";
import { triggerEvent } from "./events";

let buildStatus: BuildStatus = { state: "idle" };
let nextBuildId = 1;

interface MockRun {
  id: number;
  task: string;
  origin: BuildActor;
  timers: ReturnType<typeof setTimeout>[];
}

let activeRun: MockRun | null = null;

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

/**
 * Starts a build the way the backend does for any client: build:started,
 * then build:lines, then build:complete. `lineDelayMs` slows it down so a
 * test can act while it runs.
 */
export function startMockBuild(task: string, origin: BuildActor, lineDelayMs = 80): number {
  if (activeRun) throw new Error("A build is already running");
  const run: MockRun = { id: nextBuildId++, task, origin, timers: [] };
  activeRun = run;
  buildStatus = { state: "running", task, started_at: new Date().toISOString() };
  triggerEvent("build:started", {
    runId: run.id,
    task,
    origin,
    startedAt: new Date().toISOString(),
    projectRoot: "/mock/android-project",
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
      buildStatus = {
        state: "success",
        success: true,
        durationMs: BigInt(4000),
        errorCount: 0,
        warningCount: 0,
      } as BuildStatus;
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
      return startMockBuild(task, { kind: "app" });
    },
    cancel_build: () => {
      const run = activeRun;
      if (!run) return;
      buildStatus = { state: "cancelled" };
      finish(run, { success: false, cancelled: true, cancelledBy: { kind: "app" } });
    },
    get_build_status: () => ({ ...buildStatus }),
    get_build_errors: () => [],
    get_build_history: () => [],
    clear_build_history: () => undefined,
    get_build_log_entries: () => [...mockBuildLines],
  };
}
