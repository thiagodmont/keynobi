import { createStore } from "solid-js/store";
import { createEffect } from "solid-js";
import {
  getSettings,
  saveSettings as saveSettingsIpc,
  resetSettingsToDefaults,
  type AppSettings,
  formatError,
} from "@/lib/tauri-api";
import { listen } from "@tauri-apps/api/event";
import { showToast } from "@/components/ui";

export type { AppSettings };

const DEFAULT_SETTINGS: AppSettings = {
  appearance: { uiFontSize: 12 },
  search: { contextLines: 2, maxResults: 10_000, maxFiles: 500 },
  android: { sdkPath: null },
  lsp: { logLevel: "INFO", requestTimeoutSec: 30 },
  java: { home: null },
  advanced: {
    treeSitterCacheSize: 50,
    lspMaxMessageSizeMb: 64,
    watcherDebounceMs: 200,
    lspDidChangeDebounceMs: 300,
    diagnosticsPullDelayMs: 1000,
    hoverDelayMs: 500,
    navigationHistoryDepth: 50,
    recentFilesLimit: 20,
    logRetentionDays: 7,
    logMaxSizeMb: 500,
  },
  build: {
    autoInstallOnBuild: true,
    buildLogRetentionDays: 7,
    buildLogMaxFolderMb: 100,
  },
  logcat: {
    autoStart: true,
    maxUiLines: 20_000,
    ringMaxEntries: 50_000,
  },
  mcp: {
    autoStart: false,
    buildTimeoutSec: 600,
    logcatDefaultCount: 200,
    buildLogDefaultLines: 200,
  },
  telemetry: { enabled: false },
  onboardingCompleted: false,
  recentProjects: [],
  lastActiveProject: null,
};

const [settingsState, setSettingsState] = createStore<AppSettings>(
  structuredClone(DEFAULT_SETTINGS)
);

export { settingsState };

let saveTimer: ReturnType<typeof setTimeout> | undefined;

function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await saveSettingsIpc(settingsState);
    } catch (err) {
      const msg = formatError(err);
      console.error("Failed to save settings:", msg);
      showToast(`Settings could not be saved: ${msg}`, "error");
    }
  }, 500);
}

export async function loadSettings(): Promise<void> {
  try {
    const loaded = await getSettings();
    setSettingsState(loaded);
  } catch (err) {
    // First launch: settings file doesn't exist yet — expected, use defaults.
    // For other unexpected errors, log but don't Toast (app still works with defaults).
    const msg = formatError(err);
    if (!msg.includes("No such file") && !msg.includes("os error 2")) {
      console.error("[settings] Unexpected error loading settings:", msg);
      showToast(`Settings failed to load: ${msg}`, "error");
    }
  }
}

export function updateSetting<S extends keyof AppSettings, K extends keyof AppSettings[S]>(
  section: S,
  key: K,
  value: AppSettings[S][K]
): void {
  setSettingsState(section, key as never, value as never);
  scheduleSave();
}

/** Update a top-level `AppSettings` field (e.g. `onboardingCompleted`). */
export function setAppSetting<K extends keyof AppSettings>(key: K, value: AppSettings[K]): void {
  setSettingsState(key as never, value as never);
  scheduleSave();
}

export async function resetSettings(): Promise<void> {
  try {
    const defaults = await resetSettingsToDefaults();
    setSettingsState(defaults);
  } catch (err) {
    const msg = formatError(err);
    console.error("[settings] Failed to reset settings via backend:", msg);
    // Fall back to in-memory defaults — the user still gets a reset, but
    // the file on disk may be out of sync.
    setSettingsState(structuredClone(DEFAULT_SETTINGS));
    showToast(`Settings reset to defaults (backend error: ${msg})`, "error");
  }
}

export function getDefaults(): AppSettings {
  return DEFAULT_SETTINGS;
}

// Listen for settings corruption detected by the Rust backend on startup.
// Guard with typeof window to avoid running in test environments.
if (typeof window !== "undefined") {
  listen("settings:corrupted", () => {
    showToast(
      "Settings file was corrupted and has been reset to defaults. Your previous settings have been lost.",
      "error"
    );
  }).catch(() => {
    // Non-critical: if listen fails (e.g., in tests), ignore silently.
  });
}

// Apply CSS variable effects when appearance settings change
if (typeof document !== "undefined") {
  createEffect(() => {
    document.documentElement.style.setProperty("--font-size-ui", `${settingsState.appearance.uiFontSize}px`);
  });
}
