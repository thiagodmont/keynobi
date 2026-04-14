import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, cleanup } from "@solidjs/testing-library";
import { registerAction, unregisterAction } from "@/lib/action-registry";
import { CommandPalette, openPalette, closePalette } from "./CommandPalette";

describe("CommandPalette", () => {
  beforeEach(() => {
    closePalette();
    registerAction({ id: "test.open", label: "Open File", category: "File", shortcut: "Cmd+O", action: vi.fn() });
    registerAction({ id: "test.save", label: "Save All",  category: "File", action: vi.fn() });
  });

  afterEach(() => {
    unregisterAction("test.open");
    unregisterAction("test.save");
    cleanup();
  });

  it("does not render when closed", () => {
    render(() => <CommandPalette />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders dialog when openPalette() is called", () => {
    render(() => <CommandPalette />);
    openPalette();
    expect(screen.getByRole("dialog")).not.toBeNull();
  });

  it("closes on Escape", () => {
    render(() => <CommandPalette />);
    openPalette();
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes on backdrop click", () => {
    render(() => <CommandPalette />);
    openPalette();
    fireEvent.click(screen.getByTestId("palette-backdrop"));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows registered actions", () => {
    render(() => <CommandPalette />);
    openPalette();
    expect(screen.getByText("Open File")).not.toBeNull();
    expect(screen.getByText("Save All")).not.toBeNull();
  });

  it("filters results by query", () => {
    render(() => <CommandPalette />);
    openPalette();
    fireEvent.input(screen.getByRole("combobox"), { target: { value: "Save" } });
    expect(screen.getByText("Save All")).not.toBeNull();
    expect(screen.queryByText("Open File")).toBeNull();
  });

  it("shows empty state for unmatched query", () => {
    render(() => <CommandPalette />);
    openPalette();
    fireEvent.input(screen.getByRole("combobox"), { target: { value: "xyzzy-no-match-at-all" } });
    expect(screen.getByText("No commands found")).not.toBeNull();
  });

  it("ArrowDown moves selection to the next item", () => {
    render(() => <CommandPalette />);
    openPalette();
    const optionsBefore = screen.getAllByRole("option");
    expect(optionsBefore[0].getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "ArrowDown" });
    const optionsAfter = screen.getAllByRole("option");
    expect(optionsAfter[0].getAttribute("aria-selected")).toBe("false");
    expect(optionsAfter[1].getAttribute("aria-selected")).toBe("true");
  });
});
