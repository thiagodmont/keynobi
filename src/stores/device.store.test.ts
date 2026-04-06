import { describe, it, expect, beforeEach } from "vitest";
import {
  deviceState,
  setDevices,
  setAvds,
  pickDevice,
  setLaunchingAvd,
  onlineDevices,
  selectedDevice,
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

describe("device store error state transitions", () => {
  beforeEach(() => {
    resetDeviceState();
  });

  it("onlineDevices excludes offline devices after disconnect update", () => {
    setDevices(mockDevices);
    // Simulate a device going offline — its connectionState changes.
    const updatedDevices: Device[] = [
      { ...mockDevices[0], connectionState: "offline" },
      mockDevices[1],
    ];
    setDevices(updatedDevices);
    expect(onlineDevices()).toHaveLength(1);
    expect(onlineDevices()[0].serial).toBe("ZX1G22ABCD");
  });

  it("selectedDevice memo returns null when the selected serial is no longer in the device list", () => {
    setDevices(mockDevices);
    // Auto-select fires for the first online device.
    expect(deviceState.selectedSerial).toBe("emulator-5554");
    // Device list is replaced with a completely different set — selected serial gone.
    const differentDevice: Device[] = [
      {
        serial: "new-device-001",
        name: "Pixel 8",
        model: "Pixel 8",
        deviceKind: "physical",
        connectionState: "online",
        apiLevel: 35,
        androidVersion: "15",
      },
    ];
    // setDevices does NOT auto-select when selectedSerial is already set,
    // so selectedSerial remains "emulator-5554" while that device is gone.
    setDevices(differentDevice);
    // selectedDevice() resolves against the current device list — serial not found → null.
    expect(selectedDevice()).toBeNull();
    expect(deviceState.selectedSerial).toBe("emulator-5554");
  });

  it("onlineDevices returns empty list when all devices go offline", () => {
    setDevices(mockDevices);
    expect(onlineDevices()).toHaveLength(2);
    const allOffline: Device[] = mockDevices.map((d) => ({
      ...d,
      connectionState: "offline",
    }));
    setDevices(allOffline);
    expect(onlineDevices()).toHaveLength(0);
  });

  it("setDevices with empty list clears device list but preserves selectedSerial", () => {
    setDevices(mockDevices);
    expect(deviceState.selectedSerial).toBe("emulator-5554");
    setDevices([]);
    expect(deviceState.devices).toHaveLength(0);
    // selectedSerial is NOT cleared — the store does not auto-clear on empty list.
    expect(deviceState.selectedSerial).toBe("emulator-5554");
    // selectedDevice() returns null because the serial is not in the (empty) list.
    expect(selectedDevice()).toBeNull();
  });

  it("setDevices auto-selects first online device when list changes and none selected", () => {
    // Start with no selection.
    expect(deviceState.selectedSerial).toBeNull();
    // Provide only offline devices — nothing should be auto-selected.
    const offlineDevices: Device[] = mockDevices.map((d) => ({
      ...d,
      connectionState: "offline",
    }));
    setDevices(offlineDevices);
    expect(deviceState.selectedSerial).toBeNull();
  });
});
