import { describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import { FilterChip } from "./FilterChip";

describe("FilterChip", () => {
  it("renders an inactive chip", () => {
    const { getByRole } = render(() => <FilterChip onClick={() => {}}>Age</FilterChip>);
    const chip = getByRole("button");
    expect(chip.textContent).toBe("Age");
    expect(chip.getAttribute("aria-pressed")).toBe("false");
  });

  it("marks active chips as pressed", () => {
    const { getByRole } = render(() => (
      <FilterChip active onClick={() => {}}>
        30s
      </FilterChip>
    ));
    expect(getByRole("button").getAttribute("aria-pressed")).toBe("true");
  });

  it("calls onClick", () => {
    const onClick = vi.fn();
    const { getByRole } = render(() => <FilterChip onClick={onClick}>All</FilterChip>);
    fireEvent.click(getByRole("button"));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
