import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { makeBuildRecord, makeBuiltApk } from "@/test/factories/build";
import { makeSession, summaryOf } from "@/test/factories/sessions";
import { BuildSessionLinks, sessionsOfBuild } from "./BuildSessionLinks";
import { SessionsDialog, closeSessionsDialog } from "./SessionsDialog";

const apk = makeBuiltApk({ sha256: "a1".repeat(32) });
const record = makeBuildRecord({ id: 12, apks: [apk] });

const pixel = summaryOf(
  makeSession({
    id: "s-20260925T110000Z-000000000003",
    counts: { launches: 1, crashes: 2, anrs: 0, exits: 0, bookmarks: 0, captures: 2 },
  })
);
const olderPixel = summaryOf(makeSession({ id: "s-20260925T100000Z-000000000001" }));
const phone = summaryOf(
  makeSession({
    id: "s-20260925T105000Z-000000000002",
    device: { serial: "R58M", avdName: null, model: "SM-G991B" },
  })
);
/** Build ID 12 reused after the history was cleared: another APK. */
const reused = summaryOf(
  makeSession({
    id: "s-20260925T104000Z-000000000004",
    device: { serial: "emulator-5556", avdName: "Pixel_8", model: null },
    install: { apkSha256: "ff".repeat(32), versionCode: 1, installedAt: "", by: { kind: "app" } },
  })
);

describe("sessionsOfBuild", () => {
  it("keeps the newest session of the build's APK on each device", () => {
    expect(sessionsOfBuild(record, [pixel, phone, reused, olderPixel]).map((s) => s.id)).toEqual([
      pixel.id,
      phone.id,
    ]);
    expect(sessionsOfBuild(makeBuildRecord({ id: 13, apks: [apk] }), [pixel])).toEqual([]);
  });
});

describe("BuildSessionLinks", () => {
  beforeEach(() => {
    if (!window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;
    }
    vi.mocked(invoke).mockReset();
  });

  afterEach(() => {
    closeSessionsDialog();
    cleanup();
  });

  it("links to each device's session with its crash count, and opens it", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "list_debug_sessions") return [pixel, phone];
      if (command === "get_debug_session") throw { kind: "notFound", message: "gone" };
      throw new Error(`unexpected command ${command}`);
    });
    render(() => (
      <>
        <BuildSessionLinks record={record} />
        <SessionsDialog />
      </>
    ));

    const link = await screen.findByRole("button", { name: "Session: 2 crashes on Pixel_7" });
    expect(screen.getByRole("button", { name: "Session: no crashes on SM-G991B" })).toBeTruthy();
    fireEvent.click(link);

    await screen.findByRole("dialog", { name: "Debug Sessions" });
    const selected = await screen.findAllByRole("option", { selected: true });
    expect(selected[0].textContent).toContain("2 crashes");
  });
});
