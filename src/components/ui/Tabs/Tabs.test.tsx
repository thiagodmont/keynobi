import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Tabs } from "./Tabs";

const TABS = [
  { id: "a", label: "Alpha" },
  { id: "b", label: "Beta" },
  { id: "c", label: "Gamma", badge: 3 },
];

describe("Tabs", () => {
  it("renders all tab labels", () => {
    render(() => <Tabs tabs={TABS} activeTab="a" onChange={vi.fn()} />);
    expect(screen.getByText("Alpha")).not.toBeNull();
    expect(screen.getByText("Beta")).not.toBeNull();
    expect(screen.getByText("Gamma")).not.toBeNull();
  });

  it("marks the active tab with aria-selected=true", () => {
    const { container } = render(() => (
      <Tabs tabs={TABS} activeTab="b" onChange={vi.fn()} />
    ));
    const btns = container.querySelectorAll("button");
    expect(btns[1].getAttribute("aria-selected")).toBe("true");
    expect(btns[0].getAttribute("aria-selected")).toBe("false");
  });

  it("calls onChange with tab id on click", () => {
    const fn = vi.fn();
    render(() => <Tabs tabs={TABS} activeTab="a" onChange={fn} />);
    fireEvent.click(screen.getByText("Beta"));
    expect(fn).toHaveBeenCalledWith("b");
  });

  it("does not call onChange when active tab is clicked", () => {
    const fn = vi.fn();
    render(() => <Tabs tabs={TABS} activeTab="a" onChange={fn} />);
    fireEvent.click(screen.getByText("Alpha"));
    expect(fn).not.toHaveBeenCalled();
  });

  it("renders badge when tab has badge prop", () => {
    render(() => <Tabs tabs={TABS} activeTab="a" onChange={vi.fn()} />);
    expect(screen.getByText("3")).not.toBeNull();
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => (
      <Tabs tabs={TABS} activeTab="a" onChange={vi.fn()} class="my-tabs" />
    ));
    expect(container.firstElementChild!.classList.contains("my-tabs")).toBe(true);
  });

  it("each tab button has role=tab", () => {
    const { container } = render(() => (
      <Tabs tabs={TABS} activeTab="a" onChange={vi.fn()} />
    ));
    const btns = container.querySelectorAll("[role='tab']");
    expect(btns.length).toBe(3);
  });
});
