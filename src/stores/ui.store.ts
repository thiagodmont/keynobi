import { createStore } from "solid-js/store";
import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";

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
    showToast(
      `MCP server failed to start: ${event.payload}`,
      "error"
    );
  }).catch(() => {});
}

// ── Toast notifications ───────────────────────────────────────────────────────

export type ToastKind = "error" | "info" | "success" | "warning";

export interface Toast {
  id: string;
  message: string;
  kind: ToastKind;
}

const [_toasts, setToasts] = createSignal<Toast[]>([]);
const _toastTimers = new Map<string, ReturnType<typeof setTimeout>>();

export const toasts = _toasts;

export function showToast(message: string, kind: ToastKind = "info"): void {
  const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  setToasts(prev => [...prev, { id, message, kind }]);
  if (kind !== "error") {
    const timer = setTimeout(() => {
      _toastTimers.delete(id);
      dismissToast(id);
    }, 5000);
    _toastTimers.set(id, timer);
  }
}

export function dismissToast(id: string): void {
  const timer = _toastTimers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    _toastTimers.delete(id);
  }
  setToasts(prev => prev.filter(t => t.id !== id));
}
