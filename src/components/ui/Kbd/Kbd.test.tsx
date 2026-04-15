import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { Kbd } from "./Kbd";

describe("Kbd", () => {
  it("renders a kbd element", () => {
    const { container } = render(() => <Kbd>⌘K</Kbd>);
    expect(container.querySelector("kbd")).not.toBeNull();
  });

  it("renders children text", () => {
    const { container } = render(() => <Kbd>⌘K</Kbd>);
    expect(container.querySelector("kbd")!.textContent).toBe("⌘K");
  });

  it("passes class prop through", () => {
    const { container } = render(() => <Kbd class="my-kbd">P</Kbd>);
    expect(container.querySelector("kbd")!.classList.contains("my-kbd")).toBe(true);
  });
});
