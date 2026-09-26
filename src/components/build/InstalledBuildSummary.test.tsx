import { cleanup, render, screen } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { InstalledBuild } from "@/bindings";
import { formatInstalledAt, installLabel, installsOf } from "./InstalledBuildSummary";
import { HistoryViewBanner } from "./BuildHistoryView";
import {
  resetBuildState,
  setBuildHistory,
  viewHistoryBuild,
  viewedBuild,
} from "@/stores/build.store";
import { makeBuildRecord, makeBuiltApk, makeInstalledBuild } from "@/test/factories/build";

const TODAY = new Date(2026, 8, 25, 18, 0);

describe("installLabel", () => {
  it("names the AVD and the time of an install today", () => {
    const install = makeInstalledBuild({
      installedAt: new Date(2026, 8, 25, 10, 32).toISOString(),
    });
    expect(installLabel(install, TODAY)).toBe(
      `Installed on Pixel_7 · ${formatInstalledAt(install.installedAt, TODAY)}`
    );
    expect(formatInstalledAt(install.installedAt, TODAY)).toMatch(/10.32/);
  });

  it("names a physical device by its model, else its serial, and an older install by its date", () => {
    const earlier = new Date(2026, 8, 20, 9, 5).toISOString();
    const phone = makeInstalledBuild({ avdName: null, model: "Pixel 8", installedAt: earlier });
    expect(installLabel(phone, TODAY)).toMatch(/^Installed on Pixel 8 · .*2026/);
    const unknown = makeInstalledBuild({ avdName: null, model: null, serial: "R5CT1234" });
    expect(installLabel(unknown)).toMatch(/^Installed on R5CT1234 · /);
  });
});

describe("installsOf", () => {
  const record = makeBuildRecord({ id: 4, apks: [makeBuiltApk()] });

  it("keeps installs of this build's APK", () => {
    const mine = makeInstalledBuild({ buildId: 4 });
    const other = makeInstalledBuild({ buildId: 3 });
    expect(installsOf(record, [mine, other])).toEqual([mine]);
  });

  it("ignores an install recorded for an earlier build with the same ID", () => {
    const reused = makeInstalledBuild({ buildId: 4, apkSha256: "ff".repeat(32) });
    expect(installsOf(record, [reused])).toEqual([]);
  });
});

describe("where a past build is installed, in the past-build banner", () => {
  function installed(list: InstalledBuild[]): void {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_installed_builds") return list;
      throw new Error(`unexpected ${cmd}`);
    });
  }

  beforeEach(() => {
    resetBuildState();
    setBuildHistory([
      makeBuildRecord({ id: 1, task: "assembleDebug" }),
      makeBuildRecord({ id: 2, task: "assembleRelease", apks: [makeBuiltApk()] }),
    ]);
  });

  afterEach(() => {
    cleanup();
    resetBuildState();
    vi.mocked(invoke).mockReset();
  });

  it("says where the build is installed, with the details in the tooltip", async () => {
    installed([
      makeInstalledBuild({ buildId: 2 }),
      makeInstalledBuild({ buildId: 2, avdName: null, model: "Pixel 8", serial: "R5CT1234" }),
    ]);
    viewHistoryBuild(2);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    const badge = await screen.findByText(/^Installed on Pixel_7 · /);
    expect(badge.getAttribute("title")).toBe(
      "com.example.app · Pixel_7 (emulator-5554) · version code 42 · SHA-256 a1a1a1a1a1a1…"
    );
    expect(await screen.findByText(/^Installed on Pixel 8 · /)).not.toBeNull();
  });

  it("says nothing for a build that is not installed anywhere", async () => {
    installed([makeInstalledBuild({ buildId: 7 })]);
    viewHistoryBuild(2);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    await vi.waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith("list_installed_builds"));
    expect(screen.queryByText(/^Installed on/)).toBeNull();
  });

  it("does not ask for a build that wrote no APK", () => {
    installed([]);
    viewHistoryBuild(1);
    render(() => <HistoryViewBanner view={viewedBuild()} onBack={() => {}} />);

    expect(vi.mocked(invoke)).not.toHaveBeenCalled();
  });
});
