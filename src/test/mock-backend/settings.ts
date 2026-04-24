import type { AppSettings } from "@/bindings";

export const defaultSettings: AppSettings = {
  appearance: { uiFontSize: 14 },
  search: { contextLines: 3, maxResults: 200, maxFiles: 1000 },
  android: { sdkPath: "/mock/sdk" },
  lsp: { logLevel: "info", requestTimeoutSec: 30 },
  java: { home: null },
  advanced: {
    treeSitterCacheSize: 100,
    lspMaxMessageSizeMb: 32,
    watcherDebounceMs: 300,
    lspDidChangeDebounceMs: 500,
    diagnosticsPullDelayMs: 500,
    hoverDelayMs: 300,
    navigationHistoryDepth: 50,
    recentFilesLimit: 20,
    logRetentionDays: 7,
    logMaxSizeMb: 500,
  },
  build: {
    autoInstallOnBuild: false,
    autoScrollBuildLog: true,
    buildLogRetentionDays: 7,
    buildLogMaxFolderMb: 100,
  },
  logcat: {
    autoStart: false,
    autoScrollToEnd: true,
    maxUiLines: 5000,
    ringMaxEntries: 50000,
  },
  mcp: {
    autoStart: false,
    buildTimeoutSec: 300,
    logcatDefaultCount: 200,
    buildLogDefaultLines: 500,
  },
  telemetry: { enabled: false },
  onboardingCompleted: true,
  recentProjects: [],
  lastActiveProject: null,
};

const settingsOverrides = (
  globalThis as typeof globalThis & {
    __keynobi_e2e_settings_overrides?: Partial<AppSettings>;
  }
).__keynobi_e2e_settings_overrides;

let currentSettings: AppSettings = {
  ...defaultSettings,
  ...(settingsOverrides ?? {}),
};

export function settingsHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    get_settings: () => ({ ...currentSettings }),
    save_settings: (args: unknown) => {
      const { settings } = args as { settings: AppSettings };
      currentSettings = { ...settings };
    },
    get_default_settings: () => ({ ...defaultSettings }),
    reset_settings: () => {
      currentSettings = { ...defaultSettings };
      return { ...defaultSettings };
    },
    detect_sdk_path: () => "/mock/sdk",
    detect_java_path: () => null,
    run_health_checks: () => ({
      javaExecutableFound: true,
      javaVersion: 'openjdk version "17.0.0"',
      javaBinUsed: "/mock/java",
      androidSdkValid: true,
      adbFound: true,
      adbVersion: "Android Debug Bridge version 1.0.41",
      emulatorFound: true,
      gradleWrapperFound: true,
      lspSystemDirOk: true,
      studioCommandFound: false,
    }),
    send_native_sentry_test_event: () => undefined,
  };
}
