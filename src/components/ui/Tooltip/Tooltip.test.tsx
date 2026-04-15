import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { Tooltip } from "./Tooltip";

describe("Tooltip", () => {
  afterEach(() => vi.useRealTimers());

  it("does not show tooltip before delay", () => {
    vi.useFakeTimers();
    const { container } = render(() => (
      <Tooltip content="Helpful info">
        <button>Hover me</button>
      </Tooltip>
    ));
    fireEvent.mouseEnter(container.firstElementChild as HTMLElement);
    expect(container.querySelector('[role="tooltip"]')).toBeNull();
  });

  it("shows tooltip after delay", () => {
    vi.useFakeTimers();
    const { container } = render(() => (
      <Tooltip content="Helpful info" delay={300}>
        <button>Hover me</button>
      </Tooltip>
    ));
    fireEvent.mouseEnter(container.firstElementChild as HTMLElement);
    vi.advanceTimersByTime(300);
    expect(container.querySelector('[role="tooltip"]')).not.toBeNull();
    expect(container.querySelector('[role="tooltip"]')!.textContent).toBe("Helpful info");
  });

  it("hides tooltip on mouse leave", () => {
    vi.useFakeTimers();
    const { container } = render(() => (
      <Tooltip content="Info" delay={0}>
        <button>Hover</button>
      </Tooltip>
    ));
    const wrapper = container.firstElementChild as HTMLElement;
    fireEvent.mouseEnter(wrapper);
    vi.runAllTimers();
    expect(container.querySelector('[role="tooltip"]')).not.toBeNull();
    fireEvent.mouseLeave(wrapper);
    expect(container.querySelector('[role="tooltip"]')).toBeNull();
  });

  it("does not show tooltip when disabled", () => {
    vi.useFakeTimers();
    const { container } = render(() => (
      <Tooltip content="Info" disabled delay={0}>
        <button>Hover</button>
      </Tooltip>
    ));
    fireEvent.mouseEnter(container.firstElementChild as HTMLElement);
    vi.runAllTimers();
    expect(container.querySelector('[role="tooltip"]')).toBeNull();
  });
});
