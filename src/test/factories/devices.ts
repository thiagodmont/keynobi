import type { AvdInfo, Device } from "@/bindings";

export function makeDevice(overrides: Partial<Device> = {}): Device {
  return {
    serial: "emulator-5554",
    name: "Pixel 6 API 34",
    model: "sdk_gphone64_x86_64",
    deviceKind: "emulator",
    connectionState: "online",
    apiLevel: 34,
    androidVersion: "14",
    ...overrides,
  };
}

export function makeAvd(overrides: Partial<AvdInfo> = {}): AvdInfo {
  return {
    name: "Pixel_6_API_34",
    displayName: "Pixel 6 API 34",
    target: "android-34",
    apiLevel: 34,
    abi: "x86_64",
    path: "/Users/user/.android/avd/Pixel_6_API_34.avd",
    ...overrides,
  };
}
