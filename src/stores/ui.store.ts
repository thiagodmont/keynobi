import { createStore } from "solid-js/store";

export type MainTab = "build" | "logcat";

interface UIState {
  activeTab: MainTab;
  bottomPanelHeight: number;
  sidebarCollapsed: boolean;
  deviceSidebarCollapsed: boolean;
}

const [uiState, setUIState] = createStore<UIState>({
  activeTab: "build",
  bottomPanelHeight: 300,
  sidebarCollapsed: false,
  deviceSidebarCollapsed: false,
});

export { uiState, setUIState };

export function setActiveTab(tab: MainTab) {
  setUIState("activeTab", tab);
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
