import { createStore } from "solid-js/store";
import type { Device, AvdInfo, SystemImageInfo, DeviceDefinition } from "@/bindings";
import {
  refreshDevices,
  listAvdDevices,
  selectDevice as selectDeviceApi,
  listenDeviceListChanged,
  startDevicePolling,
} from "@/lib/tauri-api";
import { showToast } from "@/components/ui";
import { formatError } from "@/lib/tauri-api";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface DeviceStoreState {
  devices: Device[];
  avds: AvdInfo[];
  selectedSerial: string | null;
  launchingAvd: string | null;
  polling: boolean;
  systemImages: SystemImageInfo[];
  deviceDefinitions: DeviceDefinition[];
  creating: boolean;
}

// ── State ─────────────────────────────────────────────────────────────────────

const [deviceState, setDeviceState] = createStore<DeviceStoreState>({
  devices: [],
  avds: [],
  selectedSerial: null,
  launchingAvd: null,
  polling: false,
  systemImages: [],
  deviceDefinitions: [],
  creating: false,
});

export { deviceState };

// ── Derived ───────────────────────────────────────────────────────────────────

export function selectedDevice(): Device | null {
  return deviceState.devices.find((d) => d.serial === deviceState.selectedSerial) ?? null;
}

export function onlineDevices(): Device[] {
  return deviceState.devices.filter((d) => d.connectionState === "online");
}

export function deviceCount(): number {
  return onlineDevices().length;
}

/**
 * Set of AVD names that have a running emulator in the connected device list.
 * Matches by display name (lowercased, spaces normalized) against the emulator model name.
 */
export function runningAvdNames(): Set<string> {
  const running = new Set<string>();
  const emulators = deviceState.devices.filter(
    (d) => d.deviceKind === "emulator" && d.connectionState === "online"
  );
  for (const avd of deviceState.avds) {
    // The emulator model name from ADB is usually the AVD name with underscores replaced.
    const normalizedAvd = avd.name.toLowerCase().replace(/[\s_-]/g, "");
    for (const em of emulators) {
      const emModel = (em.model ?? em.name).toLowerCase().replace(/[\s_-]/g, "");
      if (
        normalizedAvd === emModel ||
        emModel.includes(normalizedAvd) ||
        normalizedAvd.includes(emModel)
      ) {
        running.add(avd.name);
        break;
      }
    }
  }
  return running;
}

/** Running serial for a given AVD name, if it's currently online. */
export function serialForAvd(avdName: string): string | null {
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

// ── Actions ───────────────────────────────────────────────────────────────────

export function setDevices(devices: Device[]): void {
  setDeviceState("devices", devices);

  // Drop a selection whose device is gone. Without this the stale serial stays
  // truthy forever, so auto-select never fires again and every build falls
  // through to the device picker.
  //
  // An EMPTY list is deliberately excluded: ADB briefly reports zero devices
  // when its server restarts, and dropping the selection there would discard
  // the user's choice over a transient blip.
  const current = deviceState.selectedSerial;
  if (
    current !== null &&
    devices.length > 0 &&
    !devices.some((d) => d.serial === current && d.connectionState === "online")
  ) {
    setDeviceState("selectedSerial", null);
  }

  // Auto-select the first online device if none is selected.
  if (!deviceState.selectedSerial) {
    const first = devices.find((d) => d.connectionState === "online");
    if (first) {
      setDeviceState("selectedSerial", first.serial);
    }
  }
}

export function setAvds(avds: AvdInfo[]): void {
  setDeviceState("avds", avds);
}

export function setSystemImages(images: SystemImageInfo[]): void {
  setDeviceState("systemImages", images);
}

export function setDeviceDefinitions(defs: DeviceDefinition[]): void {
  setDeviceState("deviceDefinitions", defs);
}

export function setCreating(v: boolean): void {
  setDeviceState("creating", v);
}

export async function pickDevice(serial: string): Promise<void> {
  const previous = deviceState.selectedSerial;
  setDeviceState("selectedSerial", serial);
  try {
    await selectDeviceApi(serial);
  } catch (err) {
    // The backend's selection is what MCP tools and every command that resolves
    // a `None` serial use. Silently keeping a local-only selection makes the UI
    // and the agent target different devices, so roll back and say so.
    setDeviceState("selectedSerial", previous);
    showToast(`Failed to select device: ${formatError(err)}`, "error");
    return;
  }
  // Notify the project service so it can persist per-project meta.
  _onDeviceChange?.(serial);
}

/** Registered by project.service.ts to avoid circular imports. */
let _onDeviceChange: ((serial: string) => void) | null = null;
export function onDeviceChange(cb: (serial: string) => void): void {
  _onDeviceChange = cb;
}

let deviceListUnlisten: (() => void) | null = null;
let deviceListListenerGeneration = 0;

export function setLaunchingAvd(avdName: string | null): void {
  setDeviceState("launchingAvd", avdName);
}

export async function initDevices(): Promise<void> {
  const initGeneration = deviceListListenerGeneration;
  const [devices, avds] = await Promise.all([
    refreshDevices().catch(() => [] as Device[]),
    listAvdDevices().catch(() => [] as AvdInfo[]),
  ]);
  if (initGeneration !== deviceListListenerGeneration) return;

  setDevices(devices);
  setAvds(avds);

  // Start polling and register event listener.
  if (!deviceState.polling) {
    setDeviceState("polling", true);
    await startDevicePolling().catch((err) => {
      console.error("[device] Failed to start device polling:", err);
      showToast(`Device polling failed: ${err}`, "error");
    });
    // eslint-disable-next-line solid/reactivity
    const unlisten = await listenDeviceListChanged((newDevices) => {
      setDevices(newDevices);
    });
    if (initGeneration !== deviceListListenerGeneration || !deviceState.polling) {
      unlisten();
      return;
    }
    deviceListUnlisten = unlisten;
  }
}

export function resetDeviceState(): void {
  deviceListListenerGeneration += 1;
  deviceListUnlisten?.();
  deviceListUnlisten = null;
  setDeviceState({
    devices: [],
    avds: [],
    selectedSerial: null,
    launchingAvd: null,
    polling: false,
    systemImages: [],
    deviceDefinitions: [],
    creating: false,
  });
}
