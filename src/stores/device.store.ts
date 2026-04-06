import { createStore } from "solid-js/store";
import { createMemo } from "solid-js";
import type { Device, AvdInfo, SystemImageInfo, DeviceDefinition } from "@/bindings";
import {
  refreshDevices,
  listAvdDevices,
  selectDevice as selectDeviceApi,
  listenDeviceListChanged,
  startDevicePolling,
} from "@/lib/tauri-api";

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

export const selectedDevice = createMemo(
  () => deviceState.devices.find((d) => d.serial === deviceState.selectedSerial) ?? null
);

export const onlineDevices = createMemo(() =>
  deviceState.devices.filter((d) => d.connectionState === "online")
);

export const deviceCount = createMemo(() => onlineDevices().length);

/**
 * Set of AVD names that have a running emulator in the connected device list.
 * Matches by display name (lowercased, spaces normalized) against the emulator model name.
 */
export const runningAvdNames = createMemo((): Set<string> => {
  const running = new Set<string>();
  const emulators = deviceState.devices.filter(
    (d) => d.deviceKind === "emulator" && d.connectionState === "online"
  );
  for (const avd of deviceState.avds) {
    // The emulator model name from ADB is usually the AVD name with underscores replaced.
    const normalizedAvd = avd.name.toLowerCase().replace(/[\s_-]/g, "");
    for (const em of emulators) {
      const emModel = (em.model ?? em.name).toLowerCase().replace(/[\s_-]/g, "");
      if (normalizedAvd === emModel || emModel.includes(normalizedAvd) || normalizedAvd.includes(emModel)) {
        running.add(avd.name);
        break;
      }
    }
  }
  return running;
});

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
  setDeviceState("selectedSerial", serial);
  try {
    await selectDeviceApi(serial);
  } catch {
    // Non-fatal — in-memory selection is still valid.
  }
  // Notify the project service so it can persist per-project meta.
  _onDeviceChange?.(serial);
}

/** Registered by project.service.ts to avoid circular imports. */
let _onDeviceChange: ((serial: string) => void) | null = null;
export function onDeviceChange(cb: (serial: string) => void): void {
  _onDeviceChange = cb;
}

export function setLaunchingAvd(avdName: string | null): void {
  setDeviceState("launchingAvd", avdName);
}

export async function initDevices(): Promise<void> {
  const [devices, avds] = await Promise.all([
    refreshDevices().catch(() => [] as Device[]),
    listAvdDevices().catch(() => [] as AvdInfo[]),
  ]);
  setDevices(devices);
  setAvds(avds);

  // Start polling and register event listener.
  if (!deviceState.polling) {
    setDeviceState("polling", true);
    await startDevicePolling().catch((err) => {
      console.error("[device] Failed to start device polling:", err);
    });
    // eslint-disable-next-line solid/reactivity
    await listenDeviceListChanged((newDevices) => {
      setDevices(newDevices);
    });
  }
}

export function resetDeviceState(): void {
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
