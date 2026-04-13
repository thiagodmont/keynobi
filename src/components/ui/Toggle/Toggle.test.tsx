import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Toggle } from "./Toggle";

describe("Toggle", () => {
  it("renders with role switch", () => {
    render(() => <Toggle checked={false} onChange={vi.fn()} />);
    expect(screen.getByRole("switch")).not.toBeNull();
  });

  it("reflects checked=true via aria-checked", () => {
    render(() => <Toggle checked={true} onChange={vi.fn()} />);
    expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("true");
  });

  it("reflects checked=false via aria-checked", () => {
    render(() => <Toggle checked={false} onChange={vi.fn()} />);
    expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("false");
  });

  it("calls onChange(true) when clicked while unchecked", () => {
    const fn = vi.fn();
    render(() => <Toggle checked={false} onChange={fn} />);
    fireEvent.click(screen.getByRole("switch"));
    expect(fn).toHaveBeenCalledWith(true);
  });

  it("does not call onChange when disabled", () => {
    const fn = vi.fn();
    render(() => <Toggle checked={false} onChange={fn} disabled />);
    fireEvent.click(screen.getByRole("switch"));
    expect(fn).not.toHaveBeenCalled();
  });

  it("calls onChange on Space key", () => {
    const fn = vi.fn();
    render(() => <Toggle checked={false} onChange={fn} />);
    fireEvent.keyDown(screen.getByRole("switch"), { key: " " });
    expect(fn).toHaveBeenCalledWith(true);
  });

  it("calls onChange on Enter key", () => {
    const fn = vi.fn();
    render(() => <Toggle checked={false} onChange={fn} />);
    fireEvent.keyDown(screen.getByRole("switch"), { key: "Enter" });
    expect(fn).toHaveBeenCalledWith(true);
  });

  it("passes class prop through", () => {
    render(() => <Toggle checked={false} onChange={vi.fn()} class="my-toggle" />);
    expect(screen.getByRole("switch").classList.contains("my-toggle")).toBe(true);
  });
});
