import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { LogcatFilterControls } from "./LogcatFilterControls";

function renderFilterControls(overrides: Partial<Parameters<typeof LogcatFilterControls>[0]> = {}) {
  const props: Parameters<typeof LogcatFilterControls>[0] = {
    query: "",
    knownTags: [],
    knownPackages: ["com.example.app"],
    hasAgeFilter: false,
    activeAge: null,
    activePackage: null,
    isFiltered: false,
    onQueryChange: vi.fn(),
    onAgeSelect: vi.fn(),
    onPackageSelect: vi.fn(),
    onClear: vi.fn(),
    renderSavedFilterMenu: () => <div data-testid="saved-filter-menu">Filters</div>,
    ...overrides,
  };

  return {
    ...render(() => <LogcatFilterControls {...props} />),
    props,
  };
}

describe("LogcatFilterControls", () => {
  it("renders the query row before the quick-filter row", () => {
    renderFilterControls();

    const queryRow = screen.getByTestId("logcat-filter-query-row");
    const quickRow = screen.getByTestId("logcat-filter-quick-row");
    const siblings = Array.from(queryRow.parentElement?.children ?? []);

    expect(siblings.indexOf(queryRow)).toBeLessThan(siblings.indexOf(quickRow));
  });

  it("renders saved filters in the quick-filter row", () => {
    renderFilterControls();

    const quickRow = screen.getByTestId("logcat-filter-quick-row");
    expect(quickRow.contains(screen.getByTestId("saved-filter-menu"))).toBe(true);
  });

  it("selects age quick filters", () => {
    const onAgeSelect = vi.fn();
    renderFilterControls({ onAgeSelect });

    fireEvent.click(screen.getByText("5m"));
    expect(onAgeSelect).toHaveBeenCalledWith("5m");

    fireEvent.click(screen.getByText("All"));
    expect(onAgeSelect).toHaveBeenCalledWith(null);
  });

  it("selects package filters from the package dropdown", () => {
    const onPackageSelect = vi.fn();
    renderFilterControls({ onPackageSelect });

    fireEvent.click(screen.getByTitle("Filter by package"));
    fireEvent.click(screen.getByText("com.example.app"));

    expect(onPackageSelect).toHaveBeenCalledWith("com.example.app");
  });

  it("clears active filters", () => {
    const onClear = vi.fn();
    renderFilterControls({ isFiltered: true, query: "level:error ", onClear });

    fireEvent.click(screen.getByTitle("Clear all filters"));

    expect(onClear).toHaveBeenCalledOnce();
  });
});
