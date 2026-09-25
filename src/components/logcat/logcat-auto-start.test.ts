import { describe, expect, it } from "vitest";
import type { Device, DeviceConnectionState } from "@/bindings";
import { pickAutoStartSerial } from "./logcat-auto-start";

function device(serial: string, connectionState: DeviceConnectionState = "online"): Device {
  return {
    serial,
    name: serial,
    model: null,
    deviceKind: "emulator",
    connectionState,
    apiLevel: null,
    androidVersion: null,
  };
}

describe("pickAutoStartSerial", () => {
  it("prefers the selected device over the first online one", () => {
    const devices = [device("emulator-5554"), device("emulator-5556")];
    expect(pickAutoStartSerial(devices, "emulator-5556")).toBe("emulator-5556");
  });

  it("falls back to the first online device when nothing is selected", () => {
    const devices = [device("emulator-5554", "offline"), device("emulator-5556")];
    expect(pickAutoStartSerial(devices, null)).toBe("emulator-5556");
  });

  it("falls back to the first online device when the selection is not online", () => {
    const devices = [device("emulator-5554"), device("emulator-5556", "unauthorized")];
    expect(pickAutoStartSerial(devices, "emulator-5556")).toBe("emulator-5554");
    expect(pickAutoStartSerial(devices, "gone-serial")).toBe("emulator-5554");
  });

  it("returns null when no device is online", () => {
    expect(pickAutoStartSerial([device("emulator-5554", "offline")], "emulator-5554")).toBeNull();
    expect(pickAutoStartSerial([], null)).toBeNull();
  });
});
