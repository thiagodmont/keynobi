import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Input } from "./Input";

describe("Input", () => {
  it("renders an input element", () => {
    const { container } = render(() => <Input />);
    expect(container.querySelector("input")).not.toBeNull();
  });

  it("calls onInput with the current value", () => {
    const fn = vi.fn();
    render(() => <Input onInput={fn} />);
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "hello" } });
    expect(fn).toHaveBeenCalledWith("hello");
  });

  it("is disabled when disabled prop is set", () => {
    const { container } = render(() => <Input disabled />);
    expect(container.querySelector("input")!.disabled).toBe(true);
  });

  it("is disabled when state=disabled", () => {
    const { container } = render(() => <Input state="disabled" />);
    expect(container.querySelector("input")!.disabled).toBe(true);
  });

  it("sets aria-invalid when state=error", () => {
    const { container } = render(() => <Input state="error" />);
    expect(container.querySelector("input")!.getAttribute("aria-invalid")).toBe("true");
  });

  it("renders prefix slot", () => {
    const { container } = render(() => (
      <Input prefix={<span data-testid="pfx" />} />
    ));
    expect(container.querySelector("[data-testid='pfx']")).not.toBeNull();
  });

  it("renders suffix slot", () => {
    const { container } = render(() => (
      <Input suffix={<span data-testid="sfx" />} />
    ));
    expect(container.querySelector("[data-testid='sfx']")).not.toBeNull();
  });

  it("renders clear button when clearable and value is non-empty", () => {
    const { container } = render(() => (
      <Input clearable value="hello" onClear={vi.fn()} />
    ));
    expect(container.querySelector("button")).not.toBeNull();
  });

  it("does not render clear button when value is empty", () => {
    const { container } = render(() => (
      <Input clearable value="" onClear={vi.fn()} />
    ));
    expect(container.querySelector("button")).toBeNull();
  });

  it("calls onClear when clear button is clicked", () => {
    const fn = vi.fn();
    const { container } = render(() => (
      <Input clearable value="hello" onClear={fn} />
    ));
    fireEvent.click(container.querySelector("button")!);
    expect(fn).toHaveBeenCalledOnce();
  });

  it("passes class prop through to wrapper", () => {
    const { container } = render(() => <Input class="my-input" />);
    expect(container.firstElementChild!.classList.contains("my-input")).toBe(true);
  });
});
