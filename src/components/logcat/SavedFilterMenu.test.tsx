import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SavedFilterMenu } from "./SavedFilterMenu";

function renderSavedFilterMenu() {
  return render(() => (
    <SavedFilterMenu
      savedFilters={[]}
      onApplyQuery={vi.fn()}
      onDeleteSavedFilter={vi.fn()}
      onRenameSavedFilter={vi.fn()}
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
});
