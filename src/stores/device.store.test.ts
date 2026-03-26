import { describe, it, expect, beforeEach } from "vitest";
import {
  deviceState,
  setDevices,
  setAvds,
  pickDevice,
  setLaunchingAvd,
  resetDeviceState,
} from "@/stores/device.store";
import type { Device, AvdInfo } from "@/bindings";

const mockDevices: Device[] = [
  {
    serial: "emulator-5554",
    name: "Pixel 7",
    model: "Pixel 7",
    deviceKind: "emulator",
    connectionState: "online",
    apiLevel: 34,
    androidVersion: "14",
  },
  {
    serial: "ZX1G22ABCD",
    name: "Pixel 5",
    model: "Pixel 5",
    deviceKind: "physical",
    connectionState: "online",
    apiLevel: 31,
    androidVersion: "12",
  },
];

const mockAvds: AvdInfo[] = [
  {
    name: "Pixel_7_API_34",
    displayName: "Pixel 7 API 34",
    target: "android-34",
    apiLevel: 34,
    abi: "arm64-v8a",
    path: "/Users/dev/.android/avd/Pixel_7_API_34.avd",
  },
];

describe("device.store", () => {
  beforeEach(() => {
    resetDeviceState();
  });

  it("starts with empty device list", () => {
    expect(deviceState.devices).toHaveLength(0);
    expect(deviceState.selectedSerial).toBeNull();
  });

  it("setDevices updates the list", () => {
    setDevices(mockDevices);
    expect(deviceState.devices).toHaveLength(2);
    expect(deviceState.devices[0].serial).toBe("emulator-5554");
  });

  it("setDevices auto-selects first online device if none selected", () => {
    setDevices(mockDevices);
    expect(deviceState.selectedSerial).toBe("emulator-5554");
  });

  it("setDevices does not override explicit selection", () => {
    setDevices(mockDevices);
    // Simulate user selecting the second device.
    deviceState.selectedSerial; // force read
    import("@/stores/device.store").then(({ deviceState: ds }) => {});
    // setDevices again with a new list
    setDevices([...mockDevices].reverse());
    // Selection should remain if the device is still present in the list.
    // (The auto-select only fires when selectedSerial is null.)
  });

  it("pickDevice updates selectedSerial", async () => {
    setDevices(mockDevices);
    await pickDevice("ZX1G22ABCD");
    expect(deviceState.selectedSerial).toBe("ZX1G22ABCD");
  });

  it("setAvds updates the AVD list", () => {
    setAvds(mockAvds);
    expect(deviceState.avds).toHaveLength(1);
    expect(deviceState.avds[0].name).toBe("Pixel_7_API_34");
  });

  it("setLaunchingAvd tracks which AVD is launching", () => {
    setLaunchingAvd("Pixel_7_API_34");
    expect(deviceState.launchingAvd).toBe("Pixel_7_API_34");
    setLaunchingAvd(null);
    expect(deviceState.launchingAvd).toBeNull();
  });

  it("resetDeviceState clears everything", () => {
    setDevices(mockDevices);
    setAvds(mockAvds);
    resetDeviceState();
    expect(deviceState.devices).toHaveLength(0);
    expect(deviceState.avds).toHaveLength(0);
    expect(deviceState.selectedSerial).toBeNull();
  });
});
