import { describe, it, expect, vi } from "vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import { Badge } from "./Badge";

describe("Badge", () => {
  it("renders children", () => {
    const { container } = render(() => <Badge>Connected</Badge>);
    expect(container.querySelector("span")!.textContent).toContain("Connected");
  });

  it("renders a dot element when dot prop is set", () => {
    const { container } = render(() => <Badge dot>Connected</Badge>);
    // Dot is a sibling span inside the badge
    const spans = container.querySelectorAll("span span");
    expect(spans.length).toBeGreaterThan(0);
  });

  it("does not render dot when dot prop is absent", () => {
    const { container } = render(() => <Badge>Connected</Badge>);
    const spans = container.querySelectorAll("span span");
    expect(spans.length).toBe(0);
  });

  it("passes class prop through", () => {
    const { container } = render(() => <Badge class="my-badge">x</Badge>);
    expect(container.querySelector("span")!.classList.contains("my-badge")).toBe(true);
  });

  it("supports compact mono badges", () => {
    const { container } = render(() => (
      <Badge size="xs" mono>
        JSON
      </Badge>
    ));
    const badge = container.querySelector("span")!;
    expect(badge.className).toContain("xs");
    expect(badge.className).toContain("mono");
  });

  it("renders clickable badges as buttons", () => {
    const onClick = vi.fn();
    const { container } = render(() => <Badge onClick={onClick}>JSON</Badge>);
    const button = container.querySelector("button")!;
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
