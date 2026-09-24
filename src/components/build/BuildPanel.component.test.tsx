import { fireEvent, render, screen, cleanup } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { BuildPanel } from "./BuildPanel";
import { resetBuildState, setBuildHistory } from "@/stores/build.store";
import { setProject, setProjectState } from "@/stores/project.store";
import { makeBuildLine, makeBuildRecord } from "@/test/factories/build";

type LogResponder = (id: number) => Promise<unknown>;

function mockBuildLogEntries(respond: LogResponder): void {
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_build_log_entries") return respond((args as { id: number }).id);
    return Promise.resolve(undefined);
  });
}

function selectBuild(task: string): void {
  const row = screen.getByTitle(task).closest("button");
  if (!row) throw new Error(`No history row for ${task}`);
  fireEvent.click(row);
}

describe("BuildPanel historical log", () => {
  beforeEach(() => {
    if (!window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as any;
    }
    resetBuildState();
    setProject("/mock/android-project", "android-project");
    setBuildHistory([
      makeBuildRecord({ id: 2, task: "assembleRelease" }),
      makeBuildRecord({ id: 1, task: "assembleDebug" }),
    ]);
  });

  afterEach(() => {
    cleanup();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    resetBuildState();
    setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
  });

  it("shows the load error instead of 'No log saved' when loading a build's log fails", async () => {
    mockBuildLogEntries(() => Promise.reject("Failed to read build log: permission denied"));
    render(() => <BuildPanel />);

    selectBuild("assembleDebug");

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Couldn't load the log for this build");
    expect(alert.textContent).toContain("Failed to read build log: permission denied");
    expect(screen.queryByText("No log saved for this build")).toBeNull();
  });

  it("shows 'No log saved' when the build's log loads empty", async () => {
    mockBuildLogEntries(() => Promise.resolve([]));
    render(() => <BuildPanel />);

    selectBuild("assembleDebug");

    expect(await screen.findByText("No log saved for this build")).not.toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("clears the load error when switching to another build", async () => {
    mockBuildLogEntries((id) =>
      id === 1 ? Promise.reject("disk error") : Promise.resolve([makeBuildLine()])
    );
    render(() => <BuildPanel />);

    selectBuild("assembleDebug");
    await screen.findByRole("alert");

    selectBuild("assembleRelease");

    expect(await screen.findByText("> Task :app:assembleDebug")).not.toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("reloads the log when Retry is pressed", async () => {
    let attempts = 0;
    mockBuildLogEntries(() => {
      attempts += 1;
      return attempts === 1 ? Promise.reject("disk error") : Promise.resolve([makeBuildLine()]);
    });
    render(() => <BuildPanel />);

    selectBuild("assembleDebug");
    await screen.findByRole("alert");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));

    expect(await screen.findByText("> Task :app:assembleDebug")).not.toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(attempts).toBe(2);
  });
});
