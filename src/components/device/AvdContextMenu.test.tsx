import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@solidjs/testing-library";
import { AvdContextMenu } from "./AvdContextMenu";

describe("AvdContextMenu", () => {
  it("calls onWipe when Wipe Data is clicked", () => {
    const onClose = vi.fn();
    const onWipe = vi.fn();
    const onDelete = vi.fn();
    render(() => (
      <AvdContextMenu onClose={onClose} onWipe={onWipe} onDelete={onDelete} />
    ));
    fireEvent.click(screen.getByRole("button", { name: /Wipe Data/i }));
    expect(onWipe).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
    expect(onDelete).not.toHaveBeenCalled();
  });

  it("calls onDelete when Delete is clicked", () => {
    const onClose = vi.fn();
    const onWipe = vi.fn();
    const onDelete = vi.fn();
    render(() => (
      <AvdContextMenu onClose={onClose} onWipe={onWipe} onDelete={onDelete} />
    ));
    fireEvent.click(screen.getByRole("button", { name: /Delete/i }));
    expect(onDelete).toHaveBeenCalledOnce();
    expect(onClose).not.toHaveBeenCalled();
    expect(onWipe).not.toHaveBeenCalled();
  });

  it("calls onClose when backdrop is clicked", () => {
    const onClose = vi.fn();
    const { container } = render(() => (
      <AvdContextMenu onClose={onClose} onWipe={vi.fn()} onDelete={vi.fn()} />
    ));
    const backdrop = container.querySelector('div[style*="fixed"]') as HTMLElement | null;
    expect(backdrop).not.toBeNull();
    fireEvent.click(backdrop!);
    expect(onClose).toHaveBeenCalledOnce();
  });
});
