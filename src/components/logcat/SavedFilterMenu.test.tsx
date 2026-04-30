import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SavedFilterMenu } from "./SavedFilterMenu";

function renderSavedFilterMenu() {
  return render(() => (
    <SavedFilterMenu query="level:error " isFiltered={true} onApplyQuery={vi.fn()} />
  ));
}

describe("SavedFilterMenu", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("opens the dropdown to the right of the trigger when placed near the left edge", () => {
    renderSavedFilterMenu();

    fireEvent.click(screen.getByTitle("Filter presets"));

    const panel = screen.getByText("Quick Filters").parentElement?.parentElement;
    expect(panel?.getAttribute("style")).toContain("left: 0");
    expect(panel?.getAttribute("style")).not.toContain("right: 0");
  });
});
