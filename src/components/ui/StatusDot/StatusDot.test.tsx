import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { StatusDot } from "./StatusDot";

describe("StatusDot", () => {
  it("renders with role img", () => {
    const { container } = render(() => <StatusDot status="ok" />);
    expect(container.querySelector('[role="img"]')).not.toBeNull();
  });

  it("has aria-label matching the status", () => {
    const { container } = render(() => <StatusDot status="error" />);
    expect(container.querySelector('[role="img"]')!.getAttribute("aria-label")).toBe("error");
  });

  it("passes class prop through", () => {
    const { container } = render(() => <StatusDot status="ok" class="my-dot" />);
    expect(container.querySelector('[role="img"]')!.classList.contains("my-dot")).toBe(true);
  });
});
