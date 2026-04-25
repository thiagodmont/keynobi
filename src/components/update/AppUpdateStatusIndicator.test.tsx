import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { dismissToast, toasts } from "@/components/ui";
import { setAppUpdateForTests } from "@/services/update.service";
import { AppUpdateStatusIndicator } from "./AppUpdateStatusIndicator";
import type * as UpdateService from "@/services/update.service";

vi.mock("@/services/update.service", async () => {
  const actual = await vi.importActual<typeof UpdateService>("@/services/update.service");
  return {
    ...actual,
    openUpdateRelease: vi.fn().mockResolvedValue(undefined),
  };
});

describe("AppUpdateStatusIndicator", () => {
  beforeEach(() => {
    setAppUpdateForTests(null);
    for (const toast of toasts()) dismissToast(toast.id);
    vi.clearAllMocks();
  });

  it("does not render when there is no available update", () => {
    setAppUpdateForTests({
      available: false,
      dismissed: false,
      currentVersion: "0.1.18",
    });

    render(() => <AppUpdateStatusIndicator />);

    expect(screen.queryByRole("button", { name: /update available/i })).toBeNull();
  });

  it("renders when an update is available even after the modal was dismissed", () => {
    setAppUpdateForTests({
      available: true,
      dismissed: true,
      currentVersion: "0.1.18",
      latestVersion: "0.1.19",
      tagName: "v0.1.19",
      releaseName: "Keynobi 0.1.19",
      releaseUrl: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
    });

    render(() => <AppUpdateStatusIndicator />);

    expect(screen.getByRole("button", { name: /update available/i }).textContent).toContain(
      "Update 0.1.19"
    );
  });

  it("opens the release link when clicked", async () => {
    const { openUpdateRelease } = await import("@/services/update.service");
    setAppUpdateForTests({
      available: true,
      dismissed: false,
      currentVersion: "0.1.18",
      latestVersion: "0.1.19",
      tagName: "v0.1.19",
      releaseName: "Keynobi 0.1.19",
      releaseUrl: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
    });

    render(() => <AppUpdateStatusIndicator />);
    fireEvent.click(screen.getByRole("button", { name: /update available/i }));

    expect(openUpdateRelease).toHaveBeenCalledWith(
      expect.objectContaining({
        releaseUrl: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
      })
    );
  });

  it("shows a toast when opening the release link fails", async () => {
    const { openUpdateRelease } = await import("@/services/update.service");
    vi.mocked(openUpdateRelease).mockRejectedValueOnce(new Error("blocked"));
    setAppUpdateForTests({
      available: true,
      dismissed: false,
      currentVersion: "0.1.18",
      latestVersion: "0.1.19",
      tagName: "v0.1.19",
      releaseName: "Keynobi 0.1.19",
      releaseUrl: "https://github.com/thiagodmont/keynobi/releases/tag/v0.1.19",
    });

    render(() => <AppUpdateStatusIndicator />);
    fireEvent.click(screen.getByRole("button", { name: /update available/i }));
    await Promise.resolve();

    expect(toasts()).toEqual([
      expect.objectContaining({
        kind: "error",
        message: "Failed to open release page: blocked",
      }),
    ]);
  });
});
