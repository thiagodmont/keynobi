import type {
  AppError,
  BuildActor,
  BuildError,
  BuildLine,
  BuildRecord,
  BuildStatus,
  BuiltApk,
  Device,
  InstalledBuild,
  LaunchTiming,
  MappingSnapshot,
  RunApk,
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
/** The mock project's only application module. */
const MOCK_APP_MODULE = ":app";

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
  build: Omit<BuildRecord, "id" | "status" | "projectRoot" | "launch" | "mappings" | "apks"> & {
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
      launch: null,
      mappings: mockMappings(state, build.task),
      apks: mockApks(state, build.task, id),
    },
    lines,
  });
  return id;
}

/**
 * Like the backend, a successful build records the R8 mapping it wrote. Here a
 * release task stands for a minified variant.
 */
function mockMappings(state: "success" | "failed" | "cancelled", task: string): MappingSnapshot[] {
  if (state !== "success" || !/release/i.test(task)) return [];
  return [
    {
      module: ":app",
      variant: "release",
      sha256: "6b1c2f0a".repeat(8),
      bytes: 48_213_771,
      pgMapId: "6b1c2f0",
    },
  ];
}

/** Like the backend, a successful assemble build records the APK it wrote. */
function mockApks(state: "success" | "failed" | "cancelled", task: string, id: number): BuiltApk[] {
  const variant = /^(?::app:)?assemble(.+)$/.exec(task)?.[1];
  if (state !== "success" || !variant) return [];
  const name = variant.charAt(0).toLowerCase() + variant.slice(1);
  return [
    {
      module: MOCK_APP_MODULE,
      variant: name,
      applicationId: /debug/i.test(name) ? "com.example.mockapp.debug" : "com.example.mockapp",
      versionCode: 1,
      sha256: id.toString(16).padStart(64, "0"),
      bytes: 8_388_608,
      path: `app/build/outputs/apk/${name}/app-${name}.apk`,
    },
  ];
}

/**
 * Like the backend: the APK the build recorded for the module and variant,
 * else (up to date) the variant's APK in the outputs, matched to the newest
 * build that wrote it.
 */
function mockRunApk(variant: string, buildId: number | null): RunApk {
  const matches = (apk: BuiltApk) =>
    apk.module === MOCK_APP_MODULE && apk.variant.toLowerCase() === variant.toLowerCase();
  const own = history.find((e) => e.record.id === buildId)?.record.apks.find(matches);
  if (own) {
    return { path: `${MOCK_PROJECT_ROOT}/${own.path}`, buildId, fromThisBuild: true };
  }
  const writer = [...history].reverse().find((e) => e.record.apks.some(matches));
  return {
    path: `${MOCK_PROJECT_ROOT}/app/build/outputs/apk/${variant}/app-${variant}.apk`,
    buildId: writer?.record.id ?? null,
    fromThisBuild: false,
  };
}

/** Most installs kept, as `MAX_INSTALLED_TARGETS` in the backend. */
const MAX_MOCK_INSTALLS = 16;
let installs: InstalledBuild[] = [];

function sameDevice(a: InstalledBuild, b: InstalledBuild): boolean {
  if (a.avdName !== null || b.avdName !== null) return a.avdName === b.avdName;
  return a.serial === b.serial;
}

/**
 * Like the backend: an install is matched to the newest build that wrote an
 * APK of the same file name (the backend compares hashes) and replaces the
 * earlier install of that package on that device.
 */
export function recordMockInstall(serial: string, device: Device | undefined, apkPath: string) {
  const file = apkPath.split("/").pop();
  const match = [...history]
    .reverse()
    .map(({ record }) => ({ record, apk: record.apks.find((a) => a.path.endsWith(`/${file}`)) }))
    .find((m) => m.apk !== undefined);
  const apk = match?.apk;
  const entry: InstalledBuild = {
    serial,
    avdName: device?.avdName ?? null,
    model: device?.model ?? null,
    package: apk?.applicationId ?? "com.example.mockapp.debug",
    apkSha256: apk?.sha256 ?? "f".repeat(64),
    buildId: match?.record.id ?? null,
    versionCode: apk?.versionCode ?? null,
    mappings: match?.record.mappings.filter((m) => m.variant === apk?.variant) ?? [],
    installedAt: new Date().toISOString(),
  };
  installs = installs.filter((i) => !(i.package === entry.package && sameDevice(i, entry)));
  installs = [...installs, entry].slice(-MAX_MOCK_INSTALLS);
}

/** Like the backend: a launch time is recorded only on a successful build. */
export function attachMockLaunch(id: number, timing: LaunchTiming): void {
  const entry = history.find((e) => e.record.id === id);
  // A new object, as a fresh IPC response would be: the frontend store may hold the old one.
  if (entry?.record.status.state === "success") entry.record = { ...entry.record, launch: timing };
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
  /** The launch time Run App recorded on it (successful builds only). */
  launch?: LaunchTiming;
}

export function addMockPastBuild(build: MockPastBuild): number {
  const id = recordBuild(
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
  if (build.launch) attachMockLaunch(id, build.launch);
  return id;
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
  const recordId = recordBuild(
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
    recordId,
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
    get_application_module: (args: unknown) => {
      const { module } = (args ?? {}) as { module?: string | null };
      if (module && module !== MOCK_APP_MODULE) {
        const error: AppError = {
          kind: "invalidInput",
          message: `'${module}' does not name an application module of this project. Application modules: ${MOCK_APP_MODULE}.`,
        };
        throw error;
      }
      return MOCK_APP_MODULE;
    },
    find_apk_path: (args: unknown) => {
      const { variant, buildId } = args as { variant: string; buildId?: number | null };
      return mockRunApk(variant, buildId ?? null);
    },
    get_build_status: () => ({ ...buildStatus }),
    get_build_errors: () => [],
    get_build_history: () => history.map((entry) => entry.record),
    list_installed_builds: () => [...installs],
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
