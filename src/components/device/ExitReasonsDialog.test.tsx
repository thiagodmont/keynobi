import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import type { AppExitReasons, Device } from "@/bindings";
import { resetDeviceState, setDevices } from "@/stores/device.store";
import { mockExitReasons } from "@/test/mock-backend/devices";
import {
  ExitReasonsDialog,
  closeExitReasonsDialog,
  openExitReasonsDialog,
} from "./ExitReasonsDialog";

const emulator: Device = {
  serial: "emulator-5554",
  name: "Pixel 8 API 35",
  model: "Pixel 8",
  deviceKind: "emulator",
  connectionState: "online",
  apiLevel: 35,
  androidVersion: "15",
};

function stubExitReasons(respond: (args: unknown) => Promise<AppExitReasons>) {
  vi.mocked(invoke).mockImplementation((command: string, args?: unknown) => {
    if (command === "get_exit_reasons") return respond(args);
    return Promise.reject(new Error(`unexpected command ${command}`));
  });
}

function exitReasonCalls() {
  return vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_exit_reasons");
}

describe("ExitReasonsDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    resetDeviceState();
    setDevices([emulator]);
  });

  afterEach(() => {
    closeExitReasonsDialog();
    cleanup();
    resetDeviceState();
  });

  it("lists the project app's exits for the selected device, newest first", async () => {
    stubExitReasons(() => Promise.resolve(mockExitReasons("emulator-5554", null)));
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();

    const list = await screen.findByRole("list", { name: "Process exits, newest first" });
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(4);
    expect(items[0].textContent).toContain("Crash");
    expect(items[0].textContent).toContain("2026-09-25 10:15:03.482");
    expect(items[1].textContent).toContain("ANR");
    expect(items[1].textContent).toContain("Input dispatching timed out");
    expect(items[2].textContent).toContain("Low memory");
    expect(
      screen.getByText(/com\.example\.mockapp\.debug on Pixel 8 API 35 · API 34/)
    ).toBeTruthy();
    expect(screen.getByText(/Stack traces are in Logcat only if/)).toBeTruthy();
    expect(exitReasonCalls()).toEqual([
      ["get_exit_reasons", { serial: "emulator-5554", package: null }],
    ]);
  });

  it("reads the package the user typed", async () => {
    stubExitReasons(() => Promise.resolve(mockExitReasons("emulator-5554", "com.other.app")));
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();
    await screen.findByRole("list", { name: "Process exits, newest first" });

    const input = screen.getByLabelText("Package");
    fireEvent.input(input, { target: { value: " com.other.app " } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(exitReasonCalls()).toHaveLength(2));
    expect(exitReasonCalls()[1][1]).toEqual({ serial: "emulator-5554", package: "com.other.app" });
  });

  it("says when the device is too old instead of listing nothing", async () => {
    stubExitReasons(() =>
      Promise.resolve({
        serial: "emulator-5554",
        package: "com.example.app",
        apiLevel: 29,
        supported: false,
        message:
          "Process exit reasons need Android 11 (API 30) or later; emulator-5554 runs API 29.",
        records: [],
        totalRecords: 0,
      })
    );
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Not available on this device");
    expect(alert.textContent).toContain("runs API 29");
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("shows an empty history as empty", async () => {
    stubExitReasons(() =>
      Promise.resolve({
        serial: "emulator-5554",
        package: "com.example.app",
        apiLevel: 34,
        supported: true,
        message: "No process exits are recorded for com.example.app on emulator-5554.",
        records: [],
        totalRecords: 0,
      })
    );
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();

    expect(await screen.findByText("No exits recorded")).toBeTruthy();
    expect(screen.getByText(/No process exits are recorded for com\.example\.app/)).toBeTruthy();
  });

  it("shows why the history could not be read", async () => {
    stubExitReasons(() =>
      Promise.reject({
        kind: "invalidInput",
        message:
          "The open project has several application ids (com.example.a, com.example.b): pass the package to read.",
      })
    );
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Could not read exit reasons");
    expect(alert.textContent).toContain("several application ids");
  });

  it("asks for a device when none is online, without calling the backend", async () => {
    resetDeviceState();
    stubExitReasons(() => Promise.resolve(mockExitReasons("emulator-5554", null)));
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();

    expect(await screen.findByText("No device selected")).toBeTruthy();
    expect(exitReasonCalls()).toHaveLength(0);
  });

  it("is a modal dialog that closes on Escape and ignores a late response", async () => {
    const answers: ((value: AppExitReasons) => void)[] = [];
    stubExitReasons(() => new Promise((resolve) => answers.push(resolve)));
    render(() => <ExitReasonsDialog />);
    openExitReasonsDialog();

    const dialog = await screen.findByRole("dialog", { name: "App Exit Reasons" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByText("Reading the exit history…")).toBeTruthy();

    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Reopened while the first request is still out: its answer is stale.
    openExitReasonsDialog();
    await screen.findByRole("dialog");
    await waitFor(() => expect(answers).toHaveLength(2));
    answers[0](mockExitReasons("emulator-5554", null));
    await Promise.resolve();
    await Promise.resolve();
    expect(screen.queryByRole("list")).toBeNull();
    expect(screen.getByText("Reading the exit history…")).toBeTruthy();
  });
});
