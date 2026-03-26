import { createStore } from "solid-js/store";
import { createMemo } from "solid-js";
import type { Device, AvdInfo } from "@/bindings";
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
}

// ── State ─────────────────────────────────────────────────────────────────────

const [deviceState, setDeviceState] = createStore<DeviceStoreState>({
  devices: [],
  avds: [],
  selectedSerial: null,
  launchingAvd: null,
  polling: false,
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

export async function pickDevice(serial: string): Promise<void> {
  setDeviceState("selectedSerial", serial);
  try {
    await selectDeviceApi(serial);
  } catch {
    // Non-fatal — in-memory selection is still valid.
  }
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
    await startDevicePolling().catch(() => {});
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
  });
}
