import { createStore } from "solid-js/store";
import { createEffect } from "solid-js";
import {
  getSettings,
  saveSettings as saveSettingsIpc,
  resetSettingsToDefaults,
  type AppSettings,
  formatError,
} from "@/lib/tauri-api";

export type { AppSettings };

const DEFAULT_SETTINGS: AppSettings = {
  editor: {
    fontFamily: '"SF Mono", "Fira Code", "JetBrains Mono", "Menlo", monospace',
    fontSize: 13,
    tabSize: 4,
    insertSpaces: true,
    wordWrap: false,
    lineNumbers: true,
    bracketMatching: true,
    highlightActiveLine: true,
    autoCloseBrackets: true,
  },
  appearance: { uiFontSize: 12 },
  search: { contextLines: 2, maxResults: 10_000, maxFiles: 500 },
  files: {
    excludedDirs: ["build", ".gradle", ".idea", ".git", "node_modules"],
    excludedExtensions: ["class", "dex", "apk", "aar"],
    maxFileSizeMb: 10,
  },
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
  },
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
      console.error("Failed to save settings:", formatError(err));
    }
  }, 500);
}

export async function loadSettings(): Promise<void> {
  try {
    const loaded = await getSettings();
    setSettingsState(loaded);
  } catch {
    // First launch or Tauri not available — use defaults
  }
}

export function updateSetting<
  S extends keyof AppSettings,
  K extends keyof AppSettings[S],
>(section: S, key: K, value: AppSettings[S][K]): void {
  setSettingsState(section, key as never, value as never);
  scheduleSave();
}

export async function resetSettings(): Promise<void> {
  try {
    const defaults = await resetSettingsToDefaults();
    setSettingsState(defaults);
  } catch {
    setSettingsState(structuredClone(DEFAULT_SETTINGS));
  }
}

export function getDefaults(): AppSettings {
  return DEFAULT_SETTINGS;
}

// Apply CSS variable effects when appearance/editor settings change
if (typeof document !== "undefined") {
  createEffect(() => {
    const root = document.documentElement;
    root.style.setProperty(
      "--font-size-editor",
      `${settingsState.editor.fontSize}px`
    );
    root.style.setProperty(
      "--font-size-ui",
      `${settingsState.appearance.uiFontSize}px`
    );
    root.style.setProperty("--font-mono", settingsState.editor.fontFamily);
  });
}
