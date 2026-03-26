import {
  runGradleTask,
  cancelBuild as cancelBuildApi,
  finalizeBuild,
  findApkPath,
  installApkOnDevice,
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
  buildState,
} from "@/stores/build.store";
import { variantState } from "@/stores/variant.store";
import { deviceState } from "@/stores/device.store";
import { openFileAtLocation } from "@/services/project.service";
import { setActiveBottomTab, setUIState } from "@/stores/ui.store";
import type { BuildError } from "@/bindings";

let buildCompleteUnlisten: (() => void) | null = null;

// ── Registration ──────────────────────────────────────────────────────────────

/** Call once on app startup to register the build:complete event listener. */
export async function initBuildService(): Promise<void> {
  if (buildCompleteUnlisten) return;
  buildCompleteUnlisten = await listenBuildComplete((e) => {
    setBuildResult({
      success: e.success,
      durationMs: e.durationMs,
      errorCount: e.errorCount,
      warningCount: e.warningCount,
    });
    // Reload history from backend.
    getBuildHistory()
      .then(setBuildHistory)
      .catch(() => {});
  });
}

// ── Build actions ─────────────────────────────────────────────────────────────

/**
 * Run a Gradle task and stream output into the build panel.
 *
 * Opens the build panel and switches to the Build tab automatically.
 */
export async function runBuild(task?: string): Promise<void> {
  const variant = variantState.activeVariant;
  const effectiveTask = task ?? (variant ? `assemble${capitalize(variant)}` : "assembleDebug");

  startBuild(effectiveTask);

  // Open bottom panel and switch to Build tab.
  setUIState("bottomPanelVisible", true);
  setActiveBottomTab("build");

  const startedAt = new Date().toISOString();
  const accumulatedErrors: BuildError[] = [];

  try {
    await runGradleTask(effectiveTask, (line: BuildLine) => {
      addBuildLine(line);

      // Accumulate errors for finalizeBuild call.
      if (
        (line.kind === "error" || line.kind === "warning") &&
        line.file &&
        line.line != null
      ) {
        accumulatedErrors.push({
          message: line.content,
          file: line.file,
          line: line.line,
          col: line.col ?? null,
          severity: line.kind === "error" ? "error" : "warning",
        });
      }
    });
  } catch (e) {
    // Process-level error (e.g. gradlew not found).
    setBuildResult({ success: false, durationMs: 0, errorCount: 1, warningCount: 0 });
    throw e;
  }

  // The build:complete event will call setBuildResult.
  // We call finalizeBuild here to persist history from the frontend's perspective.
  await finalizeBuild({
    success: buildState.phase === "success",
    durationMs: buildState.durationMs ?? 0,
    errors: accumulatedErrors,
    task: effectiveTask,
    startedAt,
  }).catch(() => {});
}

/**
 * Full build → install → launch cycle.
 *
 * Assembles the active variant, finds the output APK, installs it on
 * the selected device, and launches the app.
 */
export async function runAndDeploy(): Promise<void> {
  const variant = variantState.activeVariant;
  if (!variant) {
    throw new Error("No build variant selected. Open Build → Select Variant.");
  }
  const serial = deviceState.selectedSerial;
  if (!serial) {
    throw new Error("No device selected. Connect a device or start an emulator.");
  }

  // 1. Build.
  await runBuild(`assemble${capitalize(variant)}`);
  if (buildState.phase !== "success") {
    throw new Error("Build failed — check the Build panel for errors.");
  }

  // 2. Find APK.
  const apkPath = await findApkPath(variant);
  if (!apkPath) {
    throw new Error(
      "Could not locate the output APK. The build may have succeeded but the APK path is unexpected."
    );
  }

  // 3. Install.
  await installApkOnDevice(serial, apkPath);

  // 4. Launch (package name heuristic — can be improved with manifest parsing).
  // For now we surface an informational log line; actual launch requires the package name.
  addBuildLine({
    kind: "output",
    content: `APK installed on ${serial}. Use 'Launch App' to start it.`,
    file: null,
    line: null,
    col: null,
  });
}

/** Cancel the currently running build. */
export async function cancelBuild(): Promise<void> {
  cancelBuildState();
  await cancelBuildApi();
}

/**
 * Jump to a build error location in the editor.
 *
 * Uses `openFileAtLocation` to push the location onto the navigation history.
 */
export async function jumpToBuildError(error: BuildError): Promise<void> {
  await openFileAtLocation(error.file, error.line - 1, (error.col ?? 1) - 1);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
