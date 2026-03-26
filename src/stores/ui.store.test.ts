import { describe, it, expect, beforeEach } from "vitest";
import {
  uiState,
  setUIState,
  toggleSidebar,
  toggleBottomPanel,
  setSidebarWidth,
  setBottomPanelHeight,
  setActiveSidebarTab,
  setActiveBottomTab,
} from "./ui.store";

// Reset visible state to defaults before each test.
function resetUIState() {
  setUIState({
    sidebarVisible: true,
    sidebarWidth: 240,
    bottomPanelVisible: false,
    bottomPanelHeight: 250,
    activeSidebarTab: "files",
    activeBottomTab: "problems",
  });
}

// ── toggleSidebar ─────────────────────────────────────────────────────────────

describe("toggleSidebar", () => {
  beforeEach(resetUIState);

  it("hides the sidebar when it is visible", () => {
    expect(uiState.sidebarVisible).toBe(true);
    toggleSidebar();
    expect(uiState.sidebarVisible).toBe(false);
  });

  it("shows the sidebar when it is hidden", () => {
    setUIState("sidebarVisible", false);
    toggleSidebar();
    expect(uiState.sidebarVisible).toBe(true);
  });

  it("round-trips correctly on two calls", () => {
    toggleSidebar();
    toggleSidebar();
    expect(uiState.sidebarVisible).toBe(true);
  });
});

// ── toggleBottomPanel ─────────────────────────────────────────────────────────

describe("toggleBottomPanel", () => {
  beforeEach(resetUIState);

  it("shows the bottom panel when it is hidden", () => {
    expect(uiState.bottomPanelVisible).toBe(false);
    toggleBottomPanel();
    expect(uiState.bottomPanelVisible).toBe(true);
  });

  it("hides the bottom panel when it is visible", () => {
    setUIState("bottomPanelVisible", true);
    toggleBottomPanel();
    expect(uiState.bottomPanelVisible).toBe(false);
  });
});

// ── setSidebarWidth ───────────────────────────────────────────────────────────

describe("setSidebarWidth", () => {
  beforeEach(resetUIState);

  it("sets a valid width", () => {
    setSidebarWidth(300);
    expect(uiState.sidebarWidth).toBe(300);
  });

  it("clamps below the minimum (160)", () => {
    setSidebarWidth(50);
    expect(uiState.sidebarWidth).toBe(160);
  });

  it("clamps above the maximum (600)", () => {
    setSidebarWidth(9999);
    expect(uiState.sidebarWidth).toBe(600);
  });

  it("accepts the exact minimum", () => {
    setSidebarWidth(160);
    expect(uiState.sidebarWidth).toBe(160);
  });

  it("accepts the exact maximum", () => {
    setSidebarWidth(600);
    expect(uiState.sidebarWidth).toBe(600);
  });
});

// ── setBottomPanelHeight ──────────────────────────────────────────────────────

describe("setBottomPanelHeight", () => {
  beforeEach(resetUIState);

  it("sets a valid height", () => {
    setBottomPanelHeight(350);
    expect(uiState.bottomPanelHeight).toBe(350);
  });

  it("clamps below the minimum (100)", () => {
    setBottomPanelHeight(10);
    expect(uiState.bottomPanelHeight).toBe(100);
  });

  it("clamps above the maximum (600)", () => {
    setBottomPanelHeight(9999);
    expect(uiState.bottomPanelHeight).toBe(600);
  });
});

// ── setActiveSidebarTab ───────────────────────────────────────────────────────

describe("setActiveSidebarTab", () => {
  beforeEach(resetUIState);

  it("sets the active sidebar tab to search", () => {
    setActiveSidebarTab("search");
    expect(uiState.activeSidebarTab).toBe("search");
  });

  it("sets the active sidebar tab to git", () => {
    setActiveSidebarTab("git");
    expect(uiState.activeSidebarTab).toBe("git");
  });

  it("sets the active sidebar tab back to files", () => {
    setActiveSidebarTab("git");
    setActiveSidebarTab("files");
    expect(uiState.activeSidebarTab).toBe("files");
  });

  it("sets the active sidebar tab to symbols", () => {
    setActiveSidebarTab("symbols");
    expect(uiState.activeSidebarTab).toBe("symbols");
  });
});

// ── setActiveBottomTab ────────────────────────────────────────────────────────

describe("setActiveBottomTab", () => {
  beforeEach(resetUIState);

  it("sets the active bottom tab", () => {
    setActiveBottomTab("logcat");
    expect(uiState.activeBottomTab).toBe("logcat");
  });

  it("switches between tabs", () => {
    setActiveBottomTab("terminal");
    setActiveBottomTab("build");
    expect(uiState.activeBottomTab).toBe("build");
  });

  it("sets the active bottom tab to problems", () => {
    setActiveBottomTab("problems");
    expect(uiState.activeBottomTab).toBe("problems");
  });
});
