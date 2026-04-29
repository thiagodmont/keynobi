import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@solidjs/testing-library";
import { LogEntryDetailPanel } from "./LogEntryDetailPanel";
import type { LogcatEntry } from "@/lib/tauri-api";

const ENTRY = {
  id: 0n,
  timestamp: "2024-01-15 10:23:45.123",
  pid: 1234,
  tid: 5678,
  level: "error" as const,
  tag: "MainActivity",
  message: "NullPointerException at line 42",
  package: "com.example.app",
  kind: "normal" as const,
  isCrash: false,
  flags: 0,
  category: "general" as const,
  crashGroupId: null,
  jsonBody: null,
} satisfies LogcatEntry;

describe("LogEntryDetailPanel", () => {
  it("renders the tag", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(screen.getByText("MainActivity")).not.toBeNull();
  });

  it("renders the message", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(screen.getByText("NullPointerException at line 42")).not.toBeNull();
  });

  it("renders the package", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} />);
    expect(screen.getByText("com.example.app")).not.toBeNull();
  });

  it("calls onClose when close button is clicked", () => {
    let closed = false;
    render(() => (
      <LogEntryDetailPanel
        entry={ENTRY}
        onClose={() => {
          closed = true;
        }}
      />
    ));
    const closeBtn = screen.getAllByRole("button").find((b) => b.getAttribute("title") === "Close");
    expect(closeBtn).not.toBeUndefined();
    closeBtn!.click();
    expect(closed).toBe(true);
  });

  it("calls onClose when Escape is pressed", () => {
    let closed = false;
    render(() => (
      <LogEntryDetailPanel
        entry={ENTRY}
        onClose={() => {
          closed = true;
        }}
      />
    ));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(closed).toBe(true);
  });

  it("opens a floating filter menu when a metadata value is clicked", () => {
    render(() => <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={() => {}} />);

    fireEvent.click(screen.getByText("MainActivity"));

    expect(screen.getByRole("menu")).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Add as AND" })).not.toBeNull();
    expect(screen.getByRole("menuitem", { name: "Add as OR" })).not.toBeNull();
  });

  it("emits the clicked metadata token with the selected mode", () => {
    const onAddFilter = vi.fn();
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));

    fireEvent.click(screen.getByText("MainActivity"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as AND" }));

    expect(onAddFilter).toHaveBeenCalledWith({ token: "tag:MainActivity", mode: "and" });
  });

  it("emits OR filters for package values", () => {
    const onAddFilter = vi.fn();
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));

    fireEvent.click(screen.getByText("com.example.app"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({ token: "package:com.example.app", mode: "or" });
  });

  it("uses selected message text instead of the full message", () => {
    const onAddFilter = vi.fn();
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));
    const message = screen.getByText("NullPointerException at line 42");
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "line 42",
      anchorNode: message.firstChild,
      focusNode: message.firstChild,
    } as unknown as ReturnType<typeof window.getSelection>);

    fireEvent.click(message);
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({ token: 'message:"line 42"', mode: "or" });
  });

  it("ignores selected text outside the message field", () => {
    const onAddFilter = vi.fn();
    const outside = document.createTextNode("outside selection");
    document.body.appendChild(outside);
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "outside selection",
      anchorNode: outside,
      focusNode: outside,
    } as unknown as ReturnType<typeof window.getSelection>);

    fireEvent.click(screen.getByText("NullPointerException at line 42"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({
      token: 'message:"NullPointerException at line 42"',
      mode: "or",
    });
  });

  it("ignores selected text that crosses outside the message field", () => {
    const onAddFilter = vi.fn();
    const outside = document.createTextNode("outside selection");
    document.body.appendChild(outside);
    render(() => (
      <LogEntryDetailPanel entry={ENTRY} onClose={() => {}} onAddFilter={onAddFilter} />
    ));
    const message = screen.getByText("NullPointerException at line 42");
    vi.spyOn(window, "getSelection").mockReturnValue({
      toString: () => "line 42 plus outside selection",
      anchorNode: message.firstChild,
      focusNode: outside,
    } as unknown as ReturnType<typeof window.getSelection>);

    fireEvent.click(message);
    fireEvent.click(screen.getByRole("menuitem", { name: "Add as OR" }));

    expect(onAddFilter).toHaveBeenCalledWith({
      token: 'message:"NullPointerException at line 42"',
      mode: "or",
    });
  });
});
