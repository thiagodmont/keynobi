import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Toolbar } from "./Toolbar";

const ITEMS = [
  { id: "run", label: "Run", onClick: vi.fn() },
  { id: "stop", label: "Stop", onClick: vi.fn(), disabled: true },
  { id: "clear", label: "Clear", onClick: vi.fn(), separator: true },
];

describe("Toolbar", () => {
  it("renders all item labels", () => {
    render(() => <Toolbar items={ITEMS} />);
    expect(screen.getByText("Run")).not.toBeNull();
    expect(screen.getByText("Stop")).not.toBeNull();
    expect(screen.getByText("Clear")).not.toBeNull();
  });

  it("calls onClick when item is clicked", () => {
    render(() => <Toolbar items={ITEMS} />);
    fireEvent.click(screen.getByText("Run"));
    expect(ITEMS[0].onClick).toHaveBeenCalledOnce();
  });

  it("does not call onClick when disabled item is clicked", () => {
    render(() => <Toolbar items={ITEMS} />);
    fireEvent.click(screen.getByText("Stop"));
    expect(ITEMS[1].onClick).not.toHaveBeenCalled();
  });

  it("disabled item has disabled attribute", () => {
    const { container } = render(() => <Toolbar items={ITEMS} />);
    const btns = container.querySelectorAll("button");
    const stopBtn = Array.from(btns).find((b) => b.textContent?.includes("Stop"));
    expect(stopBtn!.disabled).toBe(true);
  });

  it("renders separator before items with separator=true", () => {
    const { container } = render(() => <Toolbar items={ITEMS} />);
    // Separator has role='separator'
    expect(container.querySelector("[role='separator']")).not.toBeNull();
  });

  it("active item is marked with aria-pressed", () => {
    render(() => (
      <Toolbar items={[{ id: "r", label: "Run", onClick: vi.fn(), active: true }]} />
    ));
    expect(screen.getByText("Run").closest("button")!.getAttribute("aria-pressed")).toBe("true");
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => <Toolbar items={ITEMS} class="my-tb" />);
    expect(container.firstElementChild!.classList.contains("my-tb")).toBe(true);
  });
});
