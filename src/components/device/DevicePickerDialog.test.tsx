import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import type { Device } from "@/bindings";
import { setAvds, setDevices } from "@/stores/device.store";
import { DevicePickerDialog, showDevicePicker } from "./DevicePickerDialog";

const phone: Device = {
  serial: "R58M",
  name: "SM-G991B",
  model: "Galaxy S21",
  deviceKind: "physical",
  connectionState: "online",
  apiLevel: 34,
  androidVersion: "14",
};

describe("DevicePickerDialog", () => {
  beforeEach(() => {
    setDevices([phone]);
    setAvds([]);
  });

  afterEach(() => {
    cleanup();
    setDevices([]);
  });

  function open(): { trigger: HTMLButtonElement; picked: Promise<string | null> } {
    render(() => (
      <>
        <button>Run App</button>
        <DevicePickerDialog />
      </>
    ));
    const trigger = screen.getByRole("button", { name: "Run App" }) as HTMLButtonElement;
    trigger.focus();
    const picked = showDevicePicker();
    return { trigger, picked };
  }

  it("is a modal dialog that takes focus", () => {
    open();
    const dialog = screen.getByRole("dialog", { name: "Select a Device" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("Escape cancels and returns focus", async () => {
    const { trigger, picked } = open();

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });

    expect(await picked).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("focuses the first device; activating it picks that device", async () => {
    const { picked } = open();
    const row = screen.getByRole("button", { name: /Galaxy S21/ });
    expect(document.activeElement).toBe(row);

    fireEvent.click(row);

    expect(await picked).toBe("R58M");
  });
});
