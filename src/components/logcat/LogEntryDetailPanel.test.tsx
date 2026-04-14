import { describe, it, expect } from "vitest";
import { render, screen } from "@solidjs/testing-library";
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
      <LogEntryDetailPanel entry={ENTRY} onClose={() => { closed = true; }} />
    ));
    const closeBtn = screen.getAllByRole("button").find(
      (b) => b.getAttribute("title") === "Close"
    );
    expect(closeBtn).not.toBeUndefined();
    closeBtn!.click();
    expect(closed).toBe(true);
  });
});
