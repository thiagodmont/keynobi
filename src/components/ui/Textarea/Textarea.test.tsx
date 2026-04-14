import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Textarea } from "./Textarea";

describe("Textarea", () => {
  it("renders a textarea element", () => {
    const { container } = render(() => <Textarea />);
    expect(container.querySelector("textarea")).not.toBeNull();
  });

  it("calls onInput with current value", () => {
    const fn = vi.fn();
    render(() => <Textarea onInput={fn} />);
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "hello" } });
    expect(fn).toHaveBeenCalledWith("hello");
  });

  it("is disabled when disabled prop is set", () => {
    const { container } = render(() => <Textarea disabled />);
    expect(container.querySelector("textarea")!.disabled).toBe(true);
  });

  it("is disabled when state=disabled", () => {
    const { container } = render(() => <Textarea state="disabled" />);
    expect(container.querySelector("textarea")!.disabled).toBe(true);
  });

  it("sets aria-invalid when state=error", () => {
    const { container } = render(() => <Textarea state="error" />);
    expect(container.querySelector("textarea")!.getAttribute("aria-invalid")).toBe("true");
  });

  it("applies rows attribute", () => {
    const { container } = render(() => <Textarea rows={5} />);
    expect(container.querySelector("textarea")!.rows).toBe(5);
  });

  it("passes class prop through", () => {
    const { container } = render(() => <Textarea class="my-ta" />);
    expect(container.querySelector("textarea")!.classList.contains("my-ta")).toBe(true);
  });
});
