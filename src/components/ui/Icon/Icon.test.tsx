import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { Icon } from "./Icon";

describe("Icon", () => {
  it("renders an SVG element", () => {
    const { container } = render(() => <Icon name="close" />);
    expect(container.querySelector("svg")).not.toBeNull();
  });

  it("applies custom size", () => {
    const { container } = render(() => <Icon name="close" size={24} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("width")).toBe("24");
    expect(svg.getAttribute("height")).toBe("24");
  });

  it("defaults to size 16", () => {
    const { container } = render(() => <Icon name="close" />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("width")).toBe("16");
  });

  it("renders a path element for unknown icon names", () => {
    const { container } = render(() => <Icon name="__nonexistent__" />);
    expect(container.querySelector("svg path")).not.toBeNull();
  });
});
