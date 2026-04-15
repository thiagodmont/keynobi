import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";
import { VirtualList } from "./VirtualList";

describe("VirtualList", () => {
  beforeEach(() => {
    // Mock ResizeObserver for jsdom
    if (typeof window !== "undefined" && !window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as any;
    }
  });

  it("renders visible items", () => {
    const items = Array.from({ length: 10 }, (_, i) => `item-${i}`);
    const { container } = render(() => (
      <VirtualList
        items={items}
        rowHeight={30}
        renderRow={(item) => <div>{item}</div>}
      />
    ));
    // At least some items rendered (jsdom has no real scroll height, so all items may render)
    expect(container.textContent).toContain("item-0");
  });

  it("calls onScrolledToBottom when scrolling near the end", () => {
    const onScrolledToBottom = vi.fn();
    const items = Array.from({ length: 5 }, (_, i) => `item-${i}`);
    const { container } = render(() => (
      <VirtualList
        items={items}
        rowHeight={30}
        renderRow={(item) => <div>{item}</div>}
        onScrolledToBottom={onScrolledToBottom}
      />
    ));
    // wasAtBottom starts as true, so we need to scroll away first, then back to trigger callback
    const scroller = container.firstElementChild as HTMLElement;
    Object.defineProperty(scroller, "clientHeight", { value: 200, writable: true });
    Object.defineProperty(scroller, "scrollHeight", { value: 1200, writable: true });

    // First: scroll away from bottom (distFromBottom > threshold)
    Object.defineProperty(scroller, "scrollTop", { value: 0, writable: true });
    scroller.dispatchEvent(new Event("scroll"));
    expect(onScrolledToBottom).toHaveBeenCalledTimes(0);

    // Then: scroll to bottom (distFromBottom < threshold) to trigger callback
    Object.defineProperty(scroller, "scrollTop", { value: 1000, writable: true });
    scroller.dispatchEvent(new Event("scroll"));
    expect(onScrolledToBottom).toHaveBeenCalledOnce();
  });
});
