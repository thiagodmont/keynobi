import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { MenuEmptyState, MenuList, MenuListItem, MenuSectionHeader } from "./MenuList";

describe("MenuList", () => {
  it("renders section headers, items, and empty states", () => {
    render(() => (
      <MenuList>
        <MenuSectionHeader label="Filters" end={<span>1 / 10</span>} />
        <MenuListItem mono>level:error</MenuListItem>
        <MenuEmptyState>No saved filters</MenuEmptyState>
      </MenuList>
    ));

    expect(screen.getByText("Filters")).not.toBeNull();
    expect(screen.getByText("1 / 10")).not.toBeNull();
    expect(screen.getByText("level:error")).not.toBeNull();
    expect(screen.getByText("No saved filters")).not.toBeNull();
  });

  it("calls item click handlers", () => {
    const onClick = vi.fn();
    render(() => (
      <MenuList>
        <MenuListItem onClick={onClick}>Run</MenuListItem>
      </MenuList>
    ));

    fireEvent.click(screen.getByText("Run"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("defaults clickable items to menuitem role", () => {
    render(() => (
      <MenuList>
        <MenuListItem onClick={() => undefined}>Run</MenuListItem>
      </MenuList>
    ));

    expect(screen.getByRole("menuitem", { name: "Run" })).not.toBeNull();
  });

  it("preserves explicit item roles", () => {
    render(() => (
      <MenuList>
        <MenuListItem role="option" onClick={() => undefined}>
          Run
        </MenuListItem>
      </MenuList>
    ));

    expect(screen.getByRole("option", { name: "Run" })).not.toBeNull();
  });
});
