import {
  runGradleTask,
  cancelBuild as cancelBuildApi,
  findApkPath,
  getPackageNameFromApk,
  installApkOnDevice,
  launchAppOnDevice,
  getBuildHistory,
  listenBuildStarted,
  listenBuildLines,
  listenBuildComplete,
  formatError,
  type BuildActor,
  type BuildCompleteEvent,
  type BuildLine,
  type BuildLinesEvent,
  type BuildStartedEvent,
} from "@/lib/tauri-api";
import {
  startBuild,
  addBuildLine,
  flushPendingLines,
  setBuildResult,
  cancelBuildState,
  setBuildHistory,
  setDeployPhase,
  setLastLaunchedAt,
  buildState,
  isAgentBuilding,
} from "@/stores/build.store";
import { variantState } from "@/stores/variant.store";
import { deviceState } from "@/stores/device.store";
import { setActiveTab } from "@/stores/ui.store";
import { projectState, currentProjectGeneration } from "@/stores/project.store";
import { settingsState } from "@/stores/settings.store";
import { isActiveProjectTrusted } from "@/stores/projects.store";
import { buildRunningLabel } from "@/lib/build-actor";
import { formatLaunchTime } from "@/lib/launch-timing";
import type { BuildError } from "@/bindings";

let buildUnlisteners: Array<() => void> | null = null;
// Held so concurrent callers await the SAME registration. A plain
// `if (unlisten) return` guard is checked before the await, so two interleaved
// calls both register and the second orphans the first's unlisten.
let buildListenerInit: Promise<void> | null = null;
let currentBuildPromise: Promise<BuildCompletion | null> | null = null;
let deployInFlight = false;

interface RunBuildOptions {
  headerLines?: string[];
}

// ── Registration ──────────────────────────────────────────────────────────────

/** Call once on app startup to register the build event listeners. */
export function initBuildService(): Promise<void> {
  if (buildListenerInit) return buildListenerInit;

  buildListenerInit = registerBuildListeners().catch((err) => {
    // Allow a later retry rather than wedging the service permanently.
    buildListenerInit = null;
    throw err;
  });
  return buildListenerInit;
}

/** Test-only teardown, mirroring resetMcpListenersForTests. */
export function resetBuildServiceForTests(): void {
  buildUnlisteners?.forEach((unlisten) => unlisten());
  buildUnlisteners = null;
  buildListenerInit = null;
  activeRun = null;
  observedRun = null;
  earlyCompletions.clear();
  clearEarlyLines();
  clearBuildCompleteTimer();
}

async function registerBuildListeners(): Promise<void> {
  const registrations = await Promise.allSettled([
    listenBuildStarted(onBuildStarted),
    listenBuildLines(onBuildLines),
    listenBuildComplete(onBuildComplete),
  ]);
  const unlisteners = registrations.flatMap((r) => (r.status === "fulfilled" ? [r.value] : []));
  const failed = registrations.find((r) => r.status === "rejected");
  if (failed) {
    unlisteners.forEach((unlisten) => unlisten());
    throw failed.reason;
  }

  if (buildUnlisteners) {
    // A reset landed while we were awaiting — drop this registration.
    unlisteners.forEach((unlisten) => unlisten());
    return;
  }
  buildUnlisteners = unlisteners;

  // Load persisted history on startup so previous sessions are visible immediately.
  getBuildHistory()
    .then(setBuildHistory)
    .catch((err) => {
      console.error("[build] Failed to load initial build history:", err);
    });
}

interface BuildCompletion {
  success: boolean;
  durationMs: number;
  /** History record the run was saved as; null when it was cancelled from here. */
  recordId: number | null;
}

interface ActiveRun {
  /** Null until build:started or run_gradle_task names it. */
  runId: number | null;
  resolve: (result: BuildCompletion) => void;
}

/** A build this window did not start, typically an agent's. */
interface ObservedRun {
  runId: number;
  task: string;
  origin: BuildActor;
  /** The project generation it started under; a project switch drops it. */
  generation: number;
  /** Shown in the Build panel. Held back while this window's own build or deploy runs. */
  shown: boolean;
  /** Output received while not shown. */
  hiddenLines: BuildLine[];
}

// The build this window started and is waiting on. Cleared on completion,
// cancellation, timeout, or spawn failure.
let activeRun: ActiveRun | null = null;
let observedRun: ObservedRun | null = null;
// Completions and output received while activeRun.runId was still unknown.
const earlyCompletions = new Map<number, BuildCompleteEvent>();
const MAX_EARLY_COMPLETIONS = 8;
const earlyLines = new Map<number, BuildLine[]>();
let earlyLineCount = 0;
const MAX_EARLY_LINES = 2_000;
/** Output of an observed build kept while it is not shown. */
const MAX_HIDDEN_LINES = 5_000;
let _buildCompleteTimer: ReturnType<typeof setTimeout> | null = null;

function onBuildStarted(e: BuildStartedEvent): void {
  if (activeRun?.runId === e.runId) return;
  if (activeRun && activeRun.runId === null && e.origin.kind === "app") {
    // Only this window starts builds for the app, so this is the one it awaits.
    adoptRunId(activeRun, e.runId);
    return;
  }
  observedRun = {
    runId: e.runId,
    task: e.task,
    origin: e.origin,
    generation: currentProjectGeneration(),
    shown: false,
    hiddenLines: [],
  };
  showObservedRunWhenIdle();
}

function onBuildLines(e: BuildLinesEvent): void {
  if (activeRun) {
    if (activeRun.runId === e.runId) {
      e.lines.forEach(addBuildLine);
      return;
    }
    if (activeRun.runId === null && earlyLineCount + e.lines.length <= MAX_EARLY_LINES) {
      earlyLines.set(e.runId, [...(earlyLines.get(e.runId) ?? []), ...e.lines]);
      earlyLineCount += e.lines.length;
    }
  }
  const run = observedRun;
  if (run?.runId !== e.runId) return;
  if (run.shown) {
    e.lines.forEach(addBuildLine);
  } else {
    run.hiddenLines.push(...e.lines);
    if (run.hiddenLines.length > MAX_HIDDEN_LINES) {
      run.hiddenLines.splice(0, run.hiddenLines.length - MAX_HIDDEN_LINES);
    }
  }
}

function onBuildComplete(e: BuildCompleteEvent): void {
  // Rust records history before emitting build:complete, so this fetch sees
  // the completed record without a frontend finalize step.
  getBuildHistory()
    .then(setBuildHistory)
    .catch((err) => {
      console.error("[build] Failed to reload build history:", err);
    });

  // Only the run this window is waiting on may finish it. Late events from
  // cancelled, timed-out, or replaced runs are ignored.
  if (activeRun) {
    if (activeRun.runId === null) {
      // run_gradle_task has not returned the run's ID yet.
      if (earlyCompletions.size < MAX_EARLY_COMPLETIONS) earlyCompletions.set(e.runId, e);
    } else if (activeRun.runId === e.runId) {
      completeActiveRun(e);
    }
  }

  const run = observedRun;
  if (run?.runId !== e.runId) return;
  observedRun = null;
  // A build that finished unseen is in the history list. A project switch
  // resets the panel, so a build shown before it is not brought back.
  if (!run.shown || run.generation !== currentProjectGeneration()) return;
  flushPendingLines();
  if (e.cancelled) {
    cancelBuildState(e.cancelledBy);
  } else {
    setBuildResult({ success: e.success, durationMs: e.durationMs });
  }
}

/** Learn the run ID of this window's own build and apply what arrived early. */
function adoptRunId(run: ActiveRun, runId: number): void {
  run.runId = runId;
  const lines = earlyLines.get(runId) ?? [];
  clearEarlyLines();
  lines.forEach(addBuildLine);
  const early = earlyCompletions.get(runId);
  earlyCompletions.clear();
  if (early) completeActiveRun(early);
}

function clearEarlyLines(): void {
  earlyLines.clear();
  earlyLineCount = 0;
}

/** This window is running its own build or deploy. */
function ownFlowBusy(): boolean {
  return activeRun !== null || currentBuildPromise !== null || deployInFlight;
}

/** Show the observed build in the Build panel unless this window's own flow is using it. */
function showObservedRunWhenIdle(): void {
  const run = observedRun;
  if (!run || run.shown || ownFlowBusy()) return;
  if (run.generation !== currentProjectGeneration()) {
    observedRun = null;
    return;
  }
  run.shown = true;
  startBuild(run.task, run.origin);
  run.hiddenLines.splice(0).forEach(addBuildLine);
}

function completeActiveRun(e: BuildCompleteEvent): void {
  const run = activeRun;
  if (!run) return;
  activeRun = null;
  earlyCompletions.clear();
  clearEarlyLines();
  clearBuildCompleteTimer();

  // Flush any lines still in the 50ms buffer before updating phase.
  flushPendingLines();
  if (e.cancelled) {
    cancelBuildState(e.cancelledBy);
  } else {
    setBuildResult({ success: e.success, durationMs: e.durationMs });
  }
  run.resolve({ success: e.success, durationMs: e.durationMs, recordId: e.recordId });
}

function clearBuildCompleteTimer(): void {
  if (_buildCompleteTimer !== null) {
    clearTimeout(_buildCompleteTimer);
    _buildCompleteTimer = null;
  }
}

/** Why a new build cannot start while one runs, naming an agent that started it. */
function buildRunningMessage(fallback: string): string {
  return isAgentBuilding() ? `${buildRunningLabel(buildState.origin)}.` : fallback;
}

// ── Build actions ─────────────────────────────────────────────────────────────

/**
 * Run a Gradle task and stream output into the build panel.
 *
 * Returns only after the build:complete event is received, ensuring
 * buildState.phase reflects the true final state.
 *
 * @param opts.headerLines  Lines injected at the top of the log right after it
 *                          clears — used by runAndDeploy to surface context.
 */
export async function runBuild(task?: string, opts?: RunBuildOptions): Promise<void> {
  await runBuildGuarded(task, opts, false);
}

/** Button title for build actions disabled in Safe Mode. */
export const SAFE_MODE_BUILD_TITLE = "Safe Mode — trust this project to build";

/** Builds run the project's own Gradle scripts, so Safe Mode refuses them. */
function assertProjectTrusted(): void {
  if (projectState.projectRoot && !isActiveProjectTrusted()) {
    throw new Error(
      `${SAFE_MODE_BUILD_TITLE}. Keynobi does not run the Gradle build scripts of a project you have not trusted: right-click it in the Projects sidebar and choose Trust Project.`
    );
  }
}

async function runBuildGuarded(
  task: string | undefined,
  opts: RunBuildOptions | undefined,
  allowDuringDeploy: boolean
): Promise<BuildCompletion | null> {
  assertProjectTrusted();
  if (deployInFlight && !allowDuringDeploy) {
    throw new Error("A build or deploy is already running.");
  }
  if (currentBuildPromise || buildState.phase === "running" || observedRun) {
    throw new Error(buildRunningMessage("A build is already running."));
  }

  const promise = runBuildInternal(task, opts);
  currentBuildPromise = promise;
  try {
    return await promise;
  } finally {
    if (currentBuildPromise === promise) {
      currentBuildPromise = null;
    }
    showObservedRunWhenIdle();
  }
}

async function runBuildInternal(
  task?: string,
  opts?: RunBuildOptions
): Promise<BuildCompletion | null> {
  const variant = variantState.activeVariant;
  const effectiveTask = task ?? (variant ? `assemble${capitalize(variant)}` : "assembleDebug");

  startBuild(effectiveTask);
  setActiveTab("build");

  // Inject context header AFTER startBuild clears the log.
  if (opts?.headerLines?.length) {
    for (const line of opts.headerLines) {
      addBuildLine({ kind: "info", content: line, file: null, line: null, col: null });
    }
  }

  logBuildHeader(effectiveTask);

  // Create a promise that resolves when the build:complete event fires.
  // A timeout prevents the deploy from hanging forever if something goes
  // wrong in the Rust on_exit callback. Uses the user-configured Gradle
  // timeout (Settings → MCP → buildTimeoutSec) so long cold builds are not
  // falsely failed while Gradle is still running.
  const buildTimeoutSec = Math.min(3600, Math.max(60, settingsState.mcp?.buildTimeoutSec ?? 600));
  const run: ActiveRun = { runId: null, resolve: () => {} };
  const buildComplete = new Promise<BuildCompletion>((resolve, reject) => {
    run.resolve = resolve;
    _buildCompleteTimer = setTimeout(() => {
      if (activeRun === run) {
        activeRun = null;
        _buildCompleteTimer = null;
        reject(
          new Error(
            `Build timed out waiting for the build:complete event after ${buildTimeoutSec} seconds.`
          )
        );
      }
    }, buildTimeoutSec * 1000);
  });
  activeRun = run;
  earlyCompletions.clear();
  clearEarlyLines();

  try {
    const runId = await runGradleTask(effectiveTask);
    // build:started usually names the run first.
    if (activeRun === run && run.runId === null) adoptRunId(run, runId);
  } catch (e) {
    // Spawn failure (e.g. gradlew not found), or another build holds the slot.
    if (activeRun === run) activeRun = null;
    clearBuildCompleteTimer();
    clearEarlyLines();
    const msg = formatError(e);
    // The build that won the slot takes over the panel once this flow ends.
    if (observedRun) throw e;
    addBuildLine({
      kind: "error",
      content: `Failed to start Gradle: ${msg}`,
      file: null,
      line: null,
      col: null,
    });
    setBuildResult({ success: false, durationMs: 0 });
    throw e;
  }

  // runGradleTask resolves right after spawn; wait for the actual completion event.
  let completion: BuildCompletion;
  try {
    completion = await buildComplete;
  } catch (e) {
    clearBuildCompleteTimer();
    // The only rejection path is the completion timeout: Gradle is still
    // running. Cancel it so the shared build slot is released. Its late
    // completion event no longer matches the active run and is ignored.
    try {
      await cancelBuild();
    } catch (cancelErr) {
      console.error("[build] Failed to cancel timed-out build:", formatError(cancelErr));
    }
    const msg = formatError(e);
    addBuildLine({
      kind: "error",
      content: `Build event error: ${msg}`,
      file: null,
      line: null,
      col: null,
    });
    setBuildResult({ success: false, durationMs: 0 });
    throw e;
  }

  // Rust already recorded the build result before emitting build:complete.
  if (buildState.phase === "cancelled") return null;

  getBuildHistory()
    .then(setBuildHistory)
    .catch((err) => {
      console.error("[build] Failed to reload build history:", err);
    });
  return completion;
}

/**
 * Full build → install → launch cycle.
 *
 * If no device is selected, resolves a device via the DevicePickerDialog —
 * skipped entirely when "Auto Install on Build" is off (build-only run).
 * After a successful build the APK is installed and the app launched.
 */
export async function runAndDeploy(): Promise<void> {
  assertProjectTrusted();
  if (
    deployInFlight ||
    currentBuildPromise ||
    buildState.phase === "running" ||
    buildState.deployPhase ||
    observedRun
  ) {
    throw new Error(buildRunningMessage("A build or deploy is already running."));
  }

  deployInFlight = true;
  const variant = variantState.activeVariant;
  // APK lookup reads the backend's current project, so a project switch
  // mid-deploy must stop it before it installs the other project's APK.
  const projectGeneration = currentProjectGeneration();
  // Read once up front: when auto-install is off this is a build-only run,
  // which must not force device selection.
  const autoInstall = settingsState.build.autoInstallOnBuild !== false;

  try {
    if (!variant) {
      throw new Error("No build variant selected. Open Build → Select Variant.");
    }

    // Resolve a device before the build so we can bail early — but only when
    // the result will actually be installed; a build-only run needs no device.
    let serial: string | null = null;
    if (autoInstall) {
      // We log this BEFORE startBuild clears the log — that's intentional;
      // users will see the context when the build panel opens.
      logStep("Resolving target device…");
      serial = await resolveDevice();
      if (!serial) {
        logStep("No device selected — run cancelled.");
        return;
      }
      logStep(`Target device: ${serial}`);
    }

    // 1. Build. startBuild() inside runBuild() clears the log, so we add a
    //    context header as the very first callback line from the Gradle channel.
    setDeployPhase("building");
    const completion = await runBuildGuarded(
      `assemble${capitalize(variant)}`,
      {
        headerLines: [serial ? `── Deploy: ${variant} → ${serial} ──` : `── Build: ${variant} ──`],
      },
      true
    );

    const phase = buildState.phase;
    if (phase !== "success") {
      logError(`Build phase is "${phase}" — skipping install. Check the Problems tab for errors.`);
      setDeployPhase(null);
      return;
    }

    // The "Auto Install on Build" setting gates install + launch; the build
    // itself still counts as a successful deploy cycle when it is off.
    if (!autoInstall) {
      logStep("Auto Install on Build is disabled — skipping install and launch.");
      setDeployPhase(null);
      return;
    }
    if (!serial) {
      throw new Error("No device selected.");
    }

    // 2. Find APK.
    assertSameProject(projectGeneration);
    logStep(`Searching for APK (variant: ${variant})…`);
    // Rejects with the reason when no APK of this variant exists; another
    // variant's APK is never used.
    const apkPath = await findApkPath(variant);
    assertSameProject(projectGeneration);
    logStep(`APK: ${apkPath}`);

    // 3. Install.
    setDeployPhase("installing");
    const deviceInfo = deviceLabel(serial);
    logStep(`Installing on: ${deviceInfo}`);
    logStep(`adb install ${apkPath}`);
    const installStart = Date.now();
    const installOutput = await installApkOnDevice(serial, apkPath);
    logStep(`Install: ${installOutput.trim()} (${formatDuration(Date.now() - installStart)})`);

    // 4. Launch — resolve the exact package name of this APK (aapt2, or the
    // variant's output metadata). The project's base applicationId is not a
    // safe guess: it ignores applicationIdSuffix and would launch another app.
    setDeployPhase("launching");
    let packageName: string | null = null;
    try {
      packageName = await getPackageNameFromApk(apkPath);
      logStep(`Package (from APK): ${packageName}`);
    } catch (e) {
      logStep(`Could not read the APK's package name: ${formatError(e)}`);
    }

    if (packageName) {
      logStep(`adb shell am start -W (package: ${packageName})`);
      // The launch time is recorded on the build this deploy ran, named by
      // its own build:complete, never on whichever build finished last.
      const buildId = completion?.success ? completion.recordId : null;
      const launch = await launchAppOnDevice(serial, packageName, { buildId });
      logStep(`Launch: ${launch.output.trim()}`);
      setLastLaunchedAt(Date.now(), packageName);
      logStep(
        launch.timing
          ? `Launch time: ${formatLaunchTime(launch.timing)}`
          : "Launch time: not reported by this launch method"
      );
      if (launch.timing && buildId !== null) {
        getBuildHistory()
          .then(setBuildHistory)
          .catch((err) => {
            console.error("[build] Failed to reload build history:", err);
          });
      }
    } else {
      logStep(
        "APK installed. Could not determine package name — cannot auto-launch. " +
          "Ensure aapt2 is available in your Android SDK (Settings → Android SDK)."
      );
    }
  } catch (e) {
    const msg = formatError(e);
    logError(`Deploy failed: ${msg}`);
    throw e;
  } finally {
    setDeployPhase(null);
    deployInFlight = false;
    showObservedRunWhenIdle();
  }
}

/** Cancel the running build, whoever started it. No-op if no build is running. */
export async function cancelBuild(): Promise<void> {
  if (buildState.phase !== "running") return;

  if (!activeRun && observedRun?.shown) {
    // Another client's build: its build:complete confirms the outcome.
    flushPendingLines();
    cancelBuildState();
    await cancelBuildApi();
    return;
  }

  const run = activeRun;
  activeRun = null;
  earlyCompletions.clear();
  clearBuildCompleteTimer();

  // Flush any buffered log lines before finalising state.
  flushPendingLines();

  cancelBuildState();
  try {
    await cancelBuildApi();
  } finally {
    // Unblock runBuild even when the cancel request fails; the timer that
    // would otherwise release it is already cleared.
    run?.resolve({ success: false, durationMs: 0, recordId: null });
  }
}

/**
 * Show the device picker dialog if no online device is selected, then
 * return the serial of the chosen device. Returns null if the user cancels.
 */
async function resolveDevice(): Promise<string | null> {
  // Check if currently selected device is online.
  const serial = deviceState.selectedSerial;
  if (serial) {
    const dev = deviceState.devices.find((d) => d.serial === serial);
    if (dev?.connectionState === "online") return serial;
  }

  // Import lazily to avoid circular deps.
  const { showDevicePicker } = await import("@/components/device/DevicePickerDialog");
  return showDevicePicker();
}

/**
 * Jump to a build error in Android Studio when file info is available,
 * otherwise show the error in a Toast.
 */
export async function jumpToBuildError(error: BuildError): Promise<void> {
  const { showToast } = await import("@/components/ui");
  const { openInStudio } = await import("@/lib/tauri-api");

  if (error.file) {
    try {
      // openInStudio expects (classPath, filename, line).
      const parts = error.file.replace(/\\/g, "/").split("/");
      const filename = parts[parts.length - 1] ?? error.file;
      // Build a dotted class path from the path relative to java/ or kotlin/.
      const srcIdx = parts.findIndex((p) => p === "java" || p === "kotlin");
      const classPath =
        srcIdx >= 0
          ? parts
              .slice(srcIdx + 1)
              .join(".")
              .replace(/\.(kt|java)$/, "")
          : filename.replace(/\.(kt|java)$/, "");
      await openInStudio(classPath, filename, error.line ?? 1);
      return;
    } catch (e) {
      // Studio may not be running — fall through to Toast.
      console.warn("[build] openInStudio failed, falling back to Toast:", e);
    }
  }

  // Fallback: show the error in a Toast.
  const location = error.file
    ? `${error.file}${error.line !== null ? `:${error.line}` : ""}${error.col !== null ? `:${error.col}` : ""} — `
    : "";
  showToast(`${location}${error.message}`, "info");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Emit a visible info step into the build log (e.g. "Installing APK…"). */
function logStep(message: string): void {
  addBuildLine({ kind: "info", content: `▶ ${message}`, file: null, line: null, col: null });
}

/** Log an environment variable only if its value is set. */
function logEnvVar(name: string, value: string | null | undefined): void {
  if (value) {
    logStep(`${name}: ${value}`);
  }
}

/** Format device label for logging. */
function deviceLabel(serial: string): string {
  const dev = deviceState.devices.find((d) => d.serial === serial);
  if (!dev) return serial;
  const model = dev.model ?? dev.name ?? serial;
  const api = dev.apiLevel !== null ? ` (API ${dev.apiLevel})` : "";
  return `${model}${api} [${serial}]`;
}

function formatDuration(ms: number): string {
  if (!ms) return "0ms";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = ((ms % 60000) / 1000).toFixed(0);
  return `${mins}m ${secs}s`;
}

/** Log build header: task, working directory, and relevant env vars. */
function logBuildHeader(effectiveTask: string): void {
  logStep(`Build started: ${effectiveTask}`);
  const cwd = projectState.gradleRoot ?? projectState.projectRoot;
  if (cwd) logStep(`Working directory: ${cwd}`);
  logEnvVar("JAVA_HOME", settingsState.java?.home);
  logEnvVar("ANDROID_HOME", settingsState.android?.sdkPath);
  logStep(`./gradlew ${effectiveTask} --console=plain`);
}

function assertSameProject(generation: number): void {
  if (generation !== currentProjectGeneration()) {
    throw new Error("The project changed during deploy. Nothing was installed.");
  }
}

/** Emit a visible error into the build log AND the Problems tab. */
function logError(message: string): void {
  addBuildLine({ kind: "error", content: message, file: null, line: null, col: null });
}

function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
