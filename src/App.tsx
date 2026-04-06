import { type JSX, For, onMount, onCleanup } from "solid-js";
import {
  uiState,
  setActiveTab,
  toggleSidebar,
  toggleDeviceSidebar,
  type MainTab,
} from "@/stores/ui.store";
import TitleBar from "@/components/layout/TitleBar";
import StatusBar from "@/components/layout/StatusBar";
import { ToastContainer, showToast } from "@/components/common/Toast";
import { DialogHost } from "@/components/common/Dialog";
import { AppErrorBoundary } from "@/components/common/ErrorBoundary";
import { CommandPalette, openPalette } from "@/components/common/CommandPalette";
import { SettingsPanel, openSettings } from "@/components/settings/SettingsPanel";
import { HealthPanel, openHealthPanel } from "@/components/health/HealthPanel";
import { McpPanel } from "@/components/mcp/McpPanel";
import { BuildPanel } from "@/components/build/BuildPanel";
import { LogcatPanel } from "@/components/logcat/LogcatPanel";
import { ProjectInfoEditor, openProjectInfoEditor } from "@/components/projects/ProjectInfoEditor";
import { ProjectSidebar } from "@/components/projects/ProjectSidebar";
import { DeviceSidebar } from "@/components/device/DeviceSidebar";
import { DevicePickerDialog } from "@/components/device/DevicePickerDialog";
import { registerKeybinding, initKeybindings } from "@/lib/keybindings";
import { registerAction, type ActionCategory } from "@/lib/action-registry";
import { loadSettings } from "@/stores/settings.store";
import { openProjectFolder, refreshProjectsList, restoreLastProject } from "@/services/project.service";
import { initBuildService, runBuild, runAndDeploy, cancelBuild } from "@/services/build.service";
import { initDevices } from "@/stores/device.store";
import { openVariantPicker } from "@/components/build/VariantSelector";
import { formatError } from "@/lib/tauri-api";
import { projectState } from "@/stores/project.store";
import { openMcpPanel } from "@/components/mcp/McpPanel";

function formatShortcut(opts: { key: string; metaKey?: boolean; shiftKey?: boolean; altKey?: boolean }): string {
  const parts: string[] = [];
  if (opts.metaKey) parts.push("Cmd");
  if (opts.altKey) parts.push("Opt");
  if (opts.shiftKey) parts.push("Shift");
  parts.push(opts.key.length === 1 ? opts.key.toUpperCase() : opts.key);
  return parts.join("+");
}

function registerKeyAndAction(opts: {
  id: string;
  key: string;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  label: string;
  category: ActionCategory;
  action: () => void;
}) {
  registerKeybinding({
    key: opts.key,
    metaKey: opts.metaKey,
    shiftKey: opts.shiftKey,
    altKey: opts.altKey,
    description: opts.label,
    context: "global",
    action: opts.action,
  });
  registerAction({
    id: opts.id,
    label: opts.label,
    category: opts.category,
    shortcut: formatShortcut(opts),
    action: opts.action,
  });
}

export function App(): JSX.Element {
  let unlistenClose: (() => void) | undefined;

  onMount(async () => {
    initKeybindings();
    loadSettings();

    // Initialize MCP lifecycle event listeners.
    import("@/stores/mcp.store").then(({ initMcpListeners, loadMcpActivity }) => {
      initMcpListeners();
      // Prime the server-alive status so the StatusBar dot is correct immediately.
      loadMcpActivity();
    });

    // Load project registry into the sidebar store.
    refreshProjectsList().catch(console.error);

    // Restore last-active project, then initialize build/devices.
    const restored = await restoreLastProject().catch(() => false);
    if (!restored) {
      initBuildService().catch(console.error);
      initDevices().catch(console.error);
    }

    // ── Settings ─────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "general.settings", key: ",", metaKey: true, label: "Open Settings", category: "General", action: () => openSettings() });

    // ── View ─────────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "view.healthCenter", key: "h", metaKey: true, shiftKey: true, label: "Open Health Center", category: "View", action: openHealthPanel });
    registerKeyAndAction({ id: "view.buildPanel", key: "1", metaKey: true, label: "Open Build Panel", category: "View", action: () => setActiveTab("build") });
    registerKeyAndAction({ id: "view.logcatPanel", key: "2", metaKey: true, label: "Open Logcat Panel", category: "View", action: () => setActiveTab("logcat") });
    registerKeyAndAction({ id: "view.devicesPanel", key: "3", metaKey: true, label: "Toggle Device Sidebar", category: "View", action: () => toggleDeviceSidebar() });
    registerKeyAndAction({ id: "view.toggleSidebar", key: "b", metaKey: true, label: "Toggle Project Sidebar", category: "View", action: () => toggleSidebar() });

    // ── File ────────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "file.openFolder", key: "o", metaKey: true, label: "Open Folder", category: "File", action: () => { openProjectFolder(); } });

    // ── Command Palette ──────────────────────────────────────────────────────
    registerKeyAndAction({ id: "navigate.commandPalette", key: "p", metaKey: true, shiftKey: true, label: "Command Palette", category: "General", action: () => openPalette("commands") });

    // ── Build & Run ───────────────────────────────────────────────────────────
    registerKeyAndAction({
      id: "build.run", key: "r", metaKey: true, label: "Run App", category: "Build" as ActionCategory,
      action: async () => {
        try {
          await runAndDeploy();
        } catch (e) {
          showToast(typeof e === "string" ? e : (e as Error).message ?? "Run failed", "error");
        }
      },
    });
    registerKeyAndAction({
      id: "build.runOnly", key: "r", metaKey: true, shiftKey: true, label: "Build Only (no deploy)", category: "Build" as ActionCategory,
      action: async () => {
        try {
          await runBuild();
        } catch (e) {
          showToast(typeof e === "string" ? e : (e as Error).message ?? "Build failed", "error");
        }
      },
    });
    registerAction({
      id: "build.cancel",
      label: "Cancel Build",
      category: "Build" as ActionCategory,
      action: async () => {
        await cancelBuild().catch(console.error);
      },
    });
    registerAction({
      id: "build.clean",
      label: "Clean Project",
      category: "Build" as ActionCategory,
      action: async () => {
        try {
          await runBuild("clean");
        } catch (e) {
          showToast(typeof e === "string" ? e : (e as Error).message ?? "Clean failed", "error");
        }
      },
    });
    registerKeyAndAction({
      id: "build.selectVariant", key: "v", metaKey: true, shiftKey: true, label: "Select Build Variant", category: "Build" as ActionCategory,
      action: () => openVariantPicker(),
    });
    registerAction({
      id: "device.manage",
      label: "Manage Virtual Devices",
      category: "Build" as ActionCategory,
      action: () => toggleDeviceSidebar(),
    });

    // ── Project ───────────────────────────────────────────────────────────────
    registerAction({
      id: "project.info",
      label: "Project App Info",
      category: "File" as ActionCategory,
      action: () => {
        if (!projectState.projectRoot) {
          showToast("No project is open.", "error");
          return;
        }
        openProjectInfoEditor();
      },
    });

    registerKeyAndAction({
      id: "mcp.panel",
      key: "m",
      metaKey: true,
      shiftKey: true,
      label: "Open MCP Activity Panel",
      category: "General",
      action: openMcpPanel,
    });

    registerAction({
      id: "mcp.start",
      label: "Copy MCP Setup Command",
      category: "General",
      action: async () => {
        try {
          const { getMcpSetupStatus } = await import("@/lib/tauri-api");
          const s = await getMcpSetupStatus();
          const cmd = `claude mcp add --transport stdio android-companion -- "${s.exePath}" --mcp`;
          await navigator.clipboard.writeText(cmd);
          showToast("MCP setup command copied — paste it in your terminal to register with Claude Code", "success");
        } catch (e) {
          showToast(`Failed to copy MCP command: ${formatError(e)}`, "error");
        }
      },
    });

    // ── Window close guard ────────────────────────────────────────────────────
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const appWindow = getCurrentWindow();
      const unlisten = await appWindow.onCloseRequested(async (_event) => {
        // No dirty files to check — allow close immediately.
      });
      unlistenClose = unlisten;
    } catch {
      // Not running inside Tauri — skip.
    }
  });

  onCleanup(() => {
    unlistenClose?.();
  });

  const tabs: { id: MainTab; label: string }[] = [
    { id: "build", label: "Build" },
    { id: "logcat", label: "Logcat" },
  ];

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100vh",
        width: "100vw",
        overflow: "hidden",
        background: "var(--bg-primary)",
      }}
    >
      <TitleBar />

      <AppErrorBoundary>
        {/* Body: left sidebar + main column + right device sidebar */}
        <div style={{ flex: "1", display: "flex", "flex-direction": "row", overflow: "hidden" }}>

          {/* Left project sidebar */}
          <ProjectSidebar />

          {/* Main column: tab bar + panels */}
          <div style={{ flex: "1", display: "flex", "flex-direction": "column", overflow: "hidden" }}>
            {/* Tab bar */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                height: "36px",
                background: "var(--bg-tertiary)",
                "border-bottom": "1px solid var(--border)",
                "padding-left": "12px",
                "flex-shrink": "0",
                gap: "2px",
              }}
            >
              <For each={tabs}>
                {(tab) => {
                  const isActive = () => uiState.activeTab === tab.id;
                  return (
                    <button
                      onClick={() => setActiveTab(tab.id)}
                      style={{
                        padding: "0 16px",
                        height: "36px",
                        "font-size": "12px",
                        display: "flex",
                        "align-items": "center",
                        color: isActive() ? "var(--text-primary)" : "var(--text-muted)",
                        background: isActive() ? "var(--bg-secondary)" : "none",
                        "border-bottom": isActive() ? "2px solid var(--accent)" : "2px solid transparent",
                        cursor: "pointer",
                        border: "none",
                        "border-top": "none",
                        "border-left": "none",
                        "border-right": "none",
                        "font-weight": isActive() ? "500" : "normal",
                        transition: "color 0.1s",
                      }}
                    >
                      {tab.label}
                    </button>
                  );
                }}
              </For>
            </div>

            {/* Panel content area */}
            <div style={{ flex: "1", overflow: "hidden", display: "flex", "flex-direction": "column" }}>
              <div style={{ display: uiState.activeTab === "build" ? "flex" : "none", flex: "1", overflow: "hidden", "flex-direction": "column" }}>
                <BuildPanel />
              </div>
              <div style={{ display: uiState.activeTab === "logcat" ? "flex" : "none", flex: "1", overflow: "hidden", "flex-direction": "column" }}>
                <LogcatPanel />
              </div>
            </div>
          </div>

          {/* Right device sidebar */}
          <DeviceSidebar />
        </div>
      </AppErrorBoundary>

      <StatusBar />
      <ToastContainer />
      <CommandPalette />
      <SettingsPanel />
      <HealthPanel />
      <McpPanel />
      <ProjectInfoEditor />
      <DevicePickerDialog />
      <DialogHost />
    </div>
  );
}

export default App;
