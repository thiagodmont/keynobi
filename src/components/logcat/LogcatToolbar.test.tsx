import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { LogcatToolbar } from "./LogcatToolbar";

function renderToolbar(overrides: Partial<Parameters<typeof LogcatToolbar>[0]> = {}) {
  const props: Parameters<typeof LogcatToolbar>[0] = {
    streaming: true,
    paused: false,
    restarting: false,
    crashes: 0,
    selectedCount: 0,
    autoScroll: true,
    toolbarCount: { text: "0 rows", title: "No rows" },
    onStart: vi.fn(),
    onStop: vi.fn(),
    onTogglePaused: vi.fn(),
    onRestart: vi.fn(),
    onClear: vi.fn(),
    onJumpToLastCrash: vi.fn(),
    onJumpToPreviousCrash: vi.fn(),
    onJumpToNextCrash: vi.fn(),
    onCopySelectedRows: vi.fn(),
    onScrollToEnd: vi.fn(),
    onExport: vi.fn(),
    ...overrides,
  };

  return {
    ...render(() => <LogcatToolbar {...props} />),
    props,
  };
}

describe("LogcatToolbar", () => {
  it("shows copy-selected for a single selected row", () => {
    const onCopySelectedRows = vi.fn();
    renderToolbar({ selectedCount: 1, onCopySelectedRows });

    const button = screen.getByTitle("Copy selected row");
    expect(button.textContent).toContain("1 row");

    fireEvent.click(button);
    expect(onCopySelectedRows).toHaveBeenCalledOnce();
  });

  it("hides copy-selected when no rows are selected", () => {
    renderToolbar({ selectedCount: 0 });

    expect(screen.queryByTitle("Copy selected row")).toBeNull();
  });

  it("does not render saved filters in the main toolbar", () => {
    renderToolbar();

    expect(screen.queryByTestId("saved-filter-menu")).toBeNull();
  });
});
