import {
  type JSX,
  For,
  Show,
  createSignal,
  onMount,
} from "solid-js";
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
  launchAvd,
  stopAvd,
  listAvdDevices,
  deleteAvdDevice,
  wipeAvdData,
} from "@/lib/tauri-api";
import type { Device, AvdInfo } from "@/bindings";
import { toggleDeviceSidebar } from "@/stores/ui.store";
import Icon from "@/components/common/Icon";
import { showDialog } from "@/components/common/Dialog";
import { CreateDeviceDialog } from "./CreateDeviceDialog";

export interface DevicePanelProps {
  onClose?: () => void;
  /** "panel" = full-width tab content; "popover" = compact status-bar dropdown */
  mode?: "panel" | "popover";
}

export function DevicePanel(props: DevicePanelProps): JSX.Element {
  const mode = () => props.mode ?? "panel";
  const isPopover = () => mode() === "popover";

  const [refreshing, setRefreshing] = createSignal(false);
  const [launchingName, setLaunchingName] = createSignal<string | null>(null);
  const [showCreateDialog, setShowCreateDialog] = createSignal(false);

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
    }
  }

  async function handleDeleteAvd(avd: AvdInfo) {
    try {
      const result = await showDialog({
        title: "Delete Virtual Device",
        message: `Are you sure you want to permanently delete "${avd.displayName}"? This cannot be undone.`,
        buttons: [
          { label: "Delete", value: "delete", style: "danger" },
          { label: "Cancel", value: "cancel", style: "secondary" },
        ],
      });
      if (result !== "delete") return;
      const avds = await deleteAvdDevice(avd.name);
      setAvds(avds);
    } catch (e) {
      console.error("Failed to delete AVD:", e);
    }
  }

  async function handleWipeAvd(avd: AvdInfo) {
    try {
      const result = await showDialog({
        title: "Wipe Device Data",
        message: `This will erase all user data for "${avd.displayName}" and restart it. Continue?`,
        buttons: [
          { label: "Wipe Data", value: "wipe", style: "danger" },
          { label: "Cancel", value: "cancel", style: "secondary" },
        ],
      });
      if (result !== "wipe") return;
      await wipeAvdData(avd.name);
      await handleRefresh();
    } catch (e) {
      console.error("Failed to wipe AVD data:", e);
    }
  }

  // ── Popover mode / Panel mode ────────────────────────────────────────────────

  return (
    <Show
      when={isPopover()}
      fallback={
        <div
          style={{
            display: "flex",
            "flex-direction": "column",
            height: "100%",
            overflow: "hidden",
            background: "var(--bg-secondary)",
          }}
        >
          {/* Toolbar */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "4px",
              padding: "4px 8px",
              background: "var(--bg-tertiary)",
              "border-bottom": "1px solid var(--border)",
              "flex-shrink": "0",
            }}
          >
            <ToolbarBtn
              title="Refresh devices"
              onClick={handleRefresh}
              disabled={refreshing()}
            >
              <Show when={refreshing()} fallback={<Icon name="refresh" size={13} />}>
                <span class="lsp-spinner">
                  <Icon name="spinner" size={13} color="var(--text-muted)" />
                </span>
              </Show>
            </ToolbarBtn>

            <div style={{ "flex": "1" }} />

            <button
              onClick={() => setShowCreateDialog(true)}
              title="Create new virtual device"
              style={{
                display: "flex",
                "align-items": "center",
                gap: "5px",
                padding: "3px 10px",
                background: "var(--accent)",
                color: "#fff",
                border: "none",
                "border-radius": "4px",
                "font-size": "11px",
                "font-weight": "500",
                cursor: "pointer",
                "flex-shrink": "0",
                transition: "opacity 0.1s",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.85"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
            >
              <span style={{ "font-size": "14px", "line-height": "1", "margin-top": "-1px" }}>+</span>
              New Device
            </button>
          </div>

          {/* Content area */}
          <div style={{ flex: "1", "overflow-y": "auto", padding: "12px 16px", display: "flex", "flex-direction": "column", gap: "20px" }}>

            {/* Connected Devices section */}
            <section>
              <SectionHeader label="Connected Devices" count={deviceState.devices.length} />
              <Show
                when={deviceState.devices.length > 0}
                fallback={
                  <EmptyState
                    icon="device"
                    title="No devices connected"
                    message="Connect a physical device via USB or launch a virtual device below."
                  />
                }
              >
                <div style={{ display: "flex", "flex-direction": "column", gap: "2px", "margin-top": "6px" }}>
                  <For each={deviceState.devices}>
                    {(device) => (
                      <DeviceRow
                        device={device}
                        selected={deviceState.selectedSerial === device.serial}
                        compact={false}
                        onSelect={() => pickDevice(device.serial)}
                        onStop={() => handleStopDevice(device.serial)}
                      />
                    )}
                  </For>
                </div>
              </Show>
            </section>

            {/* Virtual Devices section */}
            <section>
              <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}>
                <SectionHeader label="Virtual Devices" count={deviceState.avds.length} />
              </div>
              <Show
                when={deviceState.avds.length > 0}
                fallback={
                  <EmptyState
                    icon="device"
                    title="No virtual devices"
                    message='Create a virtual device to run your app in the emulator.'
                    action={{ label: "+ New Device", onClick: () => setShowCreateDialog(true) }}
                  />
                }
              >
                <div style={{ display: "flex", "flex-direction": "column", gap: "2px", "margin-top": "6px" }}>
                  <For each={deviceState.avds}>
                    {(avd) => (
                      <AvdRow
                        avd={avd}
                        isRunning={runningAvdNames().has(avd.name)}
                        runningSerial={serialForAvd()(avd.name)}
                        launching={launchingName() === avd.name}
                        onLaunch={() => handleLaunchAvd(avd)}
                        onStop={() => {
                          const serial = serialForAvd()(avd.name);
                          if (serial) handleStopDevice(serial);
                        }}
                        onDelete={() => handleDeleteAvd(avd)}
                        onWipe={() => handleWipeAvd(avd)}
                      />
                    )}
                  </For>
                </div>
              </Show>
            </section>
          </div>

          {/* Create Device Dialog */}
          <Show when={showCreateDialog()}>
            <CreateDeviceDialog
              onClose={() => setShowCreateDialog(false)}
              onCreated={(avds) => {
                setAvds(avds);
                setShowCreateDialog(false);
              }}
            />
          </Show>
        </div>
      }
    >
      {/* ── Popover mode ── */}
      <div
        style={{
          width: "280px",
          background: "var(--bg-secondary)",
          border: "1px solid var(--border)",
          "border-radius": "6px",
          "box-shadow": "0 4px 16px rgba(0,0,0,0.4)",
          overflow: "hidden",
          display: "flex",
          "flex-direction": "column",
          "max-height": "400px",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            padding: "8px 12px",
            background: "var(--bg-tertiary)",
            "border-bottom": "1px solid var(--border)",
            gap: "8px",
          }}
        >
          <span style={{ flex: "1", "font-size": "12px", "font-weight": "600", color: "var(--text-secondary)" }}>
            Select Device
          </span>
          <button
            onClick={handleRefresh}
            disabled={refreshing()}
            title="Refresh"
            style={{
              background: "none", border: "none", cursor: refreshing() ? "default" : "pointer",
              color: "var(--text-muted)", "font-size": "14px", opacity: refreshing() ? "0.4" : "1",
              display: "flex", "align-items": "center",
            }}
          >
            {refreshing() ? "…" : "↻"}
          </button>
        </div>

        <div style={{ "overflow-y": "auto", flex: "1" }}>
          <Show
            when={deviceState.devices.length > 0}
            fallback={
              <div style={{ padding: "16px 12px", color: "var(--text-muted)", "font-size": "11px", "text-align": "center" }}>
                No devices connected
              </div>
            }
          >
            <For each={deviceState.devices}>
              {(device) => (
                <DeviceRow
                  device={device}
                  selected={deviceState.selectedSerial === device.serial}
                  compact={true}
                  onSelect={() => { pickDevice(device.serial); props.onClose?.(); }}
                  onStop={() => handleStopDevice(device.serial)}
                />
              )}
            </For>
          </Show>
        </div>

        {/* Footer: Manage Devices link */}
        <div
          style={{
            padding: "6px 12px",
            "border-top": "1px solid var(--border)",
            background: "var(--bg-tertiary)",
          }}
        >
          <button
            onClick={() => { props.onClose?.(); toggleDeviceSidebar(); }}
            style={{
              background: "none", border: "none", cursor: "pointer",
              color: "var(--accent)", "font-size": "11px", padding: "0",
              width: "100%", "text-align": "left",
            }}
            onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.7"; }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
          >
            Manage Virtual Devices →
          </button>
        </div>
      </div>
    </Show>
  );
}

// ── Connected device row ──────────────────────────────────────────────────────

function DeviceRow(props: {
  device: Device;
  selected: boolean;
  compact: boolean;
  onSelect: () => void;
  onStop: () => void;
}): JSX.Element {
  const isOnline = () => props.device.connectionState === "online";
  const isEmulator = () => props.device.deviceKind === "emulator";

  const stateLabel = () => {
    switch (props.device.connectionState) {
      case "online":       return null;
      case "offline":      return "Offline";
      case "unauthorized": return "Unauthorized";
      default:             return "Unknown";
    }
  };

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        padding: props.compact ? "6px 12px" : "8px 12px",
        cursor: isOnline() ? "pointer" : "default",
        background: props.selected ? "var(--bg-active, rgba(255,255,255,0.08))" : "transparent",
        "border-left": props.selected ? "2px solid var(--accent)" : "2px solid transparent",
        "border-radius": "4px",
        gap: "10px",
        opacity: isOnline() ? "1" : "0.55",
        transition: "background 0.1s",
      }}
      onClick={() => isOnline() && props.onSelect()}
      onMouseEnter={(e) => {
        if (isOnline() && !props.selected)
          (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.04))";
      }}
      onMouseLeave={(e) => {
        if (!props.selected)
          (e.currentTarget as HTMLElement).style.background = "transparent";
      }}
    >
      {/* Status dot */}
      <span
        style={{
          width: "8px",
          height: "8px",
          "border-radius": "50%",
          "flex-shrink": "0",
          background: connectionStateColor(props.device.connectionState),
        }}
      />

      {/* Device info */}
      <div style={{ flex: "1", "min-width": "0" }}>
        <div
          style={{
            "font-size": "13px",
            "font-weight": "500",
            color: "var(--text-primary)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.device.model ?? props.device.name}
        </div>
        <Show when={!props.compact}>
          <div style={{ "font-size": "11px", color: "var(--text-muted)", display: "flex", gap: "6px", "margin-top": "1px", "flex-wrap": "wrap" }}>
            <span>{isEmulator() ? "Emulator" : "Physical"}</span>
            <Show when={props.device.apiLevel}>
              <span style={{ opacity: "0.5" }}>·</span>
              <span>API {props.device.apiLevel}</span>
            </Show>
            <Show when={props.device.androidVersion}>
              <span style={{ opacity: "0.5" }}>·</span>
              <span>Android {props.device.androidVersion}</span>
            </Show>
            <Show when={stateLabel()}>
              <span style={{ opacity: "0.5" }}>·</span>
              <span style={{ color: "var(--warning, #fbbf24)" }}>{stateLabel()}</span>
            </Show>
            <Show when={props.compact && stateLabel()}>
              <span style={{ color: "var(--warning, #fbbf24)", "font-size": "10px" }}>{stateLabel()}</span>
            </Show>
          </div>
        </Show>
        <Show when={props.compact}>
          <div style={{ "font-size": "10px", color: "var(--text-muted)" }}>
            {isEmulator() ? "Emulator" : "Physical"}
            {props.device.apiLevel ? ` · API ${props.device.apiLevel}` : ""}
          </div>
        </Show>
      </div>

      {/* Selected checkmark */}
      <Show when={props.selected}>
        <span style={{ color: "var(--accent)", "font-size": "13px", "flex-shrink": "0" }}>✓</span>
      </Show>

      {/* Stop button for running emulators */}
      <Show when={isEmulator() && isOnline()}>
        <button
          onClick={(e) => { e.stopPropagation(); props.onStop(); }}
          title="Stop emulator"
          style={{
            background: "none", border: "none", cursor: "pointer",
            color: "var(--text-muted)", "font-size": "12px",
            padding: "3px 5px", "border-radius": "3px",
            "flex-shrink": "0",
          }}
          onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--error, #f87171)"; (e.currentTarget as HTMLElement).style.background = "rgba(248,113,113,0.1)"; }}
          onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.color = "var(--text-muted)"; (e.currentTarget as HTMLElement).style.background = "none"; }}
        >
          ✕
        </button>
      </Show>
    </div>
  );
}

// ── AVD row ───────────────────────────────────────────────────────────────────

function AvdRow(props: {
  avd: AvdInfo;
  isRunning: boolean;
  runningSerial: string | null;
  launching: boolean;
  onLaunch: () => void;
  onStop: () => void;
  onDelete: () => void;
  onWipe: () => void;
}): JSX.Element {
  const [menuOpen, setMenuOpen] = createSignal(false);

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        padding: "8px 12px",
        gap: "10px",
        "border-radius": "4px",
        "border-left": "2px solid transparent",
        background: "transparent",
        transition: "background 0.1s",
        position: "relative",
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.04))"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
    >
      {/* AVD icon */}
      <div
        style={{
          width: "32px",
          height: "32px",
          "border-radius": "6px",
          background: "var(--bg-primary, rgba(255,255,255,0.06))",
          border: "1px solid var(--border)",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          "flex-shrink": "0",
        }}
      >
        <Icon name="device" size={16} color="var(--text-muted)" />
      </div>

      {/* AVD info */}
      <div style={{ flex: "1", "min-width": "0" }}>
        <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
          <span
            style={{
              "font-size": "13px",
              "font-weight": "500",
              color: "var(--text-primary)",
              overflow: "hidden",
              "text-overflow": "ellipsis",
              "white-space": "nowrap",
            }}
          >
            {props.avd.displayName}
          </span>
          <Show when={props.isRunning}>
            <span
              style={{
                "font-size": "10px",
                "font-weight": "500",
                padding: "1px 6px",
                "border-radius": "10px",
                background: "rgba(74,222,128,0.15)",
                color: "#4ade80",
                "flex-shrink": "0",
              }}
            >
              Running
            </span>
          </Show>
        </div>
        <div style={{ "font-size": "11px", color: "var(--text-muted)", display: "flex", gap: "6px", "margin-top": "2px" }}>
          <Show when={props.avd.abi}>
            <span>{props.avd.abi}</span>
          </Show>
          <Show when={props.avd.apiLevel}>
            <span style={{ opacity: "0.5" }}>·</span>
            <span>API {props.avd.apiLevel}</span>
          </Show>
        </div>
      </div>

      {/* Action buttons */}
      <div style={{ display: "flex", gap: "4px", "flex-shrink": "0", "align-items": "center" }}>
        <Show
          when={!props.isRunning}
          fallback={
            <button
              onClick={props.onStop}
              title="Stop emulator"
              style={{
                padding: "4px 10px",
                background: "rgba(248,113,113,0.12)",
                border: "1px solid rgba(248,113,113,0.3)",
                "border-radius": "4px",
                color: "#f87171",
                "font-size": "11px",
                cursor: "pointer",
                "font-weight": "500",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(248,113,113,0.22)"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "rgba(248,113,113,0.12)"; }}
            >
              Stop
            </button>
          }
        >
          <button
            onClick={() => props.onLaunch()}
            disabled={props.launching}
            title={`Launch ${props.avd.displayName}`}
            style={{
              padding: "4px 10px",
              background: "transparent",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              color: props.launching ? "var(--text-muted)" : "var(--text-secondary)",
              "font-size": "11px",
              cursor: props.launching ? "default" : "pointer",
              opacity: props.launching ? "0.6" : "1",
              "font-weight": "500",
            }}
            onMouseEnter={(e) => {
              if (!props.launching) {
                (e.currentTarget as HTMLElement).style.borderColor = "var(--accent)";
                (e.currentTarget as HTMLElement).style.color = "var(--accent)";
              }
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
              (e.currentTarget as HTMLElement).style.color = "var(--text-secondary)";
            }}
          >
            {props.launching ? "Launching…" : "Launch"}
          </button>
        </Show>

        {/* Overflow menu */}
        <div style={{ position: "relative" }}>
          <button
            onClick={(e) => { e.stopPropagation(); setMenuOpen((v) => !v); }}
            title="More options"
            style={{
              width: "26px",
              height: "26px",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              background: "none",
              border: "1px solid transparent",
              "border-radius": "4px",
              cursor: "pointer",
              color: "var(--text-muted)",
              "font-size": "14px",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.08))";
              (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLElement).style.background = "none";
              (e.currentTarget as HTMLElement).style.borderColor = "transparent";
            }}
          >
            ···
          </button>
          <Show when={menuOpen()}>
            <AvdContextMenu
              onClose={() => setMenuOpen(false)}
              isRunning={props.isRunning}
              onWipe={() => { setMenuOpen(false); props.onWipe(); }}
              onDelete={() => { setMenuOpen(false); props.onDelete(); }}
            />
          </Show>
        </div>
      </div>
    </div>
  );
}

// ── AVD context menu ──────────────────────────────────────────────────────────

function AvdContextMenu(props: {
  isRunning: boolean;
  onClose: () => void;
  onWipe: () => void;
  onDelete: () => void;
}): JSX.Element {
  return (
    <>
      {/* Click-away backdrop */}
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
          "min-width": "160px",
          padding: "4px",
          "white-space": "nowrap",
        }}
      >
        <ContextMenuItem
          label="Wipe Data…"
          onClick={props.onWipe}
          destructive={false}
        />
        <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
        <ContextMenuItem
          label="Delete…"
          onClick={props.onDelete}
          destructive={true}
        />
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

// ── Helpers ───────────────────────────────────────────────────────────────────

function SectionHeader(props: { label: string; count: number }): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        gap: "8px",
        "padding-bottom": "4px",
        "border-bottom": "1px solid var(--border)",
      }}
    >
      <span style={{ "font-size": "11px", "font-weight": "600", color: "var(--text-muted)", "text-transform": "uppercase", "letter-spacing": "0.06em" }}>
        {props.label}
      </span>
      <Show when={props.count > 0}>
        <span
          style={{
            "font-size": "10px",
            padding: "0px 5px",
            "border-radius": "8px",
            background: "var(--bg-primary, rgba(255,255,255,0.06))",
            color: "var(--text-muted)",
            border: "1px solid var(--border)",
          }}
        >
          {props.count}
        </span>
      </Show>
    </div>
  );
}

function EmptyState(props: {
  icon: string;
  title: string;
  message: string;
  action?: { label: string; onClick: () => void };
}): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        "justify-content": "center",
        gap: "8px",
        padding: "24px 16px",
        color: "var(--text-muted)",
        "text-align": "center",
      }}
    >
      <Icon name={props.icon} size={28} color="var(--text-muted)" />
      <div>
        <div style={{ "font-size": "13px", "font-weight": "500", color: "var(--text-secondary)", "margin-bottom": "4px" }}>
          {props.title}
        </div>
        <div style={{ "font-size": "11px", "line-height": "1.5", "max-width": "240px" }}>
          {props.message}
        </div>
      </div>
      <Show when={props.action}>
        {(action) => (
          <button
            onClick={action().onClick}
            style={{
              "margin-top": "4px",
              padding: "5px 14px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "4px",
              "font-size": "12px",
              "font-weight": "500",
              cursor: "pointer",
            }}
            onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.85"; }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
          >
            {action().label}
          </button>
        )}
      </Show>
    </div>
  );
}

function ToolbarBtn(props: {
  title: string;
  onClick: () => void;
  disabled?: boolean;
  children: JSX.Element;
}): JSX.Element {
  return (
    <button
      onClick={() => props.onClick()}
      disabled={props.disabled}
      title={props.title}
      style={{
        width: "26px",
        height: "26px",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        background: "none",
        border: "1px solid transparent",
        "border-radius": "4px",
        cursor: props.disabled ? "default" : "pointer",
        color: "var(--text-muted)",
        opacity: props.disabled ? "0.4" : "1",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        if (!props.disabled) {
          (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.08))";
          (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
        }
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = "none";
        (e.currentTarget as HTMLElement).style.borderColor = "transparent";
      }}
    >
      {props.children}
    </button>
  );
}

function connectionStateColor(state: Device["connectionState"]): string {
  switch (state) {
    case "online":       return "#4ade80";
    case "offline":      return "#6b7280";
    case "unauthorized": return "#fbbf24";
    default:             return "#6b7280";
  }
}
