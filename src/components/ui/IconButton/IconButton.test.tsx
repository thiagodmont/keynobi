import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { IconButton } from "./IconButton";

describe("IconButton", () => {
  it("renders children", () => {
    render(() => (
      <IconButton title="Close" onClick={vi.fn()}>
        X
      </IconButton>
    ));
    expect(screen.getByText("X")).not.toBeNull();
  });

  it("has title attribute", () => {
    render(() => (
      <IconButton title="Refresh" onClick={vi.fn()}>
        R
      </IconButton>
    ));
    expect(screen.getByTitle("Refresh")).not.toBeNull();
  });

  it("calls onClick", () => {
    const fn = vi.fn();
    render(() => (
      <IconButton title="Go" onClick={fn}>
        G
      </IconButton>
    ));
    fireEvent.click(screen.getByTitle("Go"));
    expect(fn).toHaveBeenCalledOnce();
  });

  it("does not call onClick when disabled", () => {
    const fn = vi.fn();
    render(() => (
      <IconButton title="Go" onClick={fn} disabled>
        G
      </IconButton>
    ));
    fireEvent.click(screen.getByTitle("Go"));
    expect(fn).not.toHaveBeenCalled();
  });

  it("sets aria-pressed when active", () => {
    render(() => (
      <IconButton title="Toggle" onClick={vi.fn()} active>
        T
      </IconButton>
    ));
    expect(screen.getByTitle("Toggle").getAttribute("aria-pressed")).toBe("true");
  });

  it("has no aria-pressed when inactive", () => {
    render(() => (
      <IconButton title="Toggle" onClick={vi.fn()}>
        T
      </IconButton>
    ));
    expect(screen.getByTitle("Toggle").getAttribute("aria-pressed")).toBeNull();
  });
});
