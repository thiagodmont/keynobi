import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { ProgressBar } from "./ProgressBar";

describe("ProgressBar", () => {
  it("renders a progress element", () => {
    const { container } = render(() => <ProgressBar value={50} />);
    expect(container.querySelector("[role='progressbar']")).not.toBeNull();
  });

  it("sets aria-valuenow when value is provided", () => {
    const { container } = render(() => <ProgressBar value={75} />);
    expect(container.querySelector("[role='progressbar']")!.getAttribute("aria-valuenow")).toBe("75");
  });

  it("sets aria-valuemin=0 and aria-valuemax=100", () => {
    const { container } = render(() => <ProgressBar value={50} />);
    const el = container.querySelector("[role='progressbar']")!;
    expect(el.getAttribute("aria-valuemin")).toBe("0");
    expect(el.getAttribute("aria-valuemax")).toBe("100");
  });

  it("does not set aria-valuenow in indeterminate mode", () => {
    const { container } = render(() => <ProgressBar />);
    expect(container.querySelector("[role='progressbar']")!.getAttribute("aria-valuenow")).toBeNull();
  });

  it("clamps value between 0 and 100", () => {
    const { container } = render(() => <ProgressBar value={120} />);
    const bar = container.querySelector("[data-testid='fill']") as HTMLElement;
    expect(bar.style.width).toBe("100%");
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => <ProgressBar value={50} class="my-pb" />);
    expect(container.firstElementChild!.classList.contains("my-pb")).toBe(true);
  });
});
