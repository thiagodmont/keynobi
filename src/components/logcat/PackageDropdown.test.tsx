import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { PackageDropdown } from "./PackageDropdown";

function renderPackageDropdown() {
  return render(() => (
    <PackageDropdown packages={["com.example.app"]} selected={null} onSelect={vi.fn()} />
  ));
}

describe("PackageDropdown", () => {
  it("closes when the window loses focus", () => {
    renderPackageDropdown();

    fireEvent.click(screen.getByTitle("Filter by package"));
    expect(screen.getByPlaceholderText("Search packages…")).not.toBeNull();

    window.dispatchEvent(new Event("blur"));

    expect(screen.queryByPlaceholderText("Search packages…")).toBeNull();
  });
});
