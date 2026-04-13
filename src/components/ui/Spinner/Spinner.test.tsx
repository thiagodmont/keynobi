import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { Spinner } from "./Spinner";

describe("Spinner", () => {
  it("renders a status element", () => {
    const { container } = render(() => <Spinner />);
    expect(container.querySelector('[role="status"]')).not.toBeNull();
  });

  it("has accessible label", () => {
    const { container } = render(() => <Spinner />);
    const el = container.querySelector('[role="status"]')!;
    expect(el.getAttribute("aria-label")).toBe("Loading");
  });

  it("accepts a class prop", () => {
    const { container } = render(() => <Spinner class="my-spinner" />);
    const el = container.querySelector('[role="status"]')!;
    expect(el.classList.contains("my-spinner")).toBe(true);
  });
});
