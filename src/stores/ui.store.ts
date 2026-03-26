import { createStore } from "solid-js/store";

export type MainTab = "build" | "logcat" | "devices";

interface UIState {
  activeTab: MainTab;
  bottomPanelHeight: number;
}

const [uiState, setUIState] = createStore<UIState>({
  activeTab: "build",
  bottomPanelHeight: 300,
});

export { uiState, setUIState };

export function setActiveTab(tab: MainTab) {
  setUIState("activeTab", tab);
}
