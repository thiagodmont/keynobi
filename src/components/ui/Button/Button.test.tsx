import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { Button } from "./Button";

describe("Button", () => {
  it("renders children", () => {
    const { container } = render(() => <Button>Run build</Button>);
    expect(container.querySelector("button")!.textContent).toContain("Run build");
  });

  it("calls onClick when clicked", () => {
    const fn = vi.fn();
    const { container } = render(() => <Button onClick={fn}>Click</Button>);
    fireEvent.click(container.querySelector("button")!);
    expect(fn).toHaveBeenCalledOnce();
  });

  it("is disabled when disabled prop is set", () => {
    const { container } = render(() => <Button disabled>Click</Button>);
    expect(container.querySelector("button")!.disabled).toBe(true);
  });

  it("does not call onClick when disabled", () => {
    const fn = vi.fn();
    const { container } = render(() => <Button disabled onClick={fn}>Click</Button>);
    fireEvent.click(container.querySelector("button")!);
    expect(fn).not.toHaveBeenCalled();
  });

  it("is disabled and shows spinner when loading", () => {
    const { container } = render(() => <Button loading>Click</Button>);
    expect(container.querySelector("button")!.disabled).toBe(true);
    expect(container.querySelector('[role="status"]')).not.toBeNull();
  });

  it("defaults type to button", () => {
    const { container } = render(() => <Button>Click</Button>);
    expect(container.querySelector("button")!.type).toBe("button");
  });

  it("passes class prop through to the button element", () => {
    const { container } = render(() => <Button class="extra">Click</Button>);
    expect(container.querySelector("button")!.classList.contains("extra")).toBe(true);
  });
});
