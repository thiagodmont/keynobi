import { createStore } from "solid-js/store";
import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { showToast } from "@/components/ui";

export type MainTab = "build" | "logcat" | "layout";

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

// Track MCP startup errors for use in MCP-related UI components.
const [mcpStartupError, setMcpStartupError] = createSignal<string | null>(null);
export { mcpStartupError };

if (typeof window !== "undefined") {
  listen<string>("mcp:startup-failed", (event) => {
    setMcpStartupError(event.payload);
    showToast(`MCP server failed to start: ${event.payload}`, "error");
  }).catch(() => {});
}
