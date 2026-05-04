import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { resetUIStateForTests, uiState } from "@/stores/ui.store";
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
});
