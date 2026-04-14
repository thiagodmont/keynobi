import { describe, it, expect, beforeEach } from "vitest";
import {
  uiState,
  setUIState,
  setActiveTab,
} from "./ui.store";

// Reset state to defaults before each test.
function resetUIState() {
  setUIState({
    activeTab: "build",
    bottomPanelHeight: 300,
  });
}

// ── setActiveTab ──────────────────────────────────────────────────────────────

describe("setActiveTab", () => {
  beforeEach(resetUIState);

  it("starts on the build tab", () => {
    expect(uiState.activeTab).toBe("build");
  });

  it("switches to logcat", () => {
    setActiveTab("logcat");
    expect(uiState.activeTab).toBe("logcat");
  });

  it("switches to logcat tab then back to build", () => {
    setActiveTab("logcat");
    expect(uiState.activeTab).toBe("logcat");
    setActiveTab("build");
    expect(uiState.activeTab).toBe("build");
  });
});

// ── bottomPanelHeight ─────────────────────────────────────────────────────────

describe("bottomPanelHeight", () => {
  beforeEach(resetUIState);

  it("defaults to 300", () => {
    expect(uiState.bottomPanelHeight).toBe(300);
  });

  it("can be updated", () => {
    setUIState("bottomPanelHeight", 400);
    expect(uiState.bottomPanelHeight).toBe(400);
  });
});

