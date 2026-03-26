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
  pickDevice,
  setLaunchingAvd,
} from "@/stores/device.store";
import {
  refreshDevices,
  launchAvd,
  stopAvd,
  listAvdDevices,
} from "@/lib/tauri-api";
import type { Device, AvdInfo } from "@/bindings";

interface DevicePanelProps {
  onClose?: () => void;
}

export function DevicePanel(_props: DevicePanelProps): JSX.Element {
  const [refreshing, setRefreshing] = createSignal(false);
  const [launchingName, setLaunchingName] = createSignal<string | null>(null);

  onMount(() => {
    // Refresh on mount so the panel always shows current devices.
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
      import("@/stores/device.store").then(({ setAvds }) => setAvds(avds));
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

  return (
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
        "max-height": "480px",
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
        <span
          style={{
            flex: "1",
            "font-size": "12px",
            "font-weight": "600",
            color: "var(--text-secondary)",
          }}
        >
          Devices
        </span>
        <button
          onClick={handleRefresh}
          disabled={refreshing()}
          title="Refresh device list"
          style={{
            background: "none",
            border: "none",
            cursor: refreshing() ? "default" : "pointer",
            color: "var(--text-muted)",
            "font-size": "14px",
            opacity: refreshing() ? "0.4" : "1",
            display: "flex",
            "align-items": "center",
          }}
        >
          {refreshing() ? "…" : "↻"}
        </button>
      </div>

      <div style={{ "overflow-y": "auto", flex: "1" }}>
        {/* Connected devices */}
        <Show
          when={deviceState.devices.length > 0}
          fallback={
            <div
              style={{
                padding: "12px",
                color: "var(--text-muted)",
                "font-size": "11px",
              }}
            >
              No devices connected
            </div>
          }
        >
          <div
            style={{
              padding: "6px 12px 2px",
              "font-size": "10px",
              "font-weight": "600",
              color: "var(--text-muted)",
              "text-transform": "uppercase",
              "letter-spacing": "0.06em",
            }}
          >
            Connected
          </div>
          <For each={deviceState.devices}>
            {(device) => (
              <DeviceRow
                device={device}
                selected={deviceState.selectedSerial === device.serial}
                onSelect={() => pickDevice(device.serial)}
                onStop={() => handleStopDevice(device.serial)}
              />
            )}
          </For>
        </Show>

        {/* Available AVDs */}
        <Show when={deviceState.avds.length > 0}>
          <div
            style={{
              padding: "10px 12px 2px",
              "font-size": "10px",
              "font-weight": "600",
              color: "var(--text-muted)",
              "text-transform": "uppercase",
              "letter-spacing": "0.06em",
              "border-top": "1px solid var(--border)",
              "margin-top": "4px",
            }}
          >
            Available Emulators
          </div>
          <For each={deviceState.avds}>
            {(avd) => (
              <AvdRow
                avd={avd}
                launching={launchingName() === avd.name}
                onLaunch={() => handleLaunchAvd(avd)}
              />
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}

// ── Row components ────────────────────────────────────────────────────────────

function DeviceRow(props: {
  device: Device;
  selected: boolean;
  onSelect: () => void;
  onStop: () => void;
}): JSX.Element {
  const isOnline = () => props.device.connectionState === "online";
  const isEmulator = () => props.device.deviceKind === "emulator";

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        padding: "6px 12px",
        cursor: isOnline() ? "pointer" : "default",
        background: props.selected ? "var(--bg-active, rgba(255,255,255,0.08))" : "transparent",
        "border-left": props.selected ? "2px solid var(--accent)" : "2px solid transparent",
        gap: "8px",
        opacity: isOnline() ? "1" : "0.5",
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
          width: "6px",
          height: "6px",
          "border-radius": "50%",
          "flex-shrink": "0",
          background: statusColor(props.device.connectionState),
        }}
      />

      <div style={{ flex: "1", "min-width": "0" }}>
        <div
          style={{
            "font-size": "12px",
            color: "var(--text-primary)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.device.model ?? props.device.name}
        </div>
        <div
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
          }}
        >
          {isEmulator() ? "Emulator" : "Physical"}
          {props.device.apiLevel ? ` · API ${props.device.apiLevel}` : ""}
          {props.device.androidVersion ? ` · Android ${props.device.androidVersion}` : ""}
        </div>
      </div>

      {/* Selected checkmark */}
      <Show when={props.selected}>
        <span style={{ color: "var(--accent)", "font-size": "12px" }}>✓</span>
      </Show>

      {/* Stop button for emulators */}
      <Show when={isEmulator() && isOnline()}>
        <button
          onClick={(e) => {
            e.stopPropagation();
            props.onStop();
          }}
          title="Stop emulator"
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            color: "var(--text-muted)",
            "font-size": "11px",
            padding: "2px 4px",
            "border-radius": "3px",
          }}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.color = "var(--error, #f87171)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
          }}
        >
          ✕
        </button>
      </Show>
    </div>
  );
}

function AvdRow(props: {
  avd: AvdInfo;
  launching: boolean;
  onLaunch: () => void;
}): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        padding: "6px 12px",
        gap: "8px",
        "border-left": "2px solid transparent",
      }}
    >
      <div style={{ flex: "1", "min-width": "0" }}>
        <div
          style={{
            "font-size": "12px",
            color: "var(--text-secondary)",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.avd.displayName}
        </div>
        <div style={{ "font-size": "10px", color: "var(--text-muted)" }}>
          {props.avd.abi ?? "AVD"}
          {props.avd.apiLevel ? ` · API ${props.avd.apiLevel}` : ""}
        </div>
      </div>
      <button
        onClick={props.onLaunch}
        disabled={props.launching}
        title={`Launch ${props.avd.name}`}
        style={{
          background: "transparent",
          border: "1px solid var(--border)",
          "border-radius": "3px",
          padding: "2px 8px",
          cursor: props.launching ? "default" : "pointer",
          color: props.launching ? "var(--text-muted)" : "var(--text-secondary)",
          "font-size": "11px",
          "flex-shrink": "0",
          opacity: props.launching ? "0.6" : "1",
        }}
      >
        {props.launching ? "Launching…" : "Launch"}
      </button>
    </div>
  );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function statusColor(state: Device["connectionState"]): string {
  switch (state) {
    case "online":        return "#4ade80";
    case "offline":       return "#6b7280";
    case "unauthorized":  return "#fbbf24";
    default:              return "#6b7280";
  }
}
