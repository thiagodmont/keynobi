import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resetUIStateForTests, uiState } from "@/stores/ui.store";
import { resetBuildState, setDeployPhase, startBuild } from "@/stores/build.store";
import { setProject, setProjectState } from "@/stores/project.store";
import { setProjects } from "@/stores/projects.store";
import {
  resetRunConfigurationsForTests,
  setRunConfigurations,
} from "@/stores/run-configurations.store";
import { makeProjectRunConfigurations, makeRunConfiguration } from "@/test/factories/build";
import type { ProjectEntry } from "@/bindings";
import { TitleBar } from "./TitleBar";

describe("TitleBar", () => {
  const appWindow = {
    startDragging: vi.fn().mockResolvedValue(undefined),
    isAlwaysOnTop: vi.fn().mockResolvedValue(false),
    setAlwaysOnTop: vi.fn().mockResolvedValue(undefined),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    resetUIStateForTests();
    resetBuildState();
    appWindow.isAlwaysOnTop.mockResolvedValue(false);
    appWindow.setAlwaysOnTop.mockResolvedValue(undefined);
    vi.mocked(getCurrentWindow).mockReturnValue(
      appWindow as unknown as ReturnType<typeof getCurrentWindow>
    );
  });

  it("toggles the current window always-on-top state from the title bar", async () => {
    render(() => <TitleBar />);

    const button = screen.getByRole("button", { name: /on top/i });
    await waitFor(() => expect(appWindow.isAlwaysOnTop).toHaveBeenCalled());

    expect(button.textContent).toContain("On Top");
    expect(button.getAttribute("title")).toBe("Keep window on top");

    fireEvent.click(button);

    await waitFor(() => expect(appWindow.setAlwaysOnTop).toHaveBeenCalledWith(true));
    expect(button.getAttribute("aria-pressed")).toBe("true");
  });

  it("does not let a stale initial state read overwrite a user toggle", async () => {
    let resolveInitialState: (value: boolean) => void = () => {};
    appWindow.isAlwaysOnTop.mockReturnValueOnce(
      new Promise<boolean>((resolve) => {
        resolveInitialState = resolve;
      })
    );

    render(() => <TitleBar />);

    const button = screen.getByRole("button", { name: /on top/i });
    fireEvent.click(button);

    await waitFor(() => expect(appWindow.setAlwaysOnTop).toHaveBeenCalledWith(true));
    expect(button.getAttribute("aria-pressed")).toBe("true");

    resolveInitialState(false);

    await Promise.resolve();
    await Promise.resolve();

    expect(button.getAttribute("aria-pressed")).toBe("true");
  });

  it("applies a delayed initial state read after a failed user toggle", async () => {
    let resolveInitialState: (value: boolean) => void = () => {};
    appWindow.isAlwaysOnTop.mockReturnValueOnce(
      new Promise<boolean>((resolve) => {
        resolveInitialState = resolve;
      })
    );
    appWindow.setAlwaysOnTop.mockRejectedValueOnce(new Error("pin failed"));

    render(() => <TitleBar />);

    const button = screen.getByRole("button", { name: /on top/i });
    fireEvent.click(button);

    await waitFor(() => expect(appWindow.setAlwaysOnTop).toHaveBeenCalledWith(true));
    expect(button.getAttribute("aria-pressed")).toBe(null);

    resolveInitialState(true);

    await waitFor(() => expect(button.getAttribute("aria-pressed")).toBe("true"));
  });

  it("toggles Log Mode from the title bar", () => {
    render(() => <TitleBar />);

    const button = screen.getByRole("button", { name: /log mode/i });
    expect(uiState.logMode.active).toBe(false);
    expect(button.getAttribute("title")).toBe("Enter Log Mode");

    fireEvent.click(button);

    expect(uiState.logMode.active).toBe(true);
    expect(button.getAttribute("aria-pressed")).toBe("true");
    expect(button.getAttribute("title")).toBe("Exit Log Mode");
  });

  it("exits Log Mode when the active Log Mode button is clicked again", () => {
    render(() => <TitleBar />);

    const button = screen.getByRole("button", { name: /log mode/i });
    fireEvent.click(button);
    expect(uiState.logMode.active).toBe(true);

    fireEvent.click(button);

    expect(uiState.logMode.active).toBe(false);
    expect(button.getAttribute("aria-pressed")).toBe(null);
  });
  it("offers Cancel while Gradle builds, but not during install and launch", () => {
    render(() => <TitleBar />);
    const button = screen.getByRole("button", { name: /^build$/i });

    startBuild("assembleDebug");
    setDeployPhase("building");
    expect(button.getAttribute("title")).toBe("Cancel build");
    expect(button.hasAttribute("disabled")).toBe(false);

    // The build succeeded; install cannot be cancelled, so Cancel must not show.
    resetBuildState();
    setDeployPhase("installing");
    expect(button.getAttribute("title")).toBe("Installing APK…");
    expect(button.hasAttribute("disabled")).toBe(true);

    fireEvent.click(button);
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "cancel_build")).toHaveLength(0);
  });

  it("names the agent whose build Cancel stops, and cancels it", () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    render(() => <TitleBar />);
    const button = screen.getByRole("button", { name: /^build$/i });

    startBuild("assembleDebug", {
      kind: "agent",
      sessionId: 2,
      clientName: "Codex",
      standalone: false,
    });
    expect(button.getAttribute("title")).toBe("Cancel the build started by an agent (Codex)");
    expect(button.hasAttribute("disabled")).toBe(false);

    fireEvent.click(button);
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "cancel_build")).toHaveLength(1);
  });

  describe("Safe Mode", () => {
    function openProject(trusted: boolean | null): void {
      const entry: ProjectEntry = {
        id: "p1",
        path: "/projects/app",
        name: "app",
        gradleRoot: "/projects/app",
        lastOpened: "2026-01-01T00:00:00Z",
        pinned: false,
        lastBuildVariant: null,
        lastDevice: null,
        trusted,
      };
      setProject(entry.path, entry.name);
      setProjects([entry]);
    }

    afterEach(() => {
      setProjects([]);
      setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
    });

    it("disables Run and shows a Safe Mode badge for an untrusted project", () => {
      openProject(false);
      render(() => <TitleBar />);

      const run = screen.getByTitle("Safe Mode — trust this project to build") as HTMLButtonElement;
      expect(run.disabled).toBe(true);
      expect(screen.getByRole("button", { name: "Safe Mode" })).not.toBeNull();
    });

    it("shows the run configuration picker left of Run once the project's configurations load", () => {
      openProject(true);
      render(() => <TitleBar />);
      expect(screen.queryByRole("combobox", { name: "Run configuration" })).toBeNull();

      setRunConfigurations(
        "/projects/app",
        makeProjectRunConfigurations([makeRunConfiguration({ name: "Wear" })])
      );

      const picker = screen.getByRole("combobox", { name: "Run configuration" });
      const run = screen.getByTitle(/Run App/);
      expect((picker as HTMLSelectElement).value).toBe("Wear");
      expect(
        picker.compareDocumentPosition(run) & globalThis.Node.DOCUMENT_POSITION_FOLLOWING
      ).toBeTruthy();
      resetRunConfigurationsForTests();
    });

    it("enables Run without a badge for a trusted project", () => {
      openProject(true);
      render(() => <TitleBar />);

      const run = screen.getByTitle(/Run App/) as HTMLButtonElement;
      expect(run.disabled).toBe(false);
      expect(screen.queryByRole("button", { name: "Safe Mode" })).toBeNull();
    });
  });
});
