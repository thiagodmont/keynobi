import type { BuildLine, BuildStatus } from "@/bindings";
import type { MockChannel } from "./channel";
import { triggerEvent } from "./events";

let buildStatus: BuildStatus = { state: "idle" };
let nextBuildId = 1;

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

export function buildHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    run_gradle_task: (args: unknown) => {
      const { task, onLine } = args as { task: string; onLine: MockChannel<BuildLine> };
      const id = nextBuildId++;
      buildStatus = { state: "running", task, started_at: new Date().toISOString() };

      let delay = 100;
      for (const line of mockBuildLines) {
        const captured = line;
        setTimeout(() => onLine.push(captured), delay);
        delay += 80;
      }

      setTimeout(() => {
        buildStatus = {
          state: "success",
          success: true,
          durationMs: BigInt(4000),
          errorCount: 0,
          warningCount: 0,
        } as BuildStatus;
        triggerEvent("build:complete", {
          success: true,
          cancelled: false,
          durationMs: 4000,
          errorCount: 0,
          warningCount: 0,
          task,
        });
      }, delay + 50);

      return id;
    },
    finalize_build: () => undefined,
    cancel_build: () => {
      buildStatus = { state: "cancelled" };
    },
    get_build_status: () => ({ ...buildStatus }),
    get_build_errors: () => [],
    get_build_history: () => [],
    clear_build_history: () => undefined,
    get_build_log_entries: () => [...mockBuildLines],
  };
}
