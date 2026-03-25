import { createStore } from "solid-js/store";

export type SidebarTab = "files" | "search" | "git";
export type BottomPanelTab = "build" | "logcat" | "terminal" | "ai";

interface UIState {
  sidebarVisible: boolean;
  sidebarWidth: number;
  bottomPanelVisible: boolean;
  bottomPanelHeight: number;
  activeSidebarTab: SidebarTab;
  activeBottomTab: BottomPanelTab;
}

const [uiState, setUIState] = createStore<UIState>({
  sidebarVisible: true,
  sidebarWidth: 240,
  bottomPanelVisible: false,
  bottomPanelHeight: 250,
  activeSidebarTab: "files",
  activeBottomTab: "build",
});

export { uiState, setUIState };

export function toggleSidebar() {
  setUIState("sidebarVisible", (v) => !v);
}

export function toggleBottomPanel() {
  setUIState("bottomPanelVisible", (v) => !v);
}

export function setSidebarWidth(width: number) {
  const clamped = Math.max(160, Math.min(600, width));
  setUIState("sidebarWidth", clamped);
}

export function setBottomPanelHeight(height: number) {
  const clamped = Math.max(100, Math.min(600, height));
  setUIState("bottomPanelHeight", clamped);
}

export function setActiveSidebarTab(tab: SidebarTab) {
  setUIState("activeSidebarTab", tab);
}

export function setActiveBottomTab(tab: BottomPanelTab) {
  setUIState("activeBottomTab", tab);
}
