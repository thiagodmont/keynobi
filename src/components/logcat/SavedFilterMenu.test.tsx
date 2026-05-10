import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SavedFilter } from "@/lib/logcat-filter-storage";
import { SavedFilterMenu } from "./SavedFilterMenu";

function renderSavedFilterMenu(
  overrides: {
    savedFilters?: SavedFilter[];
    onApplyQuery?: (query: string) => void;
    onDeleteSavedFilter?: (id: string) => void;
    onRenameSavedFilter?: (id: string, name: string) => void;
  } = {}
) {
  return render(() => (
    <SavedFilterMenu
      savedFilters={overrides.savedFilters ?? []}
      onApplyQuery={overrides.onApplyQuery ?? vi.fn()}
      onDeleteSavedFilter={overrides.onDeleteSavedFilter ?? vi.fn()}
      onRenameSavedFilter={overrides.onRenameSavedFilter ?? vi.fn()}
    />
  ));
}

function closestClassContaining(element: Element | null, classNamePart: string): Element | null {
  let current = element;
  while (current) {
    if (current.className.toString().includes(classNamePart)) return current;
    current = current.parentElement;
  }
  return null;
}

describe("SavedFilterMenu", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("opens the dropdown aligned to the left edge of the trigger", () => {
    renderSavedFilterMenu();

    fireEvent.click(screen.getByTitle("Filter presets"));

    const panel = closestClassContaining(screen.getByText("Quick Filters"), "panel");
    expect(panel?.className).toContain("alignLeft");
    expect(panel?.className).not.toContain("alignRight");
  });

  it("closes when the window loses focus", () => {
    renderSavedFilterMenu();

    fireEvent.click(screen.getByTitle("Filter presets"));
    expect(screen.getByText("Quick Filters")).not.toBeNull();

    window.dispatchEvent(new Event("blur"));

    expect(screen.queryByText("Quick Filters")).toBeNull();
  });

  it("applies a saved filter from the menu row", () => {
    const onApplyQuery = vi.fn();
    renderSavedFilterMenu({
      savedFilters: [{ id: "filter-1", name: "Errors", query: "level:error", createdAt: 1 }],
      onApplyQuery,
    });

    fireEvent.click(screen.getByTitle("Filter presets"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Errors" }));

    expect(onApplyQuery).toHaveBeenCalledWith("level:error ");
  });

  it("keeps saved-filter row actions separate from the apply row", () => {
    const onApplyQuery = vi.fn();
    renderSavedFilterMenu({
      savedFilters: [{ id: "filter-1", name: "Errors", query: "level:error", createdAt: 1 }],
      onApplyQuery,
    });

    fireEvent.click(screen.getByTitle("Filter presets"));
    const renameButton = screen.getByTitle("Rename");

    expect(renameButton.closest('[role="menuitem"]')).toBeNull();

    fireEvent.click(renameButton);
    expect(onApplyQuery).not.toHaveBeenCalled();
  });
});
