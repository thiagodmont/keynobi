import type { AppSettings } from "@/bindings";

export function makeSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
    appearance: { uiFontSize: 14 },
    search: { contextLines: 3, maxResults: 200, maxFiles: 1000 },
    android: { sdkPath: null },
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
    ...overrides,
  };
}
