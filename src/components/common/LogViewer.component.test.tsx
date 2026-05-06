import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LogEntry } from "@/bindings";
import { LogViewer } from "./LogViewer";

function makeEntry(id: number): LogEntry {
  return {
    id,
    timestamp: "2026-05-06T12:00:00.000Z",
    level: "info",
    source: "build",
    message: `line ${id}`,
  };
}

function getScroller(container: HTMLElement): HTMLElement {
  const scroller = Array.from(container.querySelectorAll("div")).find(
    (el) => (el as HTMLElement).style.overflowY === "auto"
  );
  if (!(scroller instanceof HTMLElement)) {
    throw new Error("Expected LogViewer to render a VirtualList scroller");
  }
  return scroller;
}

describe("LogViewer", () => {
  beforeEach(() => {
    if (typeof window !== "undefined" && !window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as any;
    }
  });

  it("scrolls to the end when the user manually resumes auto-scroll", () => {
    const entries = Array.from({ length: 100 }, (_, i) => makeEntry(i + 1));
    const { container } = render(() => (
      <LogViewer entries={entries} showSource={false} defaultAutoScroll={true} />
    ));
    const scroller = getScroller(container);
    Object.defineProperty(scroller, "clientHeight", { value: 200, writable: true });
    Object.defineProperty(scroller, "scrollHeight", { value: 2000, writable: true });
    Object.defineProperty(scroller, "scrollTop", { value: 0, writable: true });

    fireEvent.scroll(scroller);
    fireEvent.click(screen.getByTitle("Auto-scroll paused (click to resume)"));

    expect(scroller.scrollTop).toBe(2000);
  });
});
