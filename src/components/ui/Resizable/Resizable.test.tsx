import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { Resizable } from "./Resizable";

describe("Resizable", () => {
  it("calls onResize with positive delta on horizontal drag", () => {
    const onResize = vi.fn();
    const { container } = render(() => (
      <Resizable direction="horizontal" onResize={onResize} />
    ));
    const handle = container.firstElementChild as HTMLElement;

    fireEvent.mouseDown(handle, { clientX: 100 });
    fireEvent.mouseMove(document, { clientX: 120 });
    fireEvent.mouseUp(document);

    expect(onResize).toHaveBeenCalledWith(20);
  });

  it("calls onReset on double-click", () => {
    const onReset = vi.fn();
    const { container } = render(() => (
      <Resizable direction="horizontal" onResize={vi.fn()} onReset={onReset} />
    ));
    fireEvent.dblClick(container.firstElementChild as HTMLElement);
    expect(onReset).toHaveBeenCalledOnce();
  });

  it("does not call onReset when prop is omitted", () => {
    const { container } = render(() => (
      <Resizable direction="horizontal" onResize={vi.fn()} />
    ));
    // Should not throw
    fireEvent.dblClick(container.firstElementChild as HTMLElement);
  });
});
