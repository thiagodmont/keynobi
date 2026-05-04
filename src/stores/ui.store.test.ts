import { describe, it, expect, beforeEach } from "vitest";
import {
  enterLogMode,
  exitLogMode,
  resetUIStateForTests,
  setActiveTab,
  setDeviceSidebarCollapsed,
  setSidebarCollapsed,
  toggleLogMode,
  uiState,
} from "./ui.store";

describe("ui.store", () => {
  beforeEach(() => {
    resetUIStateForTests();
  });

  describe("setActiveTab", () => {
    it("starts on the logcat tab", () => {
      expect(uiState.activeTab).toBe("logcat");
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

  describe("bottomPanelHeight", () => {
    it("defaults to 300", () => {
      expect(uiState.bottomPanelHeight).toBe(300);
    });
  });

  describe("Log Mode", () => {
    it("defaults to inactive with no snapshot", () => {
      expect(uiState.logMode.active).toBe(false);
      expect(uiState.logMode.snapshot).toBe(null);
    });

    it("enters Log Mode by snapshotting current layout and switching to logcat", () => {
      setActiveTab("layout");
      setSidebarCollapsed(true);
      setDeviceSidebarCollapsed(false);

      enterLogMode();

      expect(uiState.activeTab).toBe("logcat");
      expect(uiState.logMode.active).toBe(true);
      expect(uiState.logMode.snapshot).toEqual({
        activeTab: "layout",
        sidebarCollapsed: true,
        deviceSidebarCollapsed: false,
      });
    });

    it("exits Log Mode by restoring the captured layout exactly", () => {
      setActiveTab("build");
      setSidebarCollapsed(false);
      setDeviceSidebarCollapsed(true);
      enterLogMode();

      exitLogMode();

      expect(uiState.logMode.active).toBe(false);
      expect(uiState.logMode.snapshot).toBe(null);
      expect(uiState.activeTab).toBe("build");
      expect(uiState.sidebarCollapsed).toBe(false);
      expect(uiState.deviceSidebarCollapsed).toBe(true);
    });

    it("does not overwrite the original snapshot when enter is called repeatedly", () => {
      setActiveTab("layout");
      setSidebarCollapsed(true);
      setDeviceSidebarCollapsed(false);
      enterLogMode();

      setSidebarCollapsed(false);
      setDeviceSidebarCollapsed(true);
      enterLogMode();

      expect(uiState.logMode.snapshot).toEqual({
        activeTab: "layout",
        sidebarCollapsed: true,
        deviceSidebarCollapsed: false,
      });
    });

    it("toggles Log Mode on and off", () => {
      setActiveTab("layout");

      toggleLogMode();
      expect(uiState.logMode.active).toBe(true);
      expect(uiState.activeTab).toBe("logcat");

      toggleLogMode();
      expect(uiState.logMode.active).toBe(false);
      expect(uiState.activeTab).toBe("layout");
    });

    it("exits Log Mode and opens the requested tab when navigating away", () => {
      setActiveTab("layout");
      setSidebarCollapsed(true);
      setDeviceSidebarCollapsed(true);
      enterLogMode();

      setActiveTab("build");

      expect(uiState.logMode.active).toBe(false);
      expect(uiState.logMode.snapshot).toBe(null);
      expect(uiState.activeTab).toBe("build");
      expect(uiState.sidebarCollapsed).toBe(true);
      expect(uiState.deviceSidebarCollapsed).toBe(true);
    });
  });
});
