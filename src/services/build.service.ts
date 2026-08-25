import {
  runGradleTask,
  cancelBuild as cancelBuildApi,
  findApkPath,
  getPackageNameFromApk,
  installApkOnDevice,
  launchAppOnDevice,
  getBuildHistory,
  listenBuildComplete,
  formatError,
  type BuildLine,
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
} from "@/stores/build.store";
import { variantState } from "@/stores/variant.store";
import { deviceState } from "@/stores/device.store";
import { setActiveTab } from "@/stores/ui.store";
import { projectState } from "@/stores/project.store";
import { settingsState } from "@/stores/settings.store";
import type { BuildError } from "@/bindings";

let buildCompleteUnlisten: (() => void) | null = null;
// Held so concurrent callers await the SAME registration. A plain
// `if (unlisten) return` guard is checked before the await, so two interleaved
// calls both register and the second orphans the first's unlisten.
let buildListenerInit: Promise<void> | null = null;
let currentBuildPromise: Promise<void> | null = null;
let deployInFlight = false;

interface RunBuildOptions {
  headerLines?: string[];
}

// ── Registration ──────────────────────────────────────────────────────────────

/** Call once on app startup to register the build:complete event listener. */
export function initBuildService(): Promise<void> {
  if (buildListenerInit) return buildListenerInit;

  buildListenerInit = registerBuildCompleteListener().catch((err) => {
    // Allow a later retry rather than wedging the service permanently.
    buildListenerInit = null;
    throw err;
  });
  return buildListenerInit;
}

/** Test-only teardown, mirroring resetMcpListenersForTests. */
export function resetBuildServiceForTests(): void {
  buildCompleteUnlisten?.();
  buildCompleteUnlisten = null;
  buildListenerInit = null;
}

async function registerBuildCompleteListener(): Promise<void> {
  const unlisten = await listenBuildComplete((e) => {
    // Flush any lines still in the 50ms buffer before updating phase.
    flushPendingLines();

    if (e.cancelled) {
      if (_completionTimedOut) {
        // Late event from the process we cancelled after the completion
        // timeout — keep the timeout failure instead of flipping the UI to
        // a plain user cancellation.
        _completionTimedOut = false;
      } else {
        // Build was explicitly cancelled by the user — use dedicated cancelled phase.
        cancelBuildState();
      }
    } else {
      setBuildResult({ success: e.success, durationMs: e.durationMs });
    }
    // Resolve the pending build promise if there is one.
    // Rust records history before emitting build:complete, so this fetch sees
    // the completed record without a frontend finalize step.
    getBuildHistory()
      .then(setBuildHistory)
      .catch((err) => {
        console.error("[build] Failed to reload build history:", err);
      });
    clearBuildCompleteTimer();
    _resolveBuildComplete?.({ success: e.success, durationMs: e.durationMs });
    _resolveBuildComplete = null;
  });

  if (buildCompleteUnlisten) {
    // A reset landed while we were awaiting — drop this registration.
    unlisten();
    return;
  }
  buildCompleteUnlisten = unlisten;

  // Load persisted history on startup so previous sessions are visible immediately.
  getBuildHistory()
    .then(setBuildHistory)
    .catch((err) => {
      console.error("[build] Failed to load initial build history:", err);
    });
}

// One-shot resolver for the current build. Set before a build starts, cleared on completion.
let _resolveBuildComplete: ((result: { success: boolean; durationMs: number }) => void) | null =
  null;
let _buildCompleteTimer: ReturnType<typeof setTimeout> | null = null;
// Set when the completion timer fired and we cancelled the still-running
// Gradle process. The process exit then emits a late `build:complete`
// with `cancelled: true`; the listener must not let it overwrite the
// timeout failure already shown to the user.
let _completionTimedOut = false;

function clearBuildCompleteTimer(): void {
  if (_buildCompleteTimer !== null) {
    clearTimeout(_buildCompleteTimer);
    _buildCompleteTimer = null;
  }
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
  return runBuildGuarded(task, opts, false);
}

async function runBuildGuarded(
  task: string | undefined,
  opts: RunBuildOptions | undefined,
  allowDuringDeploy: boolean
): Promise<void> {
  if (deployInFlight && !allowDuringDeploy) {
    throw new Error("A build or deploy is already running.");
  }
  if (currentBuildPromise || buildState.phase === "running") {
    throw new Error("A build is already running.");
  }

  const promise = runBuildInternal(task, opts);
  currentBuildPromise = promise;
  try {
    await promise;
  } finally {
    if (currentBuildPromise === promise) {
      currentBuildPromise = null;
    }
  }
}

async function runBuildInternal(task?: string, opts?: RunBuildOptions): Promise<void> {
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
  const buildComplete = new Promise<{ success: boolean; durationMs: number }>((resolve, reject) => {
    _resolveBuildComplete = resolve;
    _completionTimedOut = false;
    _buildCompleteTimer = setTimeout(() => {
      if (_resolveBuildComplete === resolve) {
        _resolveBuildComplete = null;
        _buildCompleteTimer = null;
        reject(
          new Error(
            `Build timed out waiting for the build:complete event after ${buildTimeoutSec} seconds.`
          )
        );
      }
    }, buildTimeoutSec * 1000);
  });

  try {
    await runGradleTask(effectiveTask, (line: BuildLine) => {
      addBuildLine(line);
    });
  } catch (e) {
    // Process-level spawn failure (e.g. gradlew not found).
    _resolveBuildComplete = null;
    clearBuildCompleteTimer();
    const msg = formatError(e);
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
  try {
    await buildComplete;
  } catch (e) {
    clearBuildCompleteTimer();
    // The only rejection path is the completion timeout: Gradle is still
    // running. Cancel it so the shared build slot is released. The flag
    // stays set until the listener consumes the late cancelled
    // build:complete emitted by the dying process (or the next build
    // starts) so that event cannot overwrite the timeout failure.
    _completionTimedOut = true;
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
  if (buildState.phase === "cancelled") return;

  getBuildHistory()
    .then(setBuildHistory)
    .catch((err) => {
      console.error("[build] Failed to reload build history:", err);
    });
}

/**
 * Full build → install → launch cycle.
 *
 * If no device is selected, resolves a device via the DevicePickerDialog.
 * After a successful build the APK is installed and the app launched.
 */
export async function runAndDeploy(): Promise<void> {
  if (
    deployInFlight ||
    currentBuildPromise ||
    buildState.phase === "running" ||
    buildState.deployPhase
  ) {
    throw new Error("A build or deploy is already running.");
  }

  deployInFlight = true;
  const variant = variantState.activeVariant;

  try {
    if (!variant) {
      throw new Error("No build variant selected. Open Build → Select Variant.");
    }

    // Resolve a device before the build so we can bail early.
    // We log this BEFORE startBuild clears the log — that's intentional; users
    // will see the context when the build panel opens.
    logStep("Resolving target device…");
    const serial = await resolveDevice();
    if (!serial) {
      logStep("No device selected — run cancelled.");
      return;
    }
    logStep(`Target device: ${serial}`);

    // 1. Build. startBuild() inside runBuild() clears the log, so we add a
    //    context header as the very first callback line from the Gradle channel.
    setDeployPhase("building");
    await runBuildGuarded(
      `assemble${capitalize(variant)}`,
      {
        headerLines: [`── Deploy: ${variant} → ${serial} ──`],
      },
      true
    );

    const phase = buildState.phase;
    if (phase !== "success") {
      logError(`Build phase is "${phase}" — skipping install. Check the Problems tab for errors.`);
      setDeployPhase(null);
      return;
    }

    // 2. Find APK.
    logStep(`Searching for APK (variant: ${variant})…`);
    const apkPath = await findApkPath(variant);
    if (!apkPath) {
      logError(
        `APK not found for variant "${variant}". ` +
          "Expected: app/build/outputs/apk/. Make sure the build produced an APK."
      );
      throw new Error("APK not found.");
    }
    logStep(`APK: ${apkPath}`);

    // 3. Install.
    setDeployPhase("installing");
    const deviceInfo = deviceLabel(serial);
    logStep(`Installing on: ${deviceInfo}`);
    logStep(`adb install ${apkPath}`);
    const installStart = Date.now();
    const installOutput = await installApkOnDevice(serial, apkPath);
    logStep(`Install: ${installOutput.trim()} (${formatDuration(Date.now() - installStart)})`);

    // 4. Launch — resolve exact package name from the APK binary.
    setDeployPhase("launching");
    let packageName: string | null = null;
    try {
      packageName = await getPackageNameFromApk(apkPath);
      logStep(`Package (from APK): ${packageName}`);
    } catch (e) {
      // aapt2 unavailable or failed — fall back to applicationId from project.
      const fallback = projectState.applicationId;
      if (fallback) {
        logStep(`aapt2 unavailable (${formatError(e)}), using applicationId: ${fallback}`);
        packageName = fallback;
      }
    }

    if (packageName) {
      logStep(`adb shell am start (package: ${packageName})`);
      const launchOutput = await launchAppOnDevice(serial, packageName);
      logStep(`Launch: ${launchOutput.trim()}`);
      setLastLaunchedAt(Date.now(), packageName);
    } else {
      logStep(
        "APK installed. Could not determine package name — cannot auto-launch. " +
          "Ensure aapt2 is available in your Android SDK or set applicationId in Project App Info."
      );
    }
  } catch (e) {
    const msg = formatError(e);
    logError(`Deploy failed: ${msg}`);
    throw e;
  } finally {
    setDeployPhase(null);
    deployInFlight = false;
  }
}

/** Cancel the currently running build. No-op if no build is running. */
export async function cancelBuild(): Promise<void> {
  if (buildState.phase !== "running") return;

  const resolve = _resolveBuildComplete;
  _resolveBuildComplete = null;
  clearBuildCompleteTimer();

  // Flush any buffered log lines before finalising state.
  flushPendingLines();

  cancelBuildState();
  await cancelBuildApi();

  // Unblock runBuild immediately so it doesn't hang until timeout.
  resolve?.({ success: false, durationMs: 0 });
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

/** Emit a visible error into the build log AND the Problems tab. */
function logError(message: string): void {
  addBuildLine({ kind: "error", content: message, file: null, line: null, col: null });
}

function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
