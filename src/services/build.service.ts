import {
  runGradleTask,
  cancelBuild as cancelBuildApi,
  listRunConfigurations,
  resolveRunConfiguration,
  runRunConfiguration,
  isAppErrorKind,
  getBuildHistory,
  listenBuildStarted,
  listenBuildLines,
  listenBuildComplete,
  listenBuildLaunchTiming,
  listenDeployPhase,
  errorMessage,
  formatError,
  type BuildActor,
  type BuildCompleteEvent,
  type BuildLine,
  type BuildLinesEvent,
  type BuildStartedEvent,
  type DeployPhaseEvent,
  type DeployResult,
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
import { buildRunningLabel, runByLabel } from "@/lib/build-actor";
import { describeDisplayTimes, formatLaunchTime } from "@/lib/launch-timing";
import type { BuildError, ResolvedRun, TargetPreference } from "@/bindings";
import {
  launchRunAvd,
  resolveApprovedRun,
  setRunConfigurationRunner,
} from "@/services/run-configurations.service";

let buildUnlisteners: Array<() => void> | null = null;
// Held so concurrent callers await the SAME registration. A plain
// `if (unlisten) return` guard is checked before the await, so two interleaved
// calls both register and the second orphans the first's unlisten.
let buildListenerInit: Promise<void> | null = null;
let currentBuildPromise: Promise<BuildCompletion | null> | null = null;
let deployInFlight = false;
/**
 * The run of a configuration whose build the Build panel shows, and the
 * project generation it ran under: its `deploy:phase` steps are logged, even
 * ones arriving after it answered. Either this window's latest run, or an
 * attached agent's run (known by its build's record once the build shows).
 * Null once another build shows.
 */
type DeployLog =
  { run: "own"; generation: number } | { run: "agent"; buildId: number; generation: number };
let deployLog: DeployLog | null = null;
/** How long a run that answered waits for its build's `build:complete`. */
const BUILD_COMPLETE_GRACE_MS = 5_000;

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
  deployLog = null;
  earlyCompletions.clear();
  clearEarlyLines();
  clearBuildCompleteTimer();
}

async function registerBuildListeners(): Promise<void> {
  const registrations = await Promise.allSettled([
    listenBuildStarted(onBuildStarted),
    listenBuildLines(onBuildLines),
    listenBuildComplete(onBuildComplete),
    listenBuildLaunchTiming(onLaunchTiming),
    listenDeployPhase(onDeployPhase),
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

/**
 * A run of a configuration moved on. This window's own run: log its steps,
 * and show its phase while it runs. An agent's run whose build the panel
 * shows: log its steps and how it ended, without taking it over.
 */
function onDeployPhase(e: DeployPhaseEvent): void {
  const log = deployLog;
  if (!log || log.generation !== currentProjectGeneration()) return;
  if (e.origin.kind === "agent") {
    if (log.run === "agent" && e.buildId === log.buildId) logAgentPhase(e);
    return;
  }
  if (log.run !== "own") return;
  e.steps.forEach(logStep);
  if (!deployInFlight) return;
  if (e.phase === "building" || e.phase === "installing" || e.phase === "launching") {
    setDeployPhase(e.phase);
  }
}

function logAgentPhase(e: DeployPhaseEvent): void {
  e.steps.forEach(logStep);
  const run = runByLabel(e.name, e.origin);
  if (e.phase === "done") logStep(`${run}: done on ${e.device.label}`);
  if (e.phase === "failed") logError(`${run} failed: ${e.error ?? "see the agent's result"}`);
}

/** Display times arrived after a launch returned: the build's record has them now. */
function onLaunchTiming(): void {
  getBuildHistory()
    .then(setBuildHistory)
    .catch((err) => {
      console.error("[build] Failed to reload build history:", err);
    });
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
  // An agent that runs a configuration installs and launches this build next.
  if (run.origin.kind === "agent" && e.recordId !== null) {
    deployLog = { run: "agent", buildId: e.recordId, generation: run.generation };
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
  deployLog = null;
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

  deployLog = null;
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
 * Full build → install → launch cycle of the active run configuration.
 *
 * The configuration is resolved first (task, device, launch); its plan heads
 * the build log. A target of Ask or Last used that finds no online device
 * shows the device picker — skipped entirely when "Auto Install on Build" is
 * off (build-only run). The backend then runs it (`runRunConfiguration`):
 * builds, installs the APK that build wrote, and launches the app the way the
 * configuration says, while this window follows the build and the
 * `deploy:phase` steps.
 */
export async function runAndDeploy(name: string | null = null): Promise<void> {
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
  // The backend refuses the run once another project is open, so it never
  // installs the other project's APK.
  const projectGeneration = currentProjectGeneration();
  // Read once up front: when auto-install is off this is a build-only run,
  // which must not force device selection.
  const autoInstall = settingsState.build.autoInstallOnBuild !== false;

  try {
    // Logged BEFORE startBuild clears the log; the plan then heads it.
    logStep("Resolving the run configuration…");
    const plan = await resolveRunPlan(!autoInstall, name);
    if (!plan) return;
    assertSameProject(projectGeneration);

    setDeployPhase("building");
    if (!autoInstall) {
      // The "Auto Install on Build" setting gates install + launch; the build
      // itself still counts as a successful run when it is off.
      await runBuildGuarded(plan.task, { headerLines: [plan.plan] }, true);
      if (buildState.phase !== "success") {
        logError(`Build phase is "${buildState.phase}" — skipping install.`);
        return;
      }
      logStep("Auto Install on Build is disabled — skipping install and launch.");
      return;
    }

    showRunResult(await runInBackend(plan));
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

/**
 * Have the backend run `plan`, showing its build in the Build panel the way
 * runBuild shows one: the plan and the build header head the log, and the
 * build's output and outcome follow its events.
 */
async function runInBackend(plan: ResolvedRun): Promise<DeployResult> {
  deployLog = { run: "own", generation: currentProjectGeneration() };
  startBuild(plan.task);
  setActiveTab("build");
  addBuildLine({ kind: "info", content: plan.plan, file: null, line: null, col: null });
  logBuildHeader(plan.task);

  const run: ActiveRun = { runId: null, resolve: () => {} };
  const built = new Promise<BuildCompletion>((resolve) => {
    run.resolve = resolve;
  });
  activeRun = run;
  earlyCompletions.clear();
  clearEarlyLines();

  let result: DeployResult | null = null;
  let failure: { error: unknown } | null = null;
  try {
    result = await runRunConfiguration({
      name: plan.name,
      selectedSerial: plan.device?.serial ?? null,
      projectRoot: projectState.projectRoot,
    });
  } catch (e) {
    failure = { error: e };
  }
  // build:complete can arrive after the run answered.
  if (activeRun === run && run.runId !== null) {
    await Promise.race([
      built,
      new Promise((resolve) => setTimeout(resolve, BUILD_COMPLETE_GRACE_MS)),
    ]);
  }
  if (activeRun === run) {
    // Its build never started, or its outcome never arrived.
    activeRun = null;
    earlyCompletions.clear();
    clearEarlyLines();
    flushPendingLines();
    if (buildState.phase === "running") setBuildResult({ success: false, durationMs: 0 });
  }
  if (failure) throw failure.error;
  return result as DeployResult;
}

/** Log how a run ended, and after a launch apply its logcat filter. */
function showRunResult(result: DeployResult): void {
  if (result.outcome !== "done") {
    const phase = result.outcome === "cancelled" ? "cancelled" : "failed";
    logError(`Build phase is "${phase}" — skipping install. Check the Problems tab for errors.`);
    return;
  }
  const launch = result.launch;
  if (!launch || !result.package) return;
  logStep(`Launch: ${launch.output.trim()}`);
  setLastLaunchedAt(Date.now(), result.package, result.logcatFilter);
  if (result.run.launch.kind === "deepLink") {
    logStep("Launch time: not reported for a deep link");
    return;
  }
  logStep(
    launch.timing
      ? `Launch time: ${[formatLaunchTime(launch.timing), ...describeDisplayTimes(launch.timing)].join(" · ")}`
      : "Launch time: not reported by this launch method"
  );
  // The run recorded the launch time on its build.
  if (launch.timing && result.buildId !== null) {
    getBuildHistory()
      .then(setBuildHistory)
      .catch((err) => {
        console.error("[build] Failed to reload build history:", err);
      });
  }
}

/**
 * Build Only: build the task of the run configuration named `name` (default:
 * the active one), with no device, install, or launch.
 */
export async function runBuildOnly(name: string | null = null): Promise<void> {
  assertProjectTrusted();
  if (deployInFlight) throw new Error("A build or deploy is already running.");
  let plan: ResolvedRun | null;
  try {
    plan = await resolveApprovedRun({ name, buildOnly: true });
  } catch (e) {
    logError(`Build failed: ${formatError(e)}`);
    throw e;
  }
  if (!plan) {
    logStep("The shared run configuration was not approved — build cancelled.");
    return;
  }
  await runBuild(plan.task, { headerLines: [plan.plan] });
}

/**
 * Resolve the run configuration named `name` (default: the active one). When
 * its target is Ask or Last used and no device is online for it, ask for one
 * with the device picker; when it runs on an AVD that is not running, offer
 * to launch it. Null when the run stops there.
 */
async function resolveRunPlan(
  buildOnly: boolean,
  name: string | null
): Promise<ResolvedRun | null> {
  let failure: unknown;
  try {
    // The app's selection may be one the device list chose, not yet the backend's.
    const plan = await resolveApprovedRun({
      name,
      buildOnly,
      selectedSerial: deviceState.selectedSerial,
    });
    if (!plan) logStep("The shared run configuration was not approved — run cancelled.");
    return plan;
  } catch (e) {
    failure = e;
  }
  const target = buildOnly ? null : await targetWithoutDevice(failure, name);
  if (target?.kind === "avd") {
    if (await offerToLaunchAvd(target.name, errorMessage(failure))) {
      logStep(`Launching ${target.name} — run again once it is online.`);
      return null;
    }
    throw failure;
  }
  if (target?.kind !== "ask" && target?.kind !== "lastUsed") throw failure;
  // Import lazily to avoid circular deps.
  const { showDevicePicker } = await import("@/components/device/DevicePickerDialog");
  const serial = await showDevicePicker();
  if (!serial) {
    logStep("No device selected — run cancelled.");
    return null;
  }
  return resolveRunConfiguration({ name, selectedSerial: serial });
}

/** The target of a run that found no device for it; null for any other failure. */
async function targetWithoutDevice(
  error: unknown,
  name: string | null
): Promise<TargetPreference | null> {
  if (!isAppErrorKind(error, "notFound")) return null;
  const { active, local } = await listRunConfigurations();
  const chosen = name ?? active;
  if (!chosen) return null;
  // A configuration without local state targets the last used device.
  return local[chosen]?.target ?? { kind: "lastUsed" };
}

/**
 * Ask whether to launch the AVD a run needs. Launching is the user's choice;
 * Keynobi never starts an emulator on its own.
 */
async function offerToLaunchAvd(avdName: string, reason: string): Promise<boolean> {
  const { showDialog } = await import("@/components/ui");
  const choice = await showDialog({
    title: "AVD not running",
    message: reason,
    buttons: [
      { label: "Launch AVD", value: "launch", style: "primary" },
      { label: "Cancel", value: "cancel", style: "secondary" },
    ],
  });
  if (choice !== "launch") return false;
  void launchRunAvd(avdName);
  return true;
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

// Palette actions run and build a configuration by name.
setRunConfigurationRunner({ run: runAndDeploy, build: runBuildOnly });
