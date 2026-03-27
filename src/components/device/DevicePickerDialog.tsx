/**
 * DevicePickerDialog — shown when "Run App" is triggered but no online device
 * is currently selected.  Lets the user pick any connected device or start an
 * AVD from the list, then resolves with the serial of the chosen device.
 */
import { type JSX, For, Show, createSignal, createMemo } from "solid-js";
import { deviceState, setLaunchingAvd } from "@/stores/device.store";
import { launchAvd } from "@/lib/tauri-api";
import type { Device, AvdInfo } from "@/bindings";
import Icon from "@/components/common/Icon";

// ── Module-level promise resolver ─────────────────────────────────────────────

type Resolver = (serial: string | null) => void;
let _resolver: Resolver | null = null;
let _setVisible: ((v: boolean) => void) | null = null;

/**
 * Open the picker and wait for the user to choose a device (or cancel).
 * Returns the serial of the chosen device, or null if cancelled.
 */
export function showDevicePicker(): Promise<string | null> {
  return new Promise<string | null>((resolve) => {
    _resolver = resolve;
    _setVisible?.(true);
  });
}

function resolve(serial: string | null) {
  _resolver?.(serial);
  _resolver = null;
  _setVisible?.(false);
}

// ── Component ─────────────────────────────────────────────────────────────────

export function DevicePickerDialog(): JSX.Element {
  const [visible, setVisible] = createSignal(false);
  const [launching, setLaunching] = createSignal<string | null>(null);
  const [launchError, setLaunchError] = createSignal<string | null>(null);

  // Register the setter so showDevicePicker() can open us.
  _setVisible = setVisible;

  const onlineDevices = createMemo(() =>
    deviceState.devices.filter((d) => d.connectionState === "online")
  );

  const avds = createMemo(() => deviceState.avds);

  async function pickDevice(device: Device) {
    resolve(device.serial);
  }

  async function pickAvd(avd: AvdInfo) {
    // If already running, just resolve with its serial.
    const running = findRunningSerial(avd.name);
    if (running) {
      resolve(running);
      return;
    }
    // Start the emulator, wait for it to boot, then resolve.
    setLaunching(avd.name);
    setLaunchError(null);
    setLaunchingAvd(avd.name);
    try {
      const serial = await launchAvd(avd.name);
      setLaunchingAvd(null);
      resolve(serial);
    } catch (e) {
      setLaunchError(typeof e === "string" ? e : (e as Error).message ?? "Failed to start emulator");
      setLaunchingAvd(null);
    } finally {
      setLaunching(null);
    }
  }

  function cancel() {
    resolve(null);
  }

  function findRunningSerial(avdName: string): string | null {
    const normalized = avdName.toLowerCase().replace(/[\s_-]/g, "");
    for (const d of deviceState.devices) {
      if (d.deviceKind !== "emulator" || d.connectionState !== "online") continue;
      const emModel = (d.model ?? d.name).toLowerCase().replace(/[\s_-]/g, "");
      if (normalized === emModel || emModel.includes(normalized) || normalized.includes(emModel)) {
        return d.serial;
      }
    }
    return null;
  }

  const hasContent = createMemo(() => onlineDevices().length > 0 || avds().length > 0);

  return (
    <Show when={visible()}>
      {/* Backdrop */}
      <div
        onClick={cancel}
        style={{
          position: "fixed",
          inset: "0",
          background: "rgba(0,0,0,0.55)",
          "z-index": "9000",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
        }}
      >
        {/* Dialog */}
        <div
          onClick={(e) => e.stopPropagation()}
          style={{
            background: "var(--bg-secondary)",
            border: "1px solid var(--border)",
            "border-radius": "8px",
            width: "400px",
            "max-width": "90vw",
            "max-height": "70vh",
            display: "flex",
            "flex-direction": "column",
            overflow: "hidden",
            "box-shadow": "0 16px 48px rgba(0,0,0,0.5)",
          }}
        >
          {/* Header */}
          <div
            style={{
              padding: "14px 16px 10px",
              "border-bottom": "1px solid var(--border)",
              "flex-shrink": "0",
            }}
          >
            <div style={{ "font-size": "13px", "font-weight": "600", color: "var(--text-primary)", "margin-bottom": "2px" }}>
              Select a Device
            </div>
            <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
              Choose where to install and run your app
            </div>
          </div>

          {/* Content */}
          <div style={{ flex: "1", "overflow-y": "auto", padding: "8px 0" }}>
            <Show when={!hasContent()}>
              <div
                style={{
                  padding: "32px 16px",
                  "text-align": "center",
                  color: "var(--text-muted)",
                  "font-size": "12px",
                  "line-height": "1.6",
                }}
              >
                <Icon name="device" size={32} color="var(--text-muted)" />
                <div style={{ "margin-top": "10px" }}>No devices or emulators found.</div>
                <div>Connect a physical device or create a virtual device first.</div>
              </div>
            </Show>

            {/* Online physical / emulator devices */}
            <Show when={onlineDevices().length > 0}>
              <SectionHeader label="Connected Devices" />
              <For each={onlineDevices()}>
                {(device) => (
                  <DeviceRow
                    label={device.model ?? device.name}
                    sublabel={`${device.deviceKind === "emulator" ? "Emulator" : "Physical"} · Android ${device.androidVersion ?? "?"} (API ${device.apiLevel ?? "?"})`}
                    serial={device.serial}
                    icon={device.deviceKind === "emulator" ? "device" : "device"}
                    onClick={() => pickDevice(device)}
                  />
                )}
              </For>
            </Show>

            {/* AVDs */}
            <Show when={avds().length > 0}>
              <SectionHeader label="Virtual Devices (AVDs)" />
              <For each={avds()}>
                {(avd) => {
                  const runningSerial = () => findRunningSerial(avd.name);
                  const isRunning = () => !!runningSerial();
                  const isLaunching = () => launching() === avd.name;
                  return (
                    <DeviceRow
                      label={avd.displayName}
                      sublabel={`Android ${avd.apiLevel ?? "?"} · ${avd.abi ?? "unknown"}`}
                      icon="device"
                      running={isRunning()}
                      launching={isLaunching()}
                      onClick={() => pickAvd(avd)}
                    />
                  );
                }}
              </For>
            </Show>

            <Show when={launchError()}>
              <div
                style={{
                  margin: "8px 12px",
                  padding: "8px 10px",
                  "border-radius": "4px",
                  background: "rgba(248,113,113,0.12)",
                  "border-left": "3px solid var(--error, #f87171)",
                  "font-size": "11px",
                  color: "var(--error, #f87171)",
                  "word-break": "break-word",
                }}
              >
                {launchError()}
              </div>
            </Show>
          </div>

          {/* Footer */}
          <div
            style={{
              padding: "10px 16px",
              "border-top": "1px solid var(--border)",
              display: "flex",
              "justify-content": "flex-end",
              gap: "8px",
              "flex-shrink": "0",
            }}
          >
            <button
              onClick={cancel}
              style={{
                padding: "5px 14px",
                "border-radius": "4px",
                border: "1px solid var(--border)",
                background: "transparent",
                color: "var(--text-secondary)",
                "font-size": "12px",
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}

// ── Sub-components ────────────────────────────────────────────────────────────

function SectionHeader(props: { label: string }): JSX.Element {
  return (
    <div
      style={{
        padding: "4px 14px 2px",
        "font-size": "10px",
        "font-weight": "600",
        "text-transform": "uppercase",
        "letter-spacing": "0.06em",
        color: "var(--text-muted)",
      }}
    >
      {props.label}
    </div>
  );
}

function DeviceRow(props: {
  label: string;
  sublabel: string;
  icon: string;
  serial?: string;
  running?: boolean;
  launching?: boolean;
  onClick: () => void;
}): JSX.Element {
  return (
    <button
      onClick={props.launching ? undefined : props.onClick}
      disabled={props.launching}
      style={{
        display: "flex",
        "align-items": "center",
        gap: "10px",
        padding: "8px 14px",
        width: "100%",
        background: "transparent",
        border: "none",
        cursor: props.launching ? "default" : "pointer",
        "text-align": "left",
        opacity: props.launching ? "0.7" : "1",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        if (!props.launching)
          (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.06))";
      }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "transparent"; }}
    >
      {/* Icon */}
      <div
        style={{
          width: "30px",
          height: "30px",
          "border-radius": "6px",
          background: "var(--bg-tertiary)",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          "flex-shrink": "0",
        }}
      >
        <Icon name={props.icon} size={16} color="var(--text-secondary)" />
      </div>

      {/* Labels */}
      <div style={{ flex: "1", "min-width": "0" }}>
        <div style={{ "font-size": "12px", "font-weight": "500", color: "var(--text-primary)", "white-space": "nowrap", overflow: "hidden", "text-overflow": "ellipsis" }}>
          {props.label}
        </div>
        <div style={{ "font-size": "10px", color: "var(--text-muted)", "margin-top": "1px" }}>
          {props.sublabel}
        </div>
      </div>

      {/* Status badge */}
      <div style={{ "flex-shrink": "0" }}>
        <Show when={props.launching}>
          <span class="lsp-spinner">
            <Icon name="spinner" size={12} color="#60a5fa" />
          </span>
        </Show>
        <Show when={props.running && !props.launching}>
          <span
            style={{
              "font-size": "10px",
              color: "#4ade80",
              background: "rgba(74,222,128,0.12)",
              padding: "1px 6px",
              "border-radius": "10px",
              "font-weight": "500",
            }}
          >
            Running
          </span>
        </Show>
        <Show when={!props.running && !props.launching && props.serial === undefined}>
          <span
            style={{
              "font-size": "10px",
              color: "var(--text-muted)",
              background: "var(--bg-tertiary)",
              padding: "1px 6px",
              "border-radius": "10px",
            }}
          >
            Start
          </span>
        </Show>
      </div>
    </button>
  );
}
