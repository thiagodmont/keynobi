import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Dropdown } from "./Dropdown";

const ITEMS = [
  { label: "Copy", onClick: vi.fn() },
  { label: "Paste", onClick: vi.fn() },
  { label: "Delete", onClick: vi.fn(), disabled: true },
];

describe("Dropdown", () => {
  it("renders the trigger", () => {
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={ITEMS} />
    ));
    expect(screen.getByText("Open")).not.toBeNull();
  });

  it("menu is not visible initially", () => {
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={ITEMS} />
    ));
    expect(screen.queryByText("Copy")).toBeNull();
  });

  it("opens on trigger click", () => {
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={ITEMS} />
    ));
    fireEvent.click(screen.getByText("Open"));
    expect(screen.getByText("Copy")).not.toBeNull();
    expect(screen.getByText("Paste")).not.toBeNull();
  });

  it("closes on second trigger click", () => {
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={ITEMS} />
    ));
    fireEvent.click(screen.getByText("Open"));
    fireEvent.click(screen.getByText("Open"));
    expect(screen.queryByText("Copy")).toBeNull();
  });

  it("calls item onClick and closes on item click", () => {
    const fn = vi.fn();
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={[{ label: "Go", onClick: fn }]} />
    ));
    fireEvent.click(screen.getByText("Open"));
    fireEvent.click(screen.getByText("Go"));
    expect(fn).toHaveBeenCalledOnce();
    expect(screen.queryByText("Go")).toBeNull();
  });

  it("does not call onClick for disabled item", () => {
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={ITEMS} />
    ));
    fireEvent.click(screen.getByText("Open"));
    fireEvent.click(screen.getByText("Delete"));
    expect(ITEMS[2].onClick).not.toHaveBeenCalled();
  });

  it("closes on Escape key", () => {
    render(() => (
      <Dropdown trigger={<button>Open</button>} items={ITEMS} />
    ));
    fireEvent.click(screen.getByText("Open"));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByText("Copy")).toBeNull();
  });

  it("navigates items with ArrowDown and activates with Enter", () => {
    const fn = vi.fn();
    render(() => (
      <Dropdown
        trigger={<button>Open</button>}
        items={[{ label: "First", onClick: fn }, { label: "Second", onClick: vi.fn() }]}
      />
    ));
    fireEvent.click(screen.getByText("Open"));
    fireEvent.keyDown(document, { key: "ArrowDown" });
    fireEvent.keyDown(document, { key: "Enter" });
    expect(fn).toHaveBeenCalledOnce();
  });
});
