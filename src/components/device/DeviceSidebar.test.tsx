import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import type { AvdInfo, Device } from "@/bindings";
import { deviceState, setAvds, setDevices } from "@/stores/device.store";
import { DeviceSidebar } from "./DeviceSidebar";

const emulator: Device = {
  serial: "emulator-5554",
  name: "Pixel 8 API 35",
  model: "Pixel 8",
  deviceKind: "emulator",
  connectionState: "online",
  apiLevel: 35,
  androidVersion: "15",
};

const offlinePhone: Device = {
  serial: "R58M",
  name: "SM-G991B",
  model: "Galaxy S21",
  deviceKind: "physical",
  connectionState: "offline",
  apiLevel: 34,
  androidVersion: "14",
};

const avd: AvdInfo = {
  name: "Tablet_API_34",
  displayName: "Tablet API 34",
  target: "android-34",
  apiLevel: 34,
  abi: "arm64-v8a",
  path: "/avd/Tablet_API_34.avd",
};

function invokedCommands(): string[] {
  return vi.mocked(invoke).mock.calls.map(([cmd]) => cmd);
}

describe("DeviceSidebar from the keyboard", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "refresh_devices") return Promise.resolve([emulator, offlinePhone]);
      if (cmd === "list_avd_devices") return Promise.resolve([avd]);
      return Promise.resolve(undefined);
    });
    setDevices([emulator, offlinePhone]);
    setAvds([avd]);
  });

  afterEach(() => {
    cleanup();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue(undefined);
    setDevices([]);
    setAvds([]);
  });

  function option(name: string): HTMLElement {
    return screen.getAllByRole("option").find((o) => o.textContent?.includes(name))!;
  }

  it("selects an online device with Enter; an offline one is focusable but not selectable", () => {
    render(() => <DeviceSidebar />);
    expect(screen.getByRole("listbox", { name: "Connected devices" })).not.toBeNull();
    expect(option("Galaxy S21").getAttribute("aria-disabled")).toBe("true");

    fireEvent.keyDown(option("Galaxy S21"), { key: "Enter" });
    expect(deviceState.selectedSerial).not.toBe("R58M");

    fireEvent.keyDown(option("Pixel 8"), { key: "Enter" });
    expect(deviceState.selectedSerial).toBe("emulator-5554");
    expect(option("Pixel 8").getAttribute("aria-selected")).toBe("true");
    expect(invokedCommands()).toContain("select_device");
  });

  it("stops a running emulator from its row's menu", () => {
    render(() => <DeviceSidebar />);
    const row = option("Pixel 8");
    row.focus();

    fireEvent.keyDown(row, { key: "F10", shiftKey: true });
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Stop Emulator" }), { key: "Enter" });

    expect(invokedCommands()).toContain("stop_avd");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("a physical device has no row menu", () => {
    render(() => <DeviceSidebar />);
    fireEvent.keyDown(option("Galaxy S21"), { key: "F10", shiftKey: true });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("a virtual device's actions are in the tab order without hovering", () => {
    render(() => <DeviceSidebar />);
    const launch = screen.getByTitle("Launch Tablet API 34");
    const more = screen.getByRole("button", { name: "More options for Tablet API 34" });
    expect(launch.tabIndex).toBe(0);
    expect(more.tabIndex).toBe(0);
  });
});
