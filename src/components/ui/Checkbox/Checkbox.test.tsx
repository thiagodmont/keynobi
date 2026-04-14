import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("renders a checkbox input", () => {
    const { container } = render(() => (
      <Checkbox checked={false} onChange={vi.fn()} />
    ));
    expect(container.querySelector('input[type="checkbox"]')).not.toBeNull();
  });

  it("reflects checked=true state", () => {
    const { container } = render(() => (
      <Checkbox checked={true} onChange={vi.fn()} />
    ));
    expect((container.querySelector('input[type="checkbox"]') as HTMLInputElement).checked).toBe(true);
  });

  it("calls onChange(true) when clicked while unchecked", () => {
    const fn = vi.fn();
    const { container } = render(() => (
      <Checkbox checked={false} onChange={fn} />
    ));
    fireEvent.click(container.querySelector('input[type="checkbox"]')!);
    expect(fn).toHaveBeenCalledWith(true);
  });

  it("calls onChange(false) when clicked while checked", () => {
    const fn = vi.fn();
    const { container } = render(() => (
      <Checkbox checked={true} onChange={fn} />
    ));
    fireEvent.click(container.querySelector('input[type="checkbox"]')!);
    expect(fn).toHaveBeenCalledWith(false);
  });

  it("is disabled when disabled prop is set", () => {
    const { container } = render(() => (
      <Checkbox checked={false} onChange={vi.fn()} disabled />
    ));
    expect((container.querySelector('input[type="checkbox"]') as HTMLInputElement).disabled).toBe(true);
  });

  it("sets aria-checked=mixed when indeterminate", () => {
    const { container } = render(() => (
      <Checkbox checked={false} onChange={vi.fn()} indeterminate />
    ));
    expect(container.querySelector("input")!.getAttribute("aria-checked")).toBe("mixed");
  });

  it("renders label children", () => {
    render(() => (
      <Checkbox checked={false} onChange={vi.fn()}>My label</Checkbox>
    ));
    expect(screen.getByText("My label")).not.toBeNull();
  });

  it("passes class prop through to label", () => {
    const { container } = render(() => (
      <Checkbox checked={false} onChange={vi.fn()} class="my-cb" />
    ));
    expect(container.querySelector("label")!.classList.contains("my-cb")).toBe(true);
  });
});
