import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  healthState,
  healthChecks,
  refreshHealthChecks,
  setSystemReport,
  setHealthChecking,
} from "@/stores/health.store";
import { updateSetting } from "@/stores/settings.store";
import type { SystemHealthReport } from "@/bindings";

const mockInvoke = vi.mocked(invoke);

function report(over: Partial<SystemHealthReport> = {}): SystemHealthReport {
  return {
    androidSdkValid: true,
    adbFound: true,
    adbVersion: "1.0.41",
    emulatorFound: true,
    javaFound: true,
    javaVersion: "17.0.9",
    javaBinUsed: "/usr/bin/java",
    studioCommandFound: true,
    gradleWrapperFound: true,
    appDirWritable: true,
    diskFreeBytes: 100_000_000_000n,
    ...over,
  } as SystemHealthReport;
}

function checkById(id: string) {
  return healthChecks().find((c) => c.id === id);
}

describe("health.store", () => {
  beforeEach(() => {
    setHealthChecking(false);
    setSystemReport(null as unknown as SystemHealthReport);
    vi.clearAllMocks();
    updateSetting("android", "sdkPath", "/Users/dev/Library/Android/sdk");
  });

  it("reports the SDK check as ok when the path is set and valid", () => {
    setSystemReport(report());
    expect(checkById("android-sdk")?.status).toBe("ok");
  });

  it("reports an error and offers a fix when no SDK path is configured", () => {
    updateSetting("android", "sdkPath", null);
    setSystemReport(report());

    const check = checkById("android-sdk");
    expect(check?.status).toBe("error");
    expect(check?.fix).toBeDefined();
  });

  it("warns when the SDK path is set but the SDK is missing", () => {
    setSystemReport(report({ androidSdkValid: false }));
    expect(checkById("android-sdk")?.status).toBe("warning");
  });

  it("skips the ADB check when no SDK path is configured", () => {
    updateSetting("android", "sdkPath", null);
    setSystemReport(report({ adbFound: false }));
    expect(checkById("adb")?.status).toBe("skip");
  });

  it("warns on missing ADB when an SDK path is configured", () => {
    setSystemReport(report({ adbFound: false }));
    expect(checkById("adb")?.status).toBe("warning");
  });

  it("stores the report and marks the run finished", async () => {
    mockInvoke.mockResolvedValue(report());
    await refreshHealthChecks();

    expect(healthState.systemReport).not.toBeNull();
    expect(healthState.isRunning).toBe(false);
    expect(healthState.lastCheckedAt).toBeInstanceOf(Date);
  });

  it("does not start a second run while one is in flight", async () => {
    setHealthChecking(true);
    await refreshHealthChecks();

    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("clears the running flag when the command fails", async () => {
    mockInvoke.mockRejectedValue(new Error("no sdk"));
    await refreshHealthChecks();

    expect(healthState.isRunning).toBe(false);
  });
});
