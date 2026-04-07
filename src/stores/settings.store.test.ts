import { describe, it, expect, vi } from "vitest";
import * as tauriApi from "@/lib/tauri-api";
import {
  settingsState,
  updateSetting,
  loadSettings,
  getDefaults,
  setAppSetting,
} from "@/stores/settings.store";

describe("settings.store", () => {
  it("has correct editor defaults", () => {
    const d = getDefaults();
    expect(d.editor.fontSize).toBe(13);
    expect(d.editor.tabSize).toBe(4);
    expect(d.editor.insertSpaces).toBe(true);
    expect(d.editor.wordWrap).toBe(false);
    expect(d.editor.lineNumbers).toBe(true);
    expect(d.editor.bracketMatching).toBe(true);
    expect(d.editor.autoCloseBrackets).toBe(true);
    expect(d.editor.highlightActiveLine).toBe(true);
  });

  it("has correct appearance defaults", () => {
    expect(getDefaults().appearance.uiFontSize).toBe(12);
  });

  it("has correct search defaults", () => {
    const d = getDefaults();
    expect(d.search.contextLines).toBe(2);
    expect(d.search.maxResults).toBe(10_000);
    expect(d.search.maxFiles).toBe(500);
  });

  it("has correct advanced defaults", () => {
    const d = getDefaults();
    expect(d.advanced.treeSitterCacheSize).toBe(50);
    expect(d.advanced.lspMaxMessageSizeMb).toBe(64);
    expect(d.advanced.hoverDelayMs).toBe(500);
    expect(d.advanced.recentFilesLimit).toBe(20);
  });

  it("has correct LSP defaults", () => {
    const d = getDefaults();
    expect(d.lsp.logLevel).toBe("INFO");
    expect(d.lsp.requestTimeoutSec).toBe(30);
  });

  it("updateSetting changes editor settings", () => {
    updateSetting("editor", "fontSize", 18);
    expect(settingsState.editor.fontSize).toBe(18);
    updateSetting("editor", "fontSize", 13);
  });

  it("updateSetting changes appearance settings", () => {
    updateSetting("appearance", "uiFontSize", 14);
    expect(settingsState.appearance.uiFontSize).toBe(14);
    updateSetting("appearance", "uiFontSize", 12);
  });

  it("updateSetting changes search settings", () => {
    updateSetting("search", "contextLines", 5);
    expect(settingsState.search.contextLines).toBe(5);
    updateSetting("search", "contextLines", 2);
  });

  it("updateSetting changes advanced settings", () => {
    updateSetting("advanced", "hoverDelayMs", 300);
    expect(settingsState.advanced.hoverDelayMs).toBe(300);
    updateSetting("advanced", "hoverDelayMs", 500);
  });

  it("updateSetting changes LSP settings", () => {
    updateSetting("lsp", "logLevel", "DEBUG");
    expect(settingsState.lsp.logLevel).toBe("DEBUG");
    updateSetting("lsp", "logLevel", "INFO");
  });

  it("updateSetting changes android settings", () => {
    updateSetting("android", "sdkPath", "/custom/sdk");
    expect(settingsState.android.sdkPath).toBe("/custom/sdk");
    updateSetting("android", "sdkPath", null);
  });

  it("getDefaults returns the same constant reference", () => {
    const d1 = getDefaults();
    const d2 = getDefaults();
    expect(d1).toEqual(d2);
    expect(d1).toBe(d2);
  });

  it("defaults onboardingCompleted to false", () => {
    expect(getDefaults().onboardingCompleted).toBe(false);
  });

  it("setAppSetting updates onboardingCompleted", () => {
    setAppSetting("onboardingCompleted", true);
    expect(settingsState.onboardingCompleted).toBe(true);
    setAppSetting("onboardingCompleted", false);
    expect(settingsState.onboardingCompleted).toBe(false);
  });
});

describe("settings.store error state transitions", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("keeps defaults when getSettings rejects with unexpected error", async () => {
    vi.spyOn(tauriApi, "getSettings").mockRejectedValue(
      new Error("IPC channel unexpectedly closed")
    );
    await loadSettings();
    // State should still reflect defaults — loadSettings must not throw.
    expect(settingsState.editor.fontSize).toBe(13);
    expect(settingsState.editor.tabSize).toBe(4);
    expect(settingsState.appearance.uiFontSize).toBe(12);
    vi.restoreAllMocks();
  });

  it("keeps defaults when getSettings rejects with file-not-found error", async () => {
    vi.spyOn(tauriApi, "getSettings").mockRejectedValue(
      new Error("No such file or directory (os error 2)")
    );
    await loadSettings();
    expect(settingsState.editor.fontSize).toBe(13);
    expect(settingsState.search.contextLines).toBe(2);
    vi.restoreAllMocks();
  });

  it("applies loaded settings when getSettings resolves with partial overrides", async () => {
    const customSettings = {
      ...getDefaults(),
      editor: { ...getDefaults().editor, fontSize: 16 },
    };
    vi.spyOn(tauriApi, "getSettings").mockResolvedValue(customSettings);
    await loadSettings();
    expect(settingsState.editor.fontSize).toBe(16);
    // Restore to default for subsequent tests.
    updateSetting("editor", "fontSize", 13);
    vi.restoreAllMocks();
  });

  it("applies onboardingCompleted from getSettings", async () => {
    vi.spyOn(tauriApi, "getSettings").mockResolvedValue({
      ...getDefaults(),
      onboardingCompleted: true,
    });
    await loadSettings();
    expect(settingsState.onboardingCompleted).toBe(true);
    setAppSetting("onboardingCompleted", false);
    vi.restoreAllMocks();
  });
});
