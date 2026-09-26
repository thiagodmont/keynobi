import type {
  AppExitReasons,
  AppExitRecord,
  Device,
  AvdInfo,
  UiHierarchySnapshot,
  LaunchResult,
  LaunchTiming,
} from "@/bindings";
import { attachMockLaunch, recordMockInstall } from "./build";
import { recordMockLaunch } from "./sessions";
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

function mockExitRecord(
  time: string,
  pid: number,
  reason: AppExitRecord["reason"],
  reasonCode: number,
  reasonLabel: string,
  importance: number,
  importanceName: string,
  description: string | null
): AppExitRecord {
  return {
    timestamp: time.replace("T", " "),
    timestampLocal: time,
    pid,
    processName: "com.example.mockapp.debug",
    reason,
    reasonCode,
    reasonLabel,
    subReasonCode: 0,
    subReason: "UNKNOWN",
    status: 0,
    importance,
    importanceName,
    pssKb: 56_320,
    rssKb: 130_048,
    description,
  };
}

/** A device's exit history for the project's app: a crash, an ANR, and two kills. */
export function mockExitReasons(serial: string, pkg: string | null): AppExitReasons {
  const records = [
    mockExitRecord(
      "2026-09-25T10:15:03.482",
      12345,
      "crash",
      4,
      "APP CRASH(EXCEPTION)",
      100,
      "foreground",
      "crash"
    ),
    mockExitRecord(
      "2026-09-25T09:58:40.004",
      12001,
      "anr",
      6,
      "ANR",
      100,
      "foreground",
      "Input dispatching timed out (com.example.mockapp.debug/.MainActivity is not responding. Waited 5001ms for FocusEvent(hasFocus=true))"
    ),
    mockExitRecord(
      "2026-09-25T09:12:15.300",
      11876,
      "lowMemory",
      3,
      "LOW_MEMORY",
      400,
      "cached",
      null
    ),
    mockExitRecord(
      "2026-09-24T18:02:55.781",
      10442,
      "userRequested",
      10,
      "USER REQUESTED",
      100,
      "foreground",
      "remove task"
    ),
  ];
  return {
    serial,
    package: pkg ?? "com.example.mockapp.debug",
    apiLevel: 34,
    supported: true,
    message: null,
    records,
    totalRecords: records.length,
  };
}

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
    install_apk_on_device: (args: unknown) => {
      const { serial, apkPath } = args as { serial: string; apkPath: string };
      recordMockInstall(
        serial,
        mockDevices.find((d) => d.serial === serial),
        apkPath
      );
      return "Success";
    },
    launch_app_on_device: (args: unknown): LaunchResult => {
      const {
        serial,
        package: pkg,
        buildId,
      } = args as { serial: string; package?: string; buildId?: number | null };
      const device = mockDevices.find((d) => d.serial === serial);
      const timing: LaunchTiming = {
        totalMs: 812,
        waitMs: 815,
        launchState: "cold",
        measuredAt: new Date().toISOString(),
        serial,
        avdName: device?.avdName ?? null,
        model: device?.model ?? null,
        displayedMs: 790,
        fullyDrawnMs: null,
      };
      if (typeof buildId === "number") {
        attachMockLaunch(buildId, timing);
        // Like the backend: the app's reportFullyDrawn arrives after the launch returned.
        setTimeout(() => {
          const launch: LaunchTiming = { ...timing, fullyDrawnMs: 1400 };
          attachMockLaunch(buildId, launch);
          triggerEvent("build:launch_timing", { recordId: buildId, launch });
          if (typeof pkg === "string") recordMockLaunch(pkg, launch, true);
        }, 1000);
      }
      if (typeof pkg === "string") recordMockLaunch(pkg, timing);
      return { output: "Status: ok\nLaunchState: COLD\nTotalTime: 812\nWaitTime: 815", timing };
    },
    stop_app_on_device: () => undefined,
    get_exit_reasons: (args: unknown) => {
      const { serial, package: pkg } = (args ?? {}) as { serial?: string; package?: string | null };
      return mockExitReasons(serial ?? mockEmulator.serial, pkg ?? null);
    },
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
    get_package_name_from_apk: () => "com.example.mockapp.debug",
  };
}
