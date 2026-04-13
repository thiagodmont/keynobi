import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
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
});
