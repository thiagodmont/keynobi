import { fireEvent, render, screen, cleanup } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { BuildPanel } from "./BuildPanel";
import {
  addBuildLine,
  cancelBuildState,
  flushPendingLines,
  resetBuildState,
  setBuildHistory,
  setBuildResult,
  startBuild,
} from "@/stores/build.store";
import { setProject, setProjectState } from "@/stores/project.store";
import { setProjects } from "@/stores/projects.store";
import { makeBuildError, makeBuildLine, makeBuildRecord } from "@/test/factories/build";
import type { BuildStatus, ProjectEntry } from "@/bindings";

function registerProject(trusted: boolean | null): void {
  const entry: ProjectEntry = {
    id: "p1",
    path: "/mock/android-project",
    name: "android-project",
    gradleRoot: "/mock/android-project",
    lastOpened: "2026-01-01T00:00:00Z",
    pinned: false,
    lastBuildVariant: null,
    lastDevice: null,
    trusted,
  };
  setProjects([entry]);
}

type LogResponder = (id: number) => Promise<unknown>;

function mockBuildLogEntries(respond: LogResponder): void {
  vi.mocked(invoke).mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_build_log_entries") return respond((args as { id: number }).id);
    return Promise.resolve(undefined);
  });
}

function selectBuild(task: string): void {
  const row = screen.getByTitle(task).closest<HTMLElement>('[role="option"]');
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
    registerProject(true);
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
    setProjects([]);
  });

  it("shows the load error instead of the empty-log message when loading a build's log fails", async () => {
    mockBuildLogEntries(() => Promise.reject("Failed to read build log: permission denied"));
    render(() => <BuildPanel />);

    selectBuild("assembleDebug");

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Couldn't load the log for this build");
    expect(alert.textContent).toContain("Failed to read build log: permission denied");
    expect(screen.queryByText("This build printed no output")).toBeNull();
  });

  it("says the build printed no output when its log loads empty", async () => {
    mockBuildLogEntries(() => Promise.resolve([]));
    render(() => <BuildPanel />);

    selectBuild("assembleDebug");

    expect(await screen.findByText("This build printed no output")).not.toBeNull();
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

describe("BuildPanel viewing a past build", () => {
  const agent = {
    kind: "agent" as const,
    sessionId: 1,
    clientName: "Claude Code",
    standalone: false,
  };

  const pastFailure = makeBuildRecord({
    id: 5,
    task: "assembleRelease",
    status: {
      state: "failed",
      success: false,
      durationMs: BigInt(2500),
      errorCount: 1,
      warningCount: 0,
    } as BuildStatus,
    errors: [makeBuildError({ message: "Past error in Release.kt", file: "Release.kt" })],
    origin: agent,
  });

  function failLiveBuild(): void {
    startBuild("assembleDebug");
    addBuildLine(makeBuildLine({ kind: "error", content: "Live error", file: "Live.kt", line: 3 }));
    flushPendingLines();
    setBuildResult({ success: false, durationMs: 1500 });
  }

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
    registerProject(true);
    mockBuildLogEntries(() => Promise.resolve([makeBuildLine()]));
  });

  afterEach(() => {
    cleanup();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    resetBuildState();
    setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
    setProjects([]);
  });

  it("shows the past build's problems, status, and who started it, not the live build's", async () => {
    failLiveBuild();
    setBuildHistory([pastFailure]);
    render(() => <BuildPanel />);
    expect(screen.getByText("Live error")).not.toBeNull();

    selectBuild("assembleRelease");

    expect(screen.getByText(/^Viewing build #5 from /)).not.toBeNull();
    expect(
      screen.getByText("Build failed in 2.5s — 1 error · Started by an agent (Claude Code)")
    ).not.toBeNull();
    expect(screen.getByRole("button", { name: "Problems (1)" })).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Problems (1)" }));
    expect(screen.getByText("Past error in Release.kt")).not.toBeNull();
    expect(screen.queryByText("Live error")).toBeNull();
  });

  it("goes back to the live build", async () => {
    failLiveBuild();
    setBuildHistory([pastFailure]);
    render(() => <BuildPanel />);
    selectBuild("assembleRelease");

    fireEvent.click(screen.getByRole("button", { name: "Back to current build" }));

    expect(screen.queryByTestId("build-history-banner")).toBeNull();
    expect(screen.getByText("Build failed in 1.5s — 1 error")).not.toBeNull();
    expect(screen.getByText("Live error")).not.toBeNull();
  });

  it("keeps the past build when an agent starts one, and offers to show it", () => {
    setBuildHistory([pastFailure]);
    render(() => <BuildPanel />);
    selectBuild("assembleRelease");

    startBuild("assembleDebug", agent);

    expect(screen.getByText(/^Viewing build #5 from /)).not.toBeNull();
    expect(screen.getByText("A build started by an agent (Claude Code) is running")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show running build" }));
    expect(screen.queryByTestId("build-history-banner")).toBeNull();
    expect(screen.getByText("Building… · Started by an agent (Claude Code)")).not.toBeNull();
  });

  it("says when rotation removed the log instead of reporting a failure", async () => {
    mockBuildLogEntries(() =>
      Promise.reject({ kind: "notFound", message: "The log of build #5 is no longer on disk" })
    );
    setBuildHistory([pastFailure]);
    render(() => <BuildPanel />);

    selectBuild("assembleRelease");

    expect(await screen.findByText("This build's log was removed")).not.toBeNull();
    expect(screen.getByText(/keeps build logs for 7 days/)).not.toBeNull();
    expect(screen.queryByText("Couldn't load the log for this build")).toBeNull();
    // The rest of the past build is still there.
    fireEvent.click(screen.getByRole("button", { name: "Problems (1)" }));
    expect(screen.getByText("Past error in Release.kt")).not.toBeNull();
  });

  it("shows that the log is loading", () => {
    mockBuildLogEntries(() => new Promise(() => {}));
    setBuildHistory([pastFailure]);
    render(() => <BuildPanel />);

    selectBuild("assembleRelease");

    expect(screen.getByText("Loading this build's log…")).not.toBeNull();
  });

  it("says when the viewed build left the history", () => {
    setBuildHistory([pastFailure]);
    render(() => <BuildPanel />);
    selectBuild("assembleRelease");

    setBuildHistory([makeBuildRecord({ id: 6, task: "assembleDebug" })]);

    expect(screen.getByText("Build #5 is no longer in the history")).not.toBeNull();
  });

  it("builds in the list are picked with the keyboard", () => {
    setBuildHistory([pastFailure, makeBuildRecord({ id: 6, task: "assembleDebug" })]);
    render(() => <BuildPanel />);
    const list = screen.getByRole("listbox", { name: "Builds" });
    const [newest, older] = screen.getAllByRole("option");
    expect(list.contains(newest)).toBe(true);
    expect(newest.tabIndex).toBe(0);

    newest.focus();
    fireEvent.keyDown(newest, { key: "ArrowDown" });
    expect(document.activeElement).toBe(older);
    fireEvent.keyDown(older, { key: "Enter" });

    expect(screen.getByText(/^Viewing build #5 from /)).not.toBeNull();
    expect(older.getAttribute("aria-selected")).toBe("true");
  });
});

describe("BuildPanel in Safe Mode", () => {
  beforeEach(() => {
    resetBuildState();
    setProject("/mock/android-project", "android-project");
  });

  afterEach(() => {
    cleanup();
    resetBuildState();
    setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
    setProjects([]);
  });

  for (const trusted of [false, null]) {
    it(`disables Run and Build and explains Safe Mode (trusted: ${trusted})`, () => {
      registerProject(trusted);
      render(() => <BuildPanel />);

      const buttons = screen.getAllByTitle("Safe Mode — trust this project to build");
      expect(buttons).toHaveLength(2);
      for (const button of buttons) expect((button as HTMLButtonElement).disabled).toBe(true);
      expect(screen.getByRole("alert").textContent).toContain("Safe Mode");
      expect(screen.getByRole("button", { name: "Trust Project…" })).not.toBeNull();
    });
  }

  it("enables Run and Build for a trusted project", () => {
    registerProject(true);
    render(() => <BuildPanel />);

    expect(screen.queryAllByTitle("Safe Mode — trust this project to build")).toHaveLength(0);
    const run = screen.getByTitle(/Run App/) as HTMLButtonElement;
    expect(run.disabled).toBe(false);
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("BuildPanel with a build an agent started", () => {
  const agent = {
    kind: "agent" as const,
    sessionId: 1,
    clientName: "Claude Code",
    standalone: false,
  };

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
    registerProject(true);
  });

  afterEach(() => {
    cleanup();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    resetBuildState();
    setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
    setProjects([]);
  });

  it("says who is building, disables Build, and offers Cancel", () => {
    startBuild("assembleDebug", agent);
    render(() => <BuildPanel />);

    expect(screen.getAllByText(/Started by an agent \(Claude Code\)/).length).toBeGreaterThan(0);
    const build = screen.getByTitle(
      "A build started by an agent (Claude Code) is running"
    ) as HTMLButtonElement;
    expect(build.disabled).toBe(true);

    const cancel = screen.getByTitle("Cancel the build started by an agent (Claude Code)");
    fireEvent.click(cancel);
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "cancel_build")).toHaveLength(1);
  });

  it("shows who cancelled an agent's build", () => {
    startBuild("assembleDebug", agent);
    cancelBuildState(agent);
    render(() => <BuildPanel />);

    expect(
      screen.getByText(
        "Build cancelled · Started by an agent (Claude Code) · Cancelled by an agent (Claude Code)"
      )
    ).not.toBeNull();
  });

  it("lists who started and cancelled past builds in the history", () => {
    setBuildHistory([
      makeBuildRecord({
        id: 2,
        task: "assembleRelease",
        status: { state: "cancelled" },
        origin: agent,
        cancelledBy: { kind: "appQuit" },
      }),
      makeBuildRecord({ id: 1, task: "assembleDebug" }),
    ]);
    render(() => <BuildPanel />);

    expect(screen.getByText("Started by an agent (Claude Code)")).not.toBeNull();
    expect(screen.getByText("Cancelled because Keynobi quit")).not.toBeNull();
    // A plain app build says nothing extra.
    expect(screen.queryByText("Started in Keynobi")).toBeNull();
  });
});
