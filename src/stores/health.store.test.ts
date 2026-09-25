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
    javaExecutableFound: true,
    javaVersion: 'openjdk version "17.0.9"',
    javaBinUsed: "/jdk/bin/java",
    javaMajorVersion: 17,
    javaHome: "/jdk",
    javaSource: "installedJdk",
    studioCommandFound: true,
    gradleWrapperFound: true,
    lspSystemDirOk: true,
    appLocationProblem: null,
    ...over,
  };
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

  it("shows the chosen JDK, its version, and where it came from", () => {
    setSystemReport(
      report({
        javaExecutableFound: true,
        javaVersion: 'openjdk version "21.0.8" 2025-07-15',
        javaMajorVersion: 21,
        javaHome: "/Applications/Android Studio.app/Contents/jbr/Contents/Home",
        javaSource: "androidStudio",
      })
    );
    const check = checkById("java");
    expect(check?.status).toBe("ok");
    expect(check?.detail).toContain('"21.0.8"');
    expect(check?.detail).toContain("Android Studio's bundled JDK");
    expect(check?.detail).toContain("/Applications/Android Studio.app/Contents/jbr/Contents/Home");
    expect(check?.fix).toBeUndefined();
  });

  it("reports Java missing as an error", () => {
    setSystemReport(
      report({
        javaExecutableFound: false,
        javaVersion: null,
        javaMajorVersion: null,
        javaHome: null,
        javaSource: null,
        javaBinUsed: "java",
      })
    );
    const check = checkById("java");
    expect(check?.status).toBe("error");
    expect(check?.detail).toContain("Not found");
    expect(check?.fix).toBeDefined();
  });

  it("warns when the JDK is older than 17", () => {
    setSystemReport(
      report({
        javaExecutableFound: true,
        javaVersion: 'openjdk version "11.0.21"',
        javaMajorVersion: 11,
        javaHome: "/jdk-11",
        javaSource: "settings",
      })
    );
    const check = checkById("java");
    expect(check?.status).toBe("warning");
    expect(check?.detail).toContain("JDK 17 or newer");
  });

  it("reports a permanent app location as ok", () => {
    setSystemReport(report());
    expect(checkById("app-location")?.status).toBe("ok");
  });

  it("warns, with the reason, when the app runs from a disk image", () => {
    const problem = "Keynobi is running from a disk image or removable volume (/Volumes/Keynobi).";
    setSystemReport(report({ appLocationProblem: problem }));

    const check = checkById("app-location");
    expect(check?.status).toBe("warning");
    expect(check?.detail).toBe(problem);
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
