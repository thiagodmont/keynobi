/**
 * health.store.ts
 *
 * Computes IDE health checks from two sources:
 *  1. Reactive: derived live from existing stores (project, lsp, settings, editor).
 *  2. Active:   results from the `run_health_checks` Tauri command (Java version,
 *               disk space, Gradle wrapper, etc.).
 *
 * The overall health signal is used by the status bar indicator.
 * The full check list is rendered by HealthPanel.
 */

import { createStore } from "solid-js/store";
import { createMemo } from "solid-js";
import { lspState } from "@/stores/lsp.store";
import { projectState } from "@/stores/project.store";
import { settingsState } from "@/stores/settings.store";
import { editorState } from "@/stores/editor.store";
import type { SystemHealthReport } from "@/bindings";

// ── Types ─────────────────────────────────────────────────────────────────────

export type CheckStatus = "ok" | "warning" | "error" | "loading" | "skip";

export interface HealthCheck {
  id: string;
  category: "project" | "environment" | "lsp" | "system";
  name: string;
  status: CheckStatus;
  detail: string;
  /** Optional action the developer can take to fix the issue. */
  fix?: { label: string; action: () => void };
}

export type OverallHealth = "ok" | "warning" | "error" | "loading";

// ── Store ─────────────────────────────────────────────────────────────────────

interface HealthStoreState {
  systemReport: SystemHealthReport | null;
  isRunning: boolean;
  lastCheckedAt: Date | null;
}

const [healthState, setHealthState] = createStore<HealthStoreState>({
  systemReport: null,
  isRunning: false,
  lastCheckedAt: null,
});

export { healthState };

export function setSystemReport(report: SystemHealthReport) {
  setHealthState({ systemReport: report, isRunning: false, lastCheckedAt: new Date() });
}

export function setHealthChecking(running: boolean) {
  setHealthState("isRunning", running);
}

// ── Reactive checks (derived from stores) ────────────────────────────────────

function openSettingsAction(): void {
  import("@/components/settings/SettingsPanel").then(({ openSettings }) =>
    openSettings()
  );
}

export const healthChecks = createMemo<HealthCheck[]>(() => {
  const report = healthState.systemReport;
  const checks: HealthCheck[] = [];

  // "Project Open" is not a health check — it's just context.
  // The project state is used by other checks (Gradle wrapper) but isn't
  // listed as a separate item.
  const projectRoot = projectState.projectRoot as string | null;

  // ── 1. Android SDK ──────────────────────────────────────────────────────────
  const sdkPath = settingsState.android.sdkPath;
  const sdkValid = report?.androidSdkValid;
  checks.push({
    id: "android-sdk",
    category: "environment",
    name: "Android SDK",
    status: !sdkPath
      ? "error"
      : sdkValid === false
      ? "warning"
      : sdkValid === true
      ? "ok"
      : "loading",
    detail: !sdkPath
      ? "ANDROID_HOME not configured — Gradle cannot find the Android SDK"
      : sdkValid === false
      ? `Path set but SDK not found at: ${sdkPath}`
      : sdkValid === true
      ? sdkPath
      : "Checking…",
    fix: !sdkPath
      ? { label: "Open Settings", action: openSettingsAction }
      : undefined,
  });

  // ── 2. ADB ──────────────────────────────────────────────────────────────────
  const adbFound = report?.adbFound;
  const adbVersion = report?.adbVersion;
  checks.push({
    id: "adb",
    category: "environment",
    name: "ADB (Android Debug Bridge)",
    status: adbFound === undefined
      ? "loading"
      : adbFound
      ? "ok"
      : !sdkPath
      ? "skip"
      : "warning",
    detail: adbFound === true
      ? adbVersion ?? "Found in platform-tools"
      : adbFound === false
      ? "adb not found — device operations will not work"
      : !sdkPath
      ? "No SDK configured"
      : "Checking…",
    fix: adbFound === false
      ? { label: "Open Settings", action: openSettingsAction }
      : undefined,
  });

  // ── 2b. Emulator ─────────────────────────────────────────────────────────────
  const emulatorFound = report?.emulatorFound;
  checks.push({
    id: "emulator",
    category: "environment",
    name: "Android Emulator",
    status: emulatorFound === undefined
      ? "loading"
      : emulatorFound
      ? "ok"
      : !sdkPath
      ? "skip"
      : "warning",
    detail: emulatorFound === true
      ? "Found in $ANDROID_HOME/emulator/"
      : emulatorFound === false
      ? "Emulator binary not found — cannot launch AVDs"
      : !sdkPath
      ? "No SDK configured"
      : "Checking…",
    fix: emulatorFound === false
      ? { label: "Open Settings", action: openSettingsAction }
      : undefined,
  });

  // ── 3. Java / JDK ──────────────────────────────────────────────────────────
  const javaHome = settingsState.java.home;
  const javaFound = report?.javaExecutableFound;
  const javaVersion = report?.javaVersion;
  checks.push({
    id: "java",
    category: "environment",
    name: "Java / JDK",
    status: javaFound === false
      ? "error"
      : javaFound === true
      ? "ok"
      : !javaHome
      ? "warning"
      : "loading",
    detail: javaFound === true
      ? javaVersion ?? `Found at ${report?.javaBinUsed}`
      : javaFound === false
      ? `Not found at: ${report?.javaBinUsed ?? "java"} — Gradle compilation may fail`
      : !javaHome
      ? "Not configured — Gradle may use an incompatible JVM"
      : "Checking…",
    fix: javaFound === false || !javaHome
      ? { label: "Open Settings", action: openSettingsAction }
      : undefined,
  });

  // ── 4. Kotlin LSP ───────────────────────────────────────────────────────────
  const lspStatusState = lspState.status.state;
  const lspMsg = lspState.status.message;

  // "stopped" is only an error when a project IS open — the LSP should have
  // auto-started.  With no project open it's expected/idle, not an error.
  const lspStatus = (): CheckStatus => {
    switch (lspStatusState) {
      case "ready":       return "ok";
      case "indexing":
      case "starting":
      case "downloading": return "warning";
      case "error":       return "error";
      case "notInstalled":return projectRoot ? "error" : "warning";
      case "stopped":     return projectRoot ? "error" : "skip";
      default:            return "warning";
    }
  };

  const lspDetail: Record<string, string> = {
    ready: "Running — language intelligence active",
    indexing: lspMsg ?? "Indexing project…",
    starting: "Starting server…",
    downloading: "Downloading Kotlin Language Server…",
    error: lspMsg ?? "Server error — check Output panel for details",
    stopped: projectRoot
      ? "Not started — open a project to auto-start"
      : "Waiting for a project to be opened",
    notInstalled: projectRoot
      ? "Kotlin Language Server not installed"
      : "Not installed — will be downloaded when you open a project",
  };
  checks.push({
    id: "kotlin-lsp",
    category: "lsp",
    name: "Kotlin LSP",
    status: lspStatus(),
    detail: lspDetail[lspStatusState] ?? lspStatusState,
    // Only show a fix action when there's actually something actionable.
    fix: lspStatusState === "error"
      ? { label: "Open Settings → Tools", action: openSettingsAction }
      : lspStatusState === "notInstalled" && !!projectRoot
      ? { label: "Open Settings → Tools", action: openSettingsAction }
      : undefined,
  });

  // ── 5. Code Navigation ──────────────────────────────────────────────────────
  const caps = lspState.serverCapabilities;
  const hasDefinition = caps?.definitionProvider;
  const hasReferences = caps?.referencesProvider;
  const hasHover = caps?.hoverProvider;
  const navFeatures = [
    hasDefinition ? "Definition" : null,
    hasReferences ? "References" : null,
    hasHover ? "Hover" : null,
  ].filter(Boolean);
  checks.push({
    id: "lsp-navigation",
    category: "lsp",
    name: "Code Navigation",
    status:
      lspStatusState !== "ready"
        ? "skip"
        : caps === null
        ? "loading"
        : !hasDefinition
        ? "warning"
        : "ok",
    detail:
      lspStatusState !== "ready"
        ? "Waiting for LSP server to be ready"
        : caps === null
        ? "Waiting for server capabilities…"
        : navFeatures.length === 0
        ? "No navigation capabilities reported by server"
        : `${navFeatures.join(" · ")} available`,
  });

  // ── 6. File Registration ────────────────────────────────────────────────────
  const openKotlinFiles = Object.values(editorState.openFiles).filter(
    (f) => f.language === "kotlin" || f.language === "gradle"
  );
  const lspReady = lspStatusState === "ready";
  checks.push({
    id: "file-registration",
    category: "lsp",
    name: "File Registration",
    status:
      openKotlinFiles.length === 0
        ? "skip"
        : !lspReady
        ? "warning"
        : "ok",
    detail:
      openKotlinFiles.length === 0
        ? "No Kotlin files open"
        : !lspReady
        ? `${openKotlinFiles.length} file(s) waiting — will register once LSP is ready`
        : `${openKotlinFiles.length} file(s) registered with LSP server`,
  });

  // ── 7. Gradle Wrapper ───────────────────────────────────────────────────────
  const gradleFound = report?.gradleWrapperFound;
  checks.push({
    id: "gradle-wrapper",
    category: "project",
    name: "Gradle Wrapper",
    status: !projectRoot
      ? "skip"
      : gradleFound === undefined
      ? "loading"
      : gradleFound
      ? "ok"
      : "warning",
    detail: !projectRoot
      ? "No project open"
      : gradleFound === undefined
      ? "Checking…"
      : gradleFound
      ? "gradlew found — Gradle integration enabled"
      : "gradlew not found at project root — Gradle may not run",
  });

  // ── 8. Disk Space ───────────────────────────────────────────────────────────
  const diskMb = report?.diskFreeMb;
  const diskStatus: CheckStatus =
    diskMb === undefined || diskMb === null
      ? "loading"
      : diskMb < 200
      ? "error"
      : diskMb < 1024
      ? "warning"
      : "ok";
  checks.push({
    id: "disk-space",
    category: "system",
    name: "Disk Space",
    status: diskStatus,
    detail:
      diskMb == null
        ? "Checking available space in ~/.androidide…"
        : diskMb < 200
        ? `Only ${diskMb} MB free — LSP indices may fail to write`
        : diskMb < 1024
        ? `${diskMb} MB free — sufficient but getting low`
        : `${diskMb} MB free — plenty of space`,
  });

  // ── 9. LSP System Directory ─────────────────────────────────────────────────
  const lspDirOk = report?.lspSystemDirOk;
  checks.push({
    id: "lsp-system-dir",
    category: "system",
    name: "LSP Data Directory",
    status:
      lspDirOk === undefined ? "loading" : lspDirOk ? "ok" : "error",
    detail:
      lspDirOk === undefined
        ? "Checking ~/.androidide/lsp-system/…"
        : lspDirOk
        ? "~/.androidide/lsp-system/ is writable"
        : "Cannot create ~/.androidide/lsp-system/ — check file permissions",
  });

  return checks;
});

// ── Overall health ────────────────────────────────────────────────────────────

export const overallHealth = createMemo<OverallHealth>(() => {
  if (healthState.isRunning) return "loading";
  const checks = healthChecks();
  const active = checks.filter((c) => c.status !== "skip");
  if (active.some((c) => c.status === "error")) return "error";
  if (active.some((c) => c.status === "warning" || c.status === "loading")) return "warning";
  return "ok";
});

export const healthSummary = createMemo(() => {
  const checks = healthChecks().filter((c) => c.status !== "skip");
  const ok = checks.filter((c) => c.status === "ok").length;
  const total = checks.length;
  return { ok, total };
});
