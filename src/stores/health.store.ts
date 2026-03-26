/**
 * health.store.ts
 *
 * Computes IDE health checks from two sources:
 *  1. Reactive: derived live from existing stores (project, settings).
 *  2. Active:   results from the `run_health_checks` Tauri command (Java version,
 *               disk space, Gradle wrapper, etc.).
 *
 * The overall health signal is used by the status bar indicator.
 * The full check list is rendered by HealthPanel.
 */

import { createStore } from "solid-js/store";
import { createMemo } from "solid-js";
import { projectState } from "@/stores/project.store";
import { settingsState } from "@/stores/settings.store";
import type { SystemHealthReport } from "@/bindings";

// ── Types ─────────────────────────────────────────────────────────────────────

export type CheckStatus = "ok" | "warning" | "error" | "loading" | "skip";

export interface HealthCheck {
  id: string;
  category: "project" | "environment" | "system";
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

  // ── 3. Emulator ─────────────────────────────────────────────────────────────
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

  // ── 4. Java / JDK ──────────────────────────────────────────────────────────
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

  // ── 5. Gradle Wrapper ───────────────────────────────────────────────────────
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

  // ── 6. Disk Space ───────────────────────────────────────────────────────────
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
        ? `Only ${diskMb} MB free — build artifacts may fail to write`
        : diskMb < 1024
        ? `${diskMb} MB free — sufficient but getting low`
        : `${diskMb} MB free — plenty of space`,
  });

  // ── 7. App Directory ────────────────────────────────────────────────────────
  const appDirOk = report?.lspSystemDirOk;
  checks.push({
    id: "app-dir",
    category: "system",
    name: "App Data Directory",
    status:
      appDirOk === undefined ? "loading" : appDirOk ? "ok" : "error",
    detail:
      appDirOk === undefined
        ? "Checking ~/.androidide/…"
        : appDirOk
        ? "~/.androidide/ is writable"
        : "Cannot create ~/.androidide/ — check file permissions",
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
