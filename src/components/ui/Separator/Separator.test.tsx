import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { Separator } from "./Separator";

describe("Separator", () => {
  it("renders with role separator", () => {
    const { container } = render(() => <Separator />);
    expect(container.querySelector('[role="separator"]')).not.toBeNull();
  });

  it("defaults to horizontal orientation", () => {
    const { container } = render(() => <Separator />);
    expect(
      container.querySelector('[role="separator"]')!.getAttribute("aria-orientation")
    ).toBe("horizontal");
  });

  it("sets vertical aria-orientation", () => {
    const { container } = render(() => <Separator orientation="vertical" />);
    expect(
      container.querySelector('[role="separator"]')!.getAttribute("aria-orientation")
    ).toBe("vertical");
  });

  it("passes class prop through", () => {
    const { container } = render(() => <Separator class="my-sep" />);
    expect(
      container.querySelector('[role="separator"]')!.classList.contains("my-sep")
    ).toBe(true);
  });
});
