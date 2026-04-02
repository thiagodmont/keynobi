import {
  runGradleTask,
  cancelBuild as cancelBuildApi,
  finalizeBuild,
  findApkPath,
  installApkOnDevice,
  launchAppOnDevice,
  getBuildHistory,
  listenBuildComplete,
  type BuildLine,
} from "@/lib/tauri-api";
import {
  startBuild,
  addBuildLine,
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
import type { BuildError } from "@/bindings";

let buildCompleteUnlisten: (() => void) | null = null;

// ── Registration ──────────────────────────────────────────────────────────────

/** Call once on app startup to register the build:complete event listener. */
export async function initBuildService(): Promise<void> {
  if (buildCompleteUnlisten) return;
  buildCompleteUnlisten = await listenBuildComplete((e) => {
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
    const msg = e instanceof Error ? e.message : String(e);
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
    const msg = e instanceof Error ? e.message : String(e);
    addBuildLine({ kind: "error", content: `Build event error: ${msg}`, file: null, line: null, col: null });
    setBuildResult({ success: false, durationMs: 0, errorCount: 1, warningCount: 0 });
    throw e;
  }

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
    logStep(`adb install ${apkPath}`);
    const installOutput = await installApkOnDevice(serial, apkPath);
    logStep(`Install: ${installOutput.trim()}`);

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
    const msg = e instanceof Error ? e.message : String(e);
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
 * Jump to a build error location.
 */
export async function jumpToBuildError(error: BuildError): Promise<void> {
  const { showToast } = await import("@/components/common/Toast");
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

/** Emit a visible error into the build log AND the Problems tab. */
function logError(message: string): void {
  addBuildLine({ kind: "error", content: message, file: null, line: null, col: null });
}

function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
