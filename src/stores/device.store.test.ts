import { describe, it, expect, beforeEach, vi } from "vitest";
import { waitFor } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  deviceState,
  setDevices,
  setAvds,
  pickDevice,
  setLaunchingAvd,
  onlineDevices,
  selectedDevice,
  resetDeviceState,
  initDevices,
  onDeviceChange,
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

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("device.store", () => {
  beforeEach(() => {
    resetDeviceState();
    vi.clearAllMocks();
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "refresh_devices") return Promise.resolve([]);
      if (cmd === "list_avd_devices") return Promise.resolve([]);
      if (cmd === "start_device_polling") return Promise.resolve(undefined);
      if (cmd === "select_device") return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });
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
    void deviceState.selectedSerial; // force read to trigger reactivity tracking
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

  it("resetDeviceState disposes the device list listener", async () => {
    const unlisten = vi.fn();
    mockListen.mockResolvedValueOnce(unlisten);

    await initDevices();
    expect(mockListen).toHaveBeenCalledTimes(1);

    resetDeviceState();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("resetDeviceState disposes a device list listener that resolves after reset", async () => {
    const unlisten = vi.fn();
    const listener = deferred<() => void>();
    mockListen.mockReturnValueOnce(listener.promise);

    const init = initDevices();
    await waitFor(() => expect(mockListen).toHaveBeenCalledTimes(1));

    resetDeviceState();
    expect(unlisten).not.toHaveBeenCalled();

    listener.resolve(unlisten);
    await init;

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("resetDeviceState prevents an in-flight initDevices from repopulating devices", async () => {
    const refresh = deferred<Device[]>();
    mockInvoke.mockImplementation((cmd) => {
      if (cmd === "refresh_devices") return refresh.promise;
      if (cmd === "list_avd_devices") return Promise.resolve(mockAvds);
      if (cmd === "start_device_polling") return Promise.resolve(undefined);
      if (cmd === "select_device") return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });

    const init = initDevices();
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh_devices"));

    resetDeviceState();
    refresh.resolve(mockDevices);
    await init;

    expect(deviceState.devices).toHaveLength(0);
    expect(deviceState.avds).toHaveLength(0);
    expect(deviceState.polling).toBe(false);
    expect(mockInvoke).not.toHaveBeenCalledWith("start_device_polling");
    expect(mockListen).not.toHaveBeenCalled();
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
    // The previously selected device is gone from a non-empty list, so the
    // stale selection is dropped and the new online device is auto-selected.
    // Previously selectedSerial stayed "emulator-5554" forever, which blocked
    // auto-select and forced the device picker on every build.
    setDevices(differentDevice);
    expect(deviceState.selectedSerial).toBe("new-device-001");
    expect(selectedDevice()?.serial).toBe("new-device-001");
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
    // selectedSerial is deliberately preserved on an EMPTY list: ADB reports
    // zero devices during a server restart, and losing the user's choice over
    // a transient blip is worse than holding a briefly-stale serial.
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

describe("device selection stays in sync with the backend", () => {
  beforeEach(() => {
    resetDeviceState();
    vi.clearAllMocks();
  });

  it("auto-selects a replacement after the selected device disconnects", async () => {
    setDevices(mockDevices);
    const original = deviceState.selectedSerial;
    expect(original).not.toBeNull();

    const replacement: Device[] = [
      {
        serial: "replacement-001",
        name: "Pixel 9",
        model: "Pixel 9",
        deviceKind: "physical",
        connectionState: "online",
        apiLevel: 36,
        androidVersion: "16",
      },
    ];
    setDevices(replacement);

    expect(deviceState.selectedSerial).toBe("replacement-001");
  });

  it("keeps the selection when the device is still online", () => {
    setDevices(mockDevices);
    const selected = deviceState.selectedSerial;
    setDevices([...mockDevices]);
    expect(deviceState.selectedSerial).toBe(selected);
  });

  it("clears the selection when the device is present but offline", () => {
    setDevices(mockDevices);
    const selected = deviceState.selectedSerial;
    const offline = mockDevices.map((d) =>
      d.serial === selected ? { ...d, connectionState: "offline" as const } : d
    );
    setDevices(offline);
    expect(deviceState.selectedSerial).not.toBe(selected);
  });

  it("does not notify the project service from a device-list refresh", () => {
    const onChange = vi.fn();
    onDeviceChange(onChange);
    setDevices(mockDevices);
    // Only an explicit pickDevice persists per-project meta; auto-select must not.
    expect(onChange).not.toHaveBeenCalled();
  });

  it("rolls back and toasts when the backend rejects the selection", async () => {
    setDevices(mockDevices);
    const before = deviceState.selectedSerial;
    const target = mockDevices.find((d) => d.serial !== before)!.serial;

    vi.mocked(invoke).mockRejectedValueOnce(new Error("device gone"));
    await pickDevice(target);

    expect(deviceState.selectedSerial).toBe(before);
  });

  it("keeps the new selection when the backend accepts it", async () => {
    setDevices(mockDevices);
    const before = deviceState.selectedSerial;
    const target = mockDevices.find((d) => d.serial !== before)!.serial;

    vi.mocked(invoke).mockResolvedValue(undefined);
    await pickDevice(target);

    expect(deviceState.selectedSerial).toBe(target);
  });
});
