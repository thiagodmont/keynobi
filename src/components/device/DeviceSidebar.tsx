/**
 * DeviceSidebar.tsx
 *
 * Persistent right sidebar showing connected ADB devices and installed AVDs.
 * - Expanded (220px): full device rows with status, name, actions
 * - Collapsed (40px): icon/status-dot rail only
 *
 * Selecting a device calls pickDevice() which sets it as the active device
 * for builds, logcat, and all other companion features.
 */

import { type JSX, Show, For, createSignal, onMount } from "solid-js";
import {
  deviceState,
  setDevices,
  setAvds,
  pickDevice,
  setLaunchingAvd,
  runningAvdNames,
  serialForAvd,
} from "@/stores/device.store";
import {
  refreshDevices,
  listAvdDevices,
  launchAvd,
  stopAvd,
  deleteAvdDevice,
  wipeAvdData,
  formatError,
} from "@/lib/tauri-api";
import type { Device, AvdInfo } from "@/bindings";
import { uiState, toggleDeviceSidebar } from "@/stores/ui.store";
import { showToast } from "@/components/ui";
import { Icon } from "@/components/ui";
import { showDialog } from "@/components/ui";
import { CreateDeviceDialog } from "./CreateDeviceDialog";
import { DownloadSystemImageDialog } from "./DownloadSystemImageDialog";

// ── Connection state color ─────────────────────────────────────────────────────

function connectionColor(state: Device["connectionState"]): string {
  switch (state) {
    case "online":       return "#4ade80";
    case "unauthorized": return "#fbbf24";
    case "offline":
    default:             return "#6b7280";
  }
}

// ── Main component ─────────────────────────────────────────────────────────────

export function DeviceSidebar(): JSX.Element {
  const collapsed = () => uiState.deviceSidebarCollapsed;

  const [refreshing, setRefreshing] = createSignal(false);
  const [launchingName, setLaunchingName] = createSignal<string | null>(null);
  const [showCreateDialog, setShowCreateDialog] = createSignal(false);
  const [showDownloadDialog, setShowDownloadDialog] = createSignal(false);

  onMount(() => {
    handleRefresh();
  });

  async function handleRefresh() {
    setRefreshing(true);
    try {
      const [devices, avds] = await Promise.all([
        refreshDevices(),
        listAvdDevices(),
      ]);
      setDevices(devices);
      setAvds(avds);
    } catch (e) {
      console.error("Device refresh failed:", e);
      showToast(`Device refresh failed: ${formatError(e)}`, "error");
    } finally {
      setRefreshing(false);
    }
  }

  async function handleLaunchAvd(avd: AvdInfo) {
    setLaunchingName(avd.name);
    setLaunchingAvd(avd.name);
    try {
      await launchAvd(avd.name);
      await handleRefresh();
    } catch (e) {
      console.error("Failed to launch emulator:", e);
      showToast(`Failed to launch emulator: ${formatError(e)}`, "error");
    } finally {
      setLaunchingName(null);
      setLaunchingAvd(null);
    }
  }

  async function handleStopDevice(serial: string) {
    try {
      await stopAvd(serial);
      await handleRefresh();
    } catch (e) {
      console.error("Failed to stop emulator:", e);
      showToast(`Failed to stop emulator: ${formatError(e)}`, "error");
    }
  }

  async function handleDeleteAvd(avd: AvdInfo) {
    const result = await showDialog({
      title: "Delete Virtual Device",
      message: `Are you sure you want to permanently delete "${avd.displayName}"? This cannot be undone.`,
      buttons: [
        { label: "Delete", value: "delete", style: "danger" },
        { label: "Cancel", value: "cancel", style: "secondary" },
      ],
    }).catch(() => "cancel");
    if (result !== "delete") return;
    try {
      const avds = await deleteAvdDevice(avd.name);
      setAvds(avds);
    } catch (e) {
      console.error("Failed to delete AVD:", e);
      showToast(`Failed to delete AVD: ${formatError(e)}`, "error");
    }
  }

  async function handleWipeAvd(avd: AvdInfo) {
    const result = await showDialog({
      title: "Wipe Device Data",
      message: `This will erase all user data for "${avd.displayName}" and restart it. Continue?`,
      buttons: [
        { label: "Wipe Data", value: "wipe", style: "danger" },
        { label: "Cancel", value: "cancel", style: "secondary" },
      ],
    }).catch(() => "cancel");
    if (result !== "wipe") return;
    try {
      await wipeAvdData(avd.name);
      await handleRefresh();
    } catch (e) {
      console.error("Failed to wipe AVD data:", e);
      showToast(`Failed to wipe AVD data: ${formatError(e)}`, "error");
    }
  }

  return (
    <>
      <div
        style={{
          width: collapsed() ? "40px" : "220px",
          "min-width": collapsed() ? "40px" : "220px",
          "max-width": collapsed() ? "40px" : "220px",
          transition: "width 0.18s ease, min-width 0.18s ease, max-width 0.18s ease",
          height: "100%",
          display: "flex",
          "flex-direction": "column",
          background: "var(--bg-secondary)",
          "border-left": "1px solid var(--border)",
          overflow: "hidden",
          "flex-shrink": "0",
        }}
      >
        {/* ── Header ── */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": collapsed() ? "center" : "space-between",
            padding: collapsed() ? "10px 0" : "10px 10px 8px 12px",
            "flex-shrink": "0",
          }}
        >
          <Show when={!collapsed()}>
            <span
              style={{
                "font-size": "10px",
                "font-weight": "600",
                color: "var(--text-muted)",
                "text-transform": "uppercase",
                "letter-spacing": "0.07em",
              }}
            >
              Devices
            </span>
          </Show>

          <div style={{ display: "flex", "align-items": "center", gap: "2px" }}>
            {/* Refresh button — only in expanded header */}
            <Show when={!collapsed()}>
              <button
                onClick={handleRefresh}
                disabled={refreshing()}
                title="Refresh devices"
                style={{
                  background: "none",
                  border: "none",
                  padding: "3px",
                  cursor: refreshing() ? "default" : "pointer",
                  color: "var(--text-muted)",
                  display: "flex",
                  "align-items": "center",
                  "border-radius": "4px",
                  opacity: refreshing() ? "0.4" : "1",
                }}
                onMouseEnter={(e) => { if (!refreshing()) (e.currentTarget as HTMLElement).style.color = "var(--text-primary)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-muted)"; }}
              >
                <Show
                  when={refreshing()}
                  fallback={<Icon name="refresh" size={13} />}
                >
                  <span class="lsp-spinner"><Icon name="spinner" size={13} /></span>
                </Show>
              </button>

              {/* Download system image button */}
              <button
                onClick={() => setShowDownloadDialog(true)}
                title="Download system image"
                style={{
                  background: "none",
                  border: "none",
                  padding: "3px",
                  cursor: "pointer",
                  color: "var(--text-muted)",
                  display: "flex",
                  "align-items": "center",
                  "border-radius": "4px",
                }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-primary)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-muted)"; }}
              >
                <Icon name="download" size={13} />
              </button>
            </Show>

            {/* Collapse toggle */}
            <button
              onClick={() => toggleDeviceSidebar()}
              title={collapsed() ? "Expand device sidebar" : "Collapse device sidebar"}
              style={{
                background: "none",
                border: "none",
                padding: "3px",
                cursor: "pointer",
                color: "var(--text-muted)",
                display: "flex",
                "align-items": "center",
                "border-radius": "4px",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-primary)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-muted)"; }}
            >
              {/* Mirror ProjectSidebar: same chevron-right + rotate trick, but flipped (sidebar is on right) */}
              <span
                style={{
                  "font-size": "12px",
                  "font-weight": "400",
                  transform: collapsed() ? "rotate(180deg)" : "none",
                  display: "inline-block",
                  transition: "transform 0.18s ease",
                  "line-height": "1",
                }}
              >
                ›
              </span>
            </button>
          </div>
        </div>

        {/* ── Scrollable content ── */}
        <div
          style={{
            flex: "1",
            "overflow-y": "auto",
            "overflow-x": "hidden",
            padding: "0 8px",
          }}
        >
          {/* Connected devices section */}
          <Show when={!collapsed()}>
            <Show when={deviceState.devices.length > 0}>
              <div
                style={{
                  "font-size": "10px",
                  "font-weight": "600",
                  color: "var(--text-muted)",
                  "text-transform": "uppercase",
                  "letter-spacing": "0.06em",
                  padding: "4px 2px 2px",
                  "margin-bottom": "2px",
                }}
              >
                Connected
              </div>
            </Show>
          </Show>

          <For each={deviceState.devices}>
            {(device) => (
              <ConnectedDeviceRow
                device={device}
                selected={deviceState.selectedSerial === device.serial}
                collapsed={collapsed()}
                onSelect={() => pickDevice(device.serial)}
                onStop={() => handleStopDevice(device.serial)}
              />
            )}
          </For>

          {/* Empty connected state — expanded only */}
          <Show when={!collapsed() && deviceState.devices.length === 0 && deviceState.avds.length === 0}>
            <div
              style={{
                "font-size": "11px",
                color: "var(--text-muted)",
                "text-align": "center",
                padding: "20px 8px",
                "line-height": "1.5",
              }}
            >
              No devices connected.
              <br />
              Create one below.
            </div>
          </Show>

          {/* Virtual devices section */}
          <Show when={!collapsed() && deviceState.avds.length > 0}>
            <div
              style={{
                "font-size": "10px",
                "font-weight": "600",
                color: "var(--text-muted)",
                "text-transform": "uppercase",
                "letter-spacing": "0.06em",
                padding: "8px 2px 2px",
                "margin-bottom": "2px",
                "border-top": deviceState.devices.length > 0 ? "1px solid var(--border)" : "none",
                "margin-top": deviceState.devices.length > 0 ? "4px" : "0",
              }}
            >
              Virtual
            </div>
          </Show>

          <Show when={collapsed() && deviceState.devices.length > 0 && deviceState.avds.length > 0}>
            {/* Visual separator between sections in collapsed mode */}
            <div style={{ height: "1px", background: "var(--border)", margin: "4px 2px" }} />
          </Show>

          <For each={deviceState.avds}>
            {(avd) => (
              <AvdRow
                avd={avd}
                isRunning={runningAvdNames().has(avd.name)}
                launching={launchingName() === avd.name}
                collapsed={collapsed()}
                onLaunch={() => handleLaunchAvd(avd)}
                onStop={() => {
                  const serial = serialForAvd(avd.name);
                  if (serial) handleStopDevice(serial);
                }}
                onDelete={() => handleDeleteAvd(avd)}
                onWipe={() => handleWipeAvd(avd)}
              />
            )}
          </For>
        </div>

        {/* ── Bottom: + New Device ── */}
        <div
          style={{
            "border-top": "1px solid var(--border)",
            "flex-shrink": "0",
            padding: "8px",
          }}
        >
          <button
            onClick={() => setShowCreateDialog(true)}
            title={collapsed() ? "New Virtual Device" : undefined}
            style={{
              width: "100%",
              display: "flex",
              "align-items": "center",
              gap: collapsed() ? "0" : "6px",
              "justify-content": "center",
              background: "none",
              border: "none",
              "border-radius": "6px",
              padding: "7px 6px",
              cursor: "pointer",
              color: "var(--text-secondary)",
              "font-size": "12px",
              transition: "background 0.1s",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.06))";
              (e.currentTarget as HTMLElement).style.color = "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.background = "none";
              (e.currentTarget as HTMLElement).style.color = "var(--text-secondary)";
            }}
          >
            <span style={{ "font-size": "16px", "font-weight": "300", "line-height": "1" }}>⊕</span>
            <Show when={!collapsed()}>
              <span>New Device…</span>
            </Show>
          </button>
        </div>
      </div>

      {/* Create Device Dialog — rendered outside sidebar so it's not clipped */}
      <Show when={showCreateDialog()}>
        <CreateDeviceDialog
          onClose={() => setShowCreateDialog(false)}
          onCreated={(avds) => {
            setAvds(avds);
            setShowCreateDialog(false);
          }}
        />
      </Show>

      {/* Download System Image Dialog */}
      <Show when={showDownloadDialog()}>
        <DownloadSystemImageDialog
          onClose={() => setShowDownloadDialog(false)}
          onDownloaded={() => {
            // Refresh installed images cache so CreateDeviceDialog picks up new images.
            import("@/stores/device.store").then(({ setSystemImages }) => setSystemImages([]));
          }}
        />
      </Show>
    </>
  );
}

// ── Connected device row ──────────────────────────────────────────────────────

function ConnectedDeviceRow(props: {
  device: Device;
  selected: boolean;
  collapsed: boolean;
  onSelect: () => void;
  onStop: () => void;
}): JSX.Element {
  const [hover, setHover] = createSignal(false);
  const isOnline = () => props.device.connectionState === "online";
  const isEmulator = () => props.device.deviceKind === "emulator";
  const dotColor = () => connectionColor(props.device.connectionState);

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={() => { if (isOnline()) props.onSelect(); }}
      title={props.collapsed ? (props.device.model ?? props.device.name) : undefined}
      style={{
        display: "flex",
        "align-items": props.collapsed ? "center" : "flex-start",
        gap: props.collapsed ? "0" : "8px",
        padding: props.collapsed ? "6px 0" : "7px 8px",
        cursor: isOnline() ? "pointer" : "default",
        "border-radius": "6px",
        "margin-bottom": "2px",
        background: props.selected
          ? "var(--accent-muted, rgba(92,124,250,0.18))"
          : hover()
          ? "var(--bg-hover, rgba(255,255,255,0.05))"
          : "transparent",
        transition: "background 0.1s",
        "justify-content": props.collapsed ? "center" : "flex-start",
        opacity: isOnline() ? "1" : "0.5",
        position: "relative",
      }}
    >
      {/* Status dot — acts as the icon in collapsed mode */}
      <span
        style={{
          width: props.collapsed ? "8px" : "7px",
          height: props.collapsed ? "8px" : "7px",
          "border-radius": "50%",
          background: dotColor(),
          "flex-shrink": "0",
          display: "inline-block",
          outline: props.selected && !props.collapsed ? `2px solid ${dotColor()}` : "none",
          "outline-offset": "2px",
          "margin-top": props.collapsed ? "0" : "4px",
        }}
      />

      <Show when={!props.collapsed}>
        <div style={{ flex: "1", "min-width": "0", overflow: "hidden" }}>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "4px",
            }}
          >
            <span
              style={{
                "font-size": "12px",
                "font-weight": props.selected ? "600" : "500",
                color: props.selected ? "var(--accent, #5c7cfa)" : "var(--text-primary)",
                overflow: "hidden",
                "text-overflow": "ellipsis",
                "white-space": "nowrap",
                "line-height": "1.3",
                flex: "1",
                "min-width": "0",
              }}
            >
              {props.device.model ?? props.device.name}
            </span>
            <Show when={props.selected}>
              <span style={{ color: "var(--accent)", "font-size": "11px", "flex-shrink": "0" }}>✓</span>
            </Show>
          </div>
          <div
            style={{
              "font-size": "10px",
              color: "var(--text-muted)",
              overflow: "hidden",
              "text-overflow": "ellipsis",
              "white-space": "nowrap",
              "margin-top": "1px",
            }}
          >
            {isEmulator() ? "Emulator" : "Physical"}
            {props.device.apiLevel ? ` · API ${props.device.apiLevel}` : ""}
          </div>
        </div>

        {/* Stop button for running emulators — show on hover */}
        <Show when={isEmulator() && isOnline() && hover()}>
          <button
            onClick={(e) => { e.stopPropagation(); props.onStop(); }}
            title="Stop emulator"
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              color: "var(--text-muted)",
              "font-size": "12px",
              padding: "2px 4px",
              "border-radius": "3px",
              "flex-shrink": "0",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.color = "var(--error, #f87171)";
              (e.currentTarget as HTMLElement).style.background = "rgba(248,113,113,0.1)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
              (e.currentTarget as HTMLElement).style.background = "none";
            }}
          >
            ✕
          </button>
        </Show>
      </Show>
    </div>
  );
}

// ── AVD row ───────────────────────────────────────────────────────────────────

function AvdRow(props: {
  avd: AvdInfo;
  isRunning: boolean;
  launching: boolean;
  collapsed: boolean;
  onLaunch: () => void;
  onStop: () => void;
  onDelete: () => void;
  onWipe: () => void;
}): JSX.Element {
  const [hover, setHover] = createSignal(false);
  const [menuOpen, setMenuOpen] = createSignal(false);

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      title={props.collapsed ? props.avd.displayName : undefined}
      style={{
        display: "flex",
        "align-items": props.collapsed ? "center" : "flex-start",
        gap: props.collapsed ? "0" : "8px",
        padding: props.collapsed ? "6px 0" : "7px 8px",
        "border-radius": "6px",
        "margin-bottom": "2px",
        background: hover() ? "var(--bg-hover, rgba(255,255,255,0.05))" : "transparent",
        transition: "background 0.1s",
        "justify-content": props.collapsed ? "center" : "flex-start",
        position: "relative",
      }}
    >
      {/* Phone icon with running indicator */}
      <div style={{ position: "relative", "flex-shrink": "0" }}>
        <div
          style={{
            width: "24px",
            height: "24px",
            "border-radius": "5px",
            background: "var(--bg-primary, rgba(255,255,255,0.06))",
            border: "1px solid var(--border)",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
          }}
        >
          <Icon name="device" size={13} color="var(--text-muted)" />
        </div>
        <Show when={props.isRunning}>
          <span
            style={{
              position: "absolute",
              bottom: "-2px",
              right: "-2px",
              width: "7px",
              height: "7px",
              "border-radius": "50%",
              background: "#4ade80",
              border: "1.5px solid var(--bg-secondary)",
            }}
          />
        </Show>
      </div>

      <Show when={!props.collapsed}>
        <div style={{ flex: "1", "min-width": "0", overflow: "hidden" }}>
          <div
            style={{
              "font-size": "12px",
              "font-weight": "500",
              color: "var(--text-primary)",
              overflow: "hidden",
              "text-overflow": "ellipsis",
              "white-space": "nowrap",
              "line-height": "1.3",
            }}
          >
            {props.avd.displayName}
          </div>
          <div
            style={{
              "font-size": "10px",
              color: "var(--text-muted)",
              overflow: "hidden",
              "text-overflow": "ellipsis",
              "white-space": "nowrap",
              "margin-top": "1px",
            }}
          >
            {props.avd.abi ?? "AVD"}
            {props.avd.apiLevel ? ` · API ${props.avd.apiLevel}` : ""}
            {props.isRunning ? " · Running" : ""}
          </div>
        </div>

        {/* Hover actions */}
        <Show when={hover()}>
          <div style={{ display: "flex", "align-items": "center", gap: "2px", "flex-shrink": "0" }}>
            <Show
              when={!props.isRunning}
              fallback={
                <button
                  onClick={(e) => { e.stopPropagation(); props.onStop(); }}
                  title="Stop emulator"
                  style={{
                    background: "none",
                    border: "none",
                    cursor: "pointer",
                    color: "var(--text-muted)",
                    "font-size": "12px",
                    padding: "2px 4px",
                    "border-radius": "3px",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLElement).style.color = "var(--error, #f87171)";
                    (e.currentTarget as HTMLElement).style.background = "rgba(248,113,113,0.1)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
                    (e.currentTarget as HTMLElement).style.background = "none";
                  }}
                >
                  ✕
                </button>
              }
            >
              <button
                onClick={(e) => { e.stopPropagation(); props.onLaunch(); }}
                disabled={props.launching}
                title={`Launch ${props.avd.displayName}`}
                style={{
                  background: "none",
                  border: "none",
                  padding: "2px",
                  cursor: props.launching ? "default" : "pointer",
                  color: "var(--text-muted)",
                  display: "flex",
                  "align-items": "center",
                  "border-radius": "3px",
                  opacity: props.launching ? "0.5" : "1",
                }}
                onMouseEnter={(e) => {
                  if (!props.launching) {
                    (e.currentTarget as HTMLElement).style.color = "#4ade80";
                    (e.currentTarget as HTMLElement).style.background = "rgba(74,222,128,0.1)";
                  }
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
                  (e.currentTarget as HTMLElement).style.background = "none";
                }}
              >
                <Icon name="play" size={11} />
              </button>
            </Show>

            {/* Overflow menu button */}
            <div style={{ position: "relative" }}>
              <button
                onClick={(e) => { e.stopPropagation(); setMenuOpen((v) => !v); }}
                title="More options"
                style={{
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  color: "var(--text-muted)",
                  "font-size": "13px",
                  padding: "2px 3px",
                  "border-radius": "3px",
                  "line-height": "1",
                  display: "flex",
                  "align-items": "center",
                }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-tertiary)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "none"; }}
              >
                ···
              </button>
              <Show when={menuOpen()}>
                <AvdContextMenu
                  onClose={() => setMenuOpen(false)}
                  onWipe={() => { setMenuOpen(false); props.onWipe(); }}
                  onDelete={() => { setMenuOpen(false); props.onDelete(); }}
                />
              </Show>
            </div>
          </div>
        </Show>
      </Show>
    </div>
  );
}

// ── AVD context menu ──────────────────────────────────────────────────────────

function AvdContextMenu(props: {
  onClose: () => void;
  onWipe: () => void;
  onDelete: () => void;
}): JSX.Element {
  return (
    <>
      <div
        style={{ position: "fixed", inset: "0", "z-index": "1999" }}
        onClick={() => props.onClose()}
      />
      <div
        style={{
          position: "absolute",
          right: "0",
          bottom: "calc(100% + 4px)",
          "z-index": "2000",
          background: "var(--bg-tertiary)",
          border: "1px solid var(--border)",
          "border-radius": "6px",
          "box-shadow": "0 4px 16px rgba(0,0,0,0.4)",
          "min-width": "150px",
          padding: "4px",
          "white-space": "nowrap",
        }}
      >
        <ContextMenuItem label="Wipe Data…" onClick={props.onWipe} destructive={false} />
        <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
        <ContextMenuItem label="Delete…" onClick={props.onDelete} destructive={true} />
      </div>
    </>
  );
}

function ContextMenuItem(props: {
  label: string;
  onClick: () => void;
  destructive: boolean;
}): JSX.Element {
  return (
    <button
      onClick={() => props.onClick()}
      style={{
        display: "block",
        width: "100%",
        padding: "6px 10px",
        background: "none",
        border: "none",
        cursor: "pointer",
        "text-align": "left",
        "font-size": "12px",
        "border-radius": "4px",
        color: props.destructive ? "var(--error, #f87171)" : "var(--text-secondary)",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = props.destructive
          ? "rgba(248,113,113,0.12)"
          : "var(--bg-hover, rgba(255,255,255,0.08))";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = "none";
      }}
    >
      {props.label}
    </button>
  );
}

export default DeviceSidebar;
