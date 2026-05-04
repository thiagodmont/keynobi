import { createStore } from "solid-js/store";
import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { showToast } from "@/components/ui";

export type MainTab = "build" | "logcat" | "layout";

export interface LogModeSnapshot {
  activeTab: MainTab;
  sidebarCollapsed: boolean;
  deviceSidebarCollapsed: boolean;
}

interface LogModeState {
  active: boolean;
  snapshot: LogModeSnapshot | null;
}

interface UIState {
  activeTab: MainTab;
  bottomPanelHeight: number;
  sidebarCollapsed: boolean;
  deviceSidebarCollapsed: boolean;
  logMode: LogModeState;
}

function defaultLogModeState(): LogModeState {
  return {
    active: false,
    snapshot: null,
  };
}

function defaultUIState(): UIState {
  return {
    activeTab: "logcat",
    bottomPanelHeight: 300,
    sidebarCollapsed: false,
    deviceSidebarCollapsed: false,
    logMode: defaultLogModeState(),
  };
}

const [uiState, setUIState] = createStore<UIState>(defaultUIState());

export { uiState, setUIState };

export function resetUIStateForTests(): void {
  setUIState(defaultUIState());
}

function restoreLogModeSnapshot(nextActiveTab?: MainTab): void {
  const snapshot = uiState.logMode.snapshot;

  if (snapshot) {
    setUIState({
      activeTab: nextActiveTab ?? snapshot.activeTab,
      bottomPanelHeight: uiState.bottomPanelHeight,
      sidebarCollapsed: snapshot.sidebarCollapsed,
      deviceSidebarCollapsed: snapshot.deviceSidebarCollapsed,
      logMode: defaultLogModeState(),
    });
    return;
  }

  setUIState({
    activeTab: nextActiveTab ?? "logcat",
    bottomPanelHeight: uiState.bottomPanelHeight,
    sidebarCollapsed: false,
    deviceSidebarCollapsed: false,
    logMode: defaultLogModeState(),
  });
}

export function setActiveTab(tab: MainTab): void {
  if (uiState.logMode.active && tab !== "logcat") {
    restoreLogModeSnapshot(tab);
    return;
  }

  setUIState("activeTab", tab);
}

export function enterLogMode(): void {
  if (uiState.logMode.active) return;

  const snapshot: LogModeSnapshot = {
    activeTab: uiState.activeTab,
    sidebarCollapsed: uiState.sidebarCollapsed,
    deviceSidebarCollapsed: uiState.deviceSidebarCollapsed,
  };

  setUIState({
    activeTab: "logcat",
    bottomPanelHeight: uiState.bottomPanelHeight,
    sidebarCollapsed: uiState.sidebarCollapsed,
    deviceSidebarCollapsed: uiState.deviceSidebarCollapsed,
    logMode: {
      active: true,
      snapshot,
    },
  });
}

export function exitLogMode(): void {
  if (!uiState.logMode.active) return;
  restoreLogModeSnapshot();
}

export function toggleLogMode(): void {
  if (uiState.logMode.active) {
    exitLogMode();
    return;
  }

  enterLogMode();
}

export function setSidebarCollapsed(v: boolean): void {
  setUIState("sidebarCollapsed", v);
}

export function toggleSidebar(): void {
  setUIState("sidebarCollapsed", !uiState.sidebarCollapsed);
}

export function setDeviceSidebarCollapsed(v: boolean): void {
  setUIState("deviceSidebarCollapsed", v);
}

export function toggleDeviceSidebar(): void {
  setUIState("deviceSidebarCollapsed", !uiState.deviceSidebarCollapsed);
}

// Track MCP startup errors for use in MCP-related UI components.
const [mcpStartupError, setMcpStartupError] = createSignal<string | null>(null);
export { mcpStartupError };

if (typeof window !== "undefined") {
  listen<string>("mcp:startup-failed", (event) => {
    setMcpStartupError(event.payload);
    showToast(`MCP server failed to start: ${event.payload}`, "error");
  }).catch(() => {});
}
