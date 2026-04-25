import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { LogViewerToolbar, activeToolbarButtonBackground } from "./LogViewerToolbar";

const noop = vi.fn();

function renderToolbar(overrides: Partial<Parameters<typeof LogViewerToolbar>[0]> = {}) {
  return render(() => (
    <LogViewerToolbar
      levelFilter="all"
      onLevelFilterChange={noop}
      sourceFilter="all"
      onSourceFilterChange={noop}
      sources={[]}
      rawSearch=""
      onRawSearchChange={noop}
      countText="0"
      countTitle="0 logs"
      showTimestamps={true}
      onToggleTimestamps={noop}
      autoScroll={true}
      onToggleAutoScroll={noop}
      copiedAll={false}
      onCopyAll={noop}
      canClear={false}
      onClear={noop}
      {...overrides}
    />
  ));
}

describe("LogViewerToolbar", () => {
  it("uses a valid color function for active colored buttons", () => {
    expect(activeToolbarButtonBackground(true, "var(--error)")).toBe(
      "color-mix(in srgb, var(--error) 14%, transparent)"
    );
  });

  it("keeps the source selector visible while a source filter is active", () => {
    renderToolbar({
      sourceFilter: "lsp:server",
      sources: ["lsp:server"],
    });

    expect(screen.getByTitle("Filter by log source")).not.toBeNull();
  });
});
