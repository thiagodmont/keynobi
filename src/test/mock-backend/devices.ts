import type { Device, AvdInfo, UiHierarchySnapshot } from "@/bindings";
import { triggerEvent } from "./events";

export const mockEmulator: Device = {
  serial: "emulator-5554",
  name: "Pixel 6 API 34",
  model: "sdk_gphone64_x86_64",
  deviceKind: "emulator",
  connectionState: "online",
  apiLevel: 34,
  androidVersion: "14",
  avdName: "Pixel_6_API_34",
};

export const mockPhone: Device = {
  serial: "28151FDH2000Q4",
  name: "panther",
  model: "Pixel 7",
  deviceKind: "physical",
  connectionState: "online",
  apiLevel: 35,
  androidVersion: "15",
};

const mockDevices = [mockEmulator, mockPhone];

export const mockAvd: AvdInfo = {
  name: "Pixel_6_API_34",
  displayName: "Pixel 6 API 34",
  target: "android-34",
  apiLevel: 34,
  abi: "x86_64",
  path: "/Users/user/.android/avd/Pixel_6_API_34.avd",
};

let selectedDevice: string | null = null;

export function devicesHandlers(): Record<string, (args: unknown) => unknown> {
  return {
    list_adb_devices: () => [...mockDevices],
    refresh_devices: () => {
      triggerEvent("device:list_changed", { devices: [...mockDevices] });
      return [...mockDevices];
    },
    select_device: (args: unknown) => {
      selectedDevice = (args as { serial: string }).serial;
    },
    get_selected_device: () => selectedDevice,
    list_avd_devices: () => [mockAvd],
    launch_avd: () => "emulator-5554",
    stop_avd: () => undefined,
    start_device_polling: () => undefined,
    stop_device_polling: () => undefined,
    install_apk_on_device: () => "Success",
    launch_app_on_device: () => "Started",
    stop_app_on_device: () => undefined,
    list_system_images_cmd: () => [],
    list_device_definitions_cmd: () => [],
    create_avd_device: () => [mockAvd],
    delete_avd_device: () => [],
    wipe_avd_data_cmd: () => undefined,
    list_available_system_images_cmd: () => [],
    download_system_image_cmd: () => undefined,
    dump_ui_hierarchy: (): UiHierarchySnapshot => ({
      capturedAt: new Date().toISOString(),
      truncated: false,
      warnings: [],
      root: {
        class: "android.widget.FrameLayout",
        resourceId: "",
        text: "",
        contentDesc: "",
        package: "com.example.mockapp",
        bounds: "[0,0][1080,2400]",
        clickable: false,
        enabled: true,
        focusable: false,
        focused: false,
        scrollable: false,
        longClickable: false,
        password: false,
        checkable: false,
        checked: false,
        editable: false,
        selected: false,
        isComposeHeuristic: false,
        children: [],
      },
      screenHash: "mock",
      interactiveCount: 0,
      foregroundActivity: null,
      layoutContext: {},
      commandLog: [],
    }),
    find_apk_path: () => "/mock/app-debug.apk",
    get_package_name_from_apk: () => "com.example.mockapp.debug",
  };
}
