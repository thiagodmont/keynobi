import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@solidjs/testing-library";
import { AvdContextMenu } from "./AvdContextMenu";

describe("AvdContextMenu", () => {
  function renderMenu(onWipe = vi.fn(), onDelete = vi.fn()) {
    render(() => (
      <AvdContextMenu
        trigger={<button type="button">···</button>}
        onWipe={onWipe}
        onDelete={onDelete}
      />
    ));
    // Open the dropdown
    fireEvent.click(screen.getByRole("button", { name: /···/ }));
  }

  it("calls onWipe when Wipe Data is clicked", () => {
    const onWipe = vi.fn();
    const onDelete = vi.fn();
    renderMenu(onWipe, onDelete);
    fireEvent.click(screen.getByRole("button", { name: /Wipe Data/i }));
    expect(onWipe).toHaveBeenCalledOnce();
    expect(onDelete).not.toHaveBeenCalled();
  });

  it("calls onDelete when Delete is clicked", () => {
    const onWipe = vi.fn();
    const onDelete = vi.fn();
    renderMenu(onWipe, onDelete);
    fireEvent.click(screen.getByRole("button", { name: /Delete/i }));
    expect(onDelete).toHaveBeenCalledOnce();
    expect(onWipe).not.toHaveBeenCalled();
  });

  it("closes menu after item click", () => {
    const onWipe = vi.fn();
    renderMenu(onWipe);
    fireEvent.click(screen.getByRole("button", { name: /Wipe Data/i }));
    expect(screen.queryByRole("button", { name: /Wipe Data/i })).toBeNull();
  });
});
