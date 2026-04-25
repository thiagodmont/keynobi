import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  checkForAppUpdate,
  clearDismissedUpdateForTests,
  compareVersions,
  dismissUpdate,
  getDismissedUpdateTag,
  normalizeRelease,
  shouldDismissUpdatePrompt,
} from "./update.service";

describe("update.service", () => {
  beforeEach(() => {
    localStorage.clear();
    clearDismissedUpdateForTests();
    vi.unstubAllGlobals();
  });

  it("compares semantic versions with optional v prefixes", () => {
    expect(compareVersions("v0.1.19", "0.1.18")).toBe(1);
    expect(compareVersions("0.2.0", "v0.1.99")).toBe(1);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(compareVersions("0.1.17", "0.1.18")).toBe(-1);
  });

  it("ignores prerelease suffixes for update comparison", () => {
    expect(compareVersions("v0.2.0-beta.1", "0.1.18")).toBe(1);
    expect(compareVersions("v0.1.18-beta.1", "0.1.18")).toBe(0);
  });

  it("normalizes a GitHub release payload", () => {
    const release = normalizeRelease({
      tag_name: "v0.1.19",
      name: "Keynobi 0.1.19",
      html_url: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
      draft: false,
      prerelease: false,
    });

    expect(release).toEqual({
      tagName: "v0.1.19",
      version: "0.1.19",
      name: "Keynobi 0.1.19",
      releaseUrl: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
      prerelease: false,
    });
  });

  it("returns an available update when latest release is newer", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            tag_name: "v0.1.19",
            name: "Keynobi 0.1.19",
            html_url: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
            draft: false,
            prerelease: false,
          }),
      })
    );

    const update = await checkForAppUpdate("0.1.18");

    expect(update).toMatchObject({
      available: true,
      dismissed: false,
      currentVersion: "0.1.18",
      latestVersion: "0.1.19",
      tagName: "v0.1.19",
    });
  });

  it("marks an available update as dismissed after Later is clicked", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: () =>
          Promise.resolve({
            tag_name: "v0.1.19",
            name: "Keynobi 0.1.19",
            html_url: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
            draft: false,
            prerelease: false,
          }),
      })
    );

    dismissUpdate("v0.1.19");
    const update = await checkForAppUpdate("0.1.18");

    expect(getDismissedUpdateTag()).toBe("v0.1.19");
    expect(update.available).toBe(true);
    expect(update.dismissed).toBe(true);
  });

  it("only persists update dismissal for the explicit Later action", () => {
    expect(shouldDismissUpdatePrompt("later")).toBe(true);
    expect(shouldDismissUpdatePrompt("cancel")).toBe(false);
    expect(shouldDismissUpdatePrompt("download")).toBe(false);
  });

  it("returns unavailable when GitHub cannot be reached", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("offline")));

    const update = await checkForAppUpdate("0.1.18");

    expect(update).toEqual({
      available: false,
      dismissed: false,
      currentVersion: "0.1.18",
      error: "offline",
    });
  });
});
