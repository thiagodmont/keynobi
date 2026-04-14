import { describe, it, expect } from "vitest";
import { render } from "@solidjs/testing-library";
import { ScrollArea } from "./ScrollArea";

describe("ScrollArea", () => {
  it("renders children", () => {
    const { container } = render(() => (
      <ScrollArea><span data-testid="child">hello</span></ScrollArea>
    ));
    expect(container.querySelector("[data-testid='child']")).not.toBeNull();
  });

  it("defaults to overflow auto", () => {
    const { container } = render(() => <ScrollArea><div /></ScrollArea>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.style.overflowY).toBe("auto");
  });

  it("applies overflow=scroll", () => {
    const { container } = render(() => <ScrollArea overflow="scroll"><div /></ScrollArea>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.style.overflowY).toBe("scroll");
  });

  it("applies overflow=hidden", () => {
    const { container } = render(() => <ScrollArea overflow="hidden"><div /></ScrollArea>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.style.overflowY).toBe("hidden");
  });

  it("enables horizontal scrolling", () => {
    const { container } = render(() => <ScrollArea horizontal><div /></ScrollArea>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.style.overflowX).toBe("auto");
  });

  it("passes class prop through", () => {
    const { container } = render(() => <ScrollArea class="my-scroll"><div /></ScrollArea>);
    expect(container.firstElementChild!.classList.contains("my-scroll")).toBe(true);
  });
});
