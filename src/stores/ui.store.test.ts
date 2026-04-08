import { describe, it, expect, beforeEach } from "vitest";
import {
  uiState,
  setUIState,
  setActiveTab,
  showToast,
  toasts,
  dismissToast,
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

// ── toast store ───────────────────────────────────────────────────────────────

describe("toast store", () => {
  beforeEach(() => {
    toasts().forEach(t => dismissToast(t.id));
  });

  it("adds an error toast", () => {
    showToast("Something went wrong", "error");
    expect(toasts()).toHaveLength(1);
    expect(toasts()[0].message).toBe("Something went wrong");
    expect(toasts()[0].kind).toBe("error");
  });

  it("adds an info toast", () => {
    showToast("Done", "info");
    expect(toasts()[0].kind).toBe("info");
  });

  it("dismisses by id", () => {
    showToast("Temp", "info");
    const id = toasts()[0].id;
    dismissToast(id);
    expect(toasts()).toHaveLength(0);
  });
});
