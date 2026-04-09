import {
  runGradleTask,
  cancelBuild as cancelBuildApi,
  finalizeBuild,
  findApkPath,
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
  buildState,
} from "@/stores/build.store";
import { variantState } from "@/stores/variant.store";
import { deviceState } from "@/stores/device.store";
import { setActiveTab } from "@/stores/ui.store";
import { projectState } from "@/stores/project.store";
import { settingsState } from "@/stores/settings.store";
import type { BuildError } from "@/bindings";

let buildCompleteUnlisten: (() => void) | null = null;

// ── Registration ──────────────────────────────────────────────────────────────

/** Call once on app startup to register the build:complete event listener. */
export async function initBuildService(): Promise<void> {
  if (buildCompleteUnlisten) return;
  buildCompleteUnlisten = await listenBuildComplete((e) => {
    // Flush any lines still in the 50ms buffer before updating phase.
    flushPendingLines();

    if (e.cancelled) {
      // Build was explicitly cancelled by the user — use dedicated cancelled phase.
      cancelBuildState();
    } else {
      setBuildResult({
        success: e.success,
        durationMs: e.durationMs,
        errorCount: e.errorCount,
        warningCount: e.warningCount,
      });
    }
    // Reload history from backend.
    getBuildHistory()
      .then(setBuildHistory)
      .catch((err) => {
        console.error("[build] Failed to reload build history:", err);
      });
    // Resolve the pending build promise if there is one.
    _resolveBuildComplete?.({ success: e.success, durationMs: e.durationMs });
    _resolveBuildComplete = null;
  });
}

// One-shot resolver for the current build. Set before a build starts, cleared on completion.
let _resolveBuildComplete: ((result: { success: boolean; durationMs: number }) => void) | null = null;

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
export async function runBuild(task?: string, opts?: { headerLines?: string[] }): Promise<void> {
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

  const startedAt = new Date().toISOString();
  const accumulatedErrors: BuildError[] = [];

  // Create a promise that resolves when the build:complete event fires.
  // A 5-minute timeout prevents the deploy from hanging forever if
  // something goes wrong in the Rust on_exit callback.
  const buildComplete = new Promise<{ success: boolean; durationMs: number }>((resolve, reject) => {
    _resolveBuildComplete = resolve;
    setTimeout(() => {
      if (_resolveBuildComplete === resolve) {
        _resolveBuildComplete = null;
        reject(new Error("Build timed out waiting for build:complete event after 5 minutes."));
      }
    }, 5 * 60 * 1000);
  });

  try {
    await runGradleTask(effectiveTask, (line: BuildLine) => {
      addBuildLine(line);

      if (line.kind === "error" || line.kind === "warning") {
        accumulatedErrors.push({
          message: line.content,
          file: line.file ?? null,
          line: line.line ?? null,
          col: line.col ?? null,
          severity: line.kind === "error" ? "error" : "warning",
        });
      }
    });
  } catch (e) {
    // Process-level spawn failure (e.g. gradlew not found).
    _resolveBuildComplete = null;
    const msg = formatError(e);
    addBuildLine({ kind: "error", content: `Failed to start Gradle: ${msg}`, file: null, line: null, col: null });
    setBuildResult({ success: false, durationMs: 0, errorCount: 1, warningCount: 0 });
    throw e;
  }

  // runGradleTask resolves right after spawn; wait for the actual completion event.
  let result: { success: boolean; durationMs: number };
  try {
    result = await buildComplete;
  } catch (e) {
    // Timeout or unexpected rejection.
    const msg = formatError(e);
    addBuildLine({ kind: "error", content: `Build event error: ${msg}`, file: null, line: null, col: null });
    setBuildResult({ success: false, durationMs: 0, errorCount: 1, warningCount: 0 });
    throw e;
  }

  // cancelBuild() already called finalizeBuild — don't duplicate.
  if (buildState.phase === "cancelled") return;

  // Persist finalized result + history to the backend.
  await finalizeBuild({
    success: result.success,
    durationMs: result.durationMs,
    errors: accumulatedErrors,
    task: effectiveTask,
    startedAt,
  }).catch((err) => {
    console.error("[build] Failed to finalize build:", err);
  });
}

/**
 * Full build → install → launch cycle.
 *
 * If no device is selected, resolves a device via the DevicePickerDialog.
 * After a successful build the APK is installed and the app launched.
 */
export async function runAndDeploy(): Promise<void> {
  const variant = variantState.activeVariant;
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

  try {
    // 1. Build. startBuild() inside runBuild() clears the log, so we add a
    //    context header as the very first callback line from the Gradle channel.
    setDeployPhase("building");
    await runBuild(`assemble${capitalize(variant)}`, {
      headerLines: [
        `── Deploy: ${variant} → ${serial} ──`,
      ],
    });

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

    // 4. Launch.
    const appId = projectState.applicationId;
    if (appId) {
      setDeployPhase("launching");
      logStep(`adb shell monkey -p ${appId} 1`);
      const launchOutput = await launchAppOnDevice(serial, appId);
      logStep(`Launch: ${launchOutput.trim()}`);
    } else {
      logStep(
        "APK installed. applicationId not found in project — cannot auto-launch. " +
        "Open 'Project App Info' to verify your applicationId is set."
      );
    }
  } catch (e) {
    const msg = formatError(e);
    logError(`Deploy failed: ${msg}`);
    throw e;
  } finally {
    setDeployPhase(null);
  }
}

/** Cancel the currently running build. */
export async function cancelBuild(): Promise<void> {
  const resolve = _resolveBuildComplete;
  _resolveBuildComplete = null;

  // Flush any buffered log lines before finalising state.
  flushPendingLines();

  const task = buildState.currentTask ?? "unknown";
  const startedAtMs = buildState.startedAt ?? Date.now();
  const startedAt = new Date(startedAtMs).toISOString();
  const errors = buildState.errors.slice(); // snapshot before state is mutated
  const durationMs = Date.now() - startedAtMs;

  cancelBuildState();
  await cancelBuildApi();

  // Persist the cancelled build to history so it appears in the side panel.
  await finalizeBuild({
    success: false,
    durationMs,
    errors,
    task,
    startedAt,
  }).catch((err) => {
    console.error("[build] Failed to finalize cancelled build:", err);
  });

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
  const { showToast } = await import("@/components/common/Toast");
  const { openInStudio } = await import("@/lib/tauri-api");

  if (error.file) {
    try {
      // openInStudio expects (classPath, filename, line).
      const parts = error.file.replace(/\\/g, "/").split("/");
      const filename = parts[parts.length - 1] ?? error.file;
      // Build a dotted class path from the path relative to java/ or kotlin/.
      const srcIdx = parts.findIndex((p) => p === "java" || p === "kotlin");
      const classPath = srcIdx >= 0
        ? parts.slice(srcIdx + 1).join(".").replace(/\.(kt|java)$/, "")
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
    ? `${error.file}${error.line != null ? `:${error.line}` : ""}${error.col != null ? `:${error.col}` : ""} — `
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
  const api = dev.apiLevel != null ? ` (API ${dev.apiLevel})` : "";
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
