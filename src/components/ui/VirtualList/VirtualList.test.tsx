import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
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
      <VirtualList items={items} rowHeight={30} renderRow={(item) => <div>{item}</div>} />
    ));
    // At least some items rendered (jsdom has no real scroll height, so all items may render)
    expect(container.textContent).toContain("item-0");
  });

  it("does not re-enable auto-scroll when manually scrolling back to the bottom", () => {
    const onScrolledUp = vi.fn();
    const items = Array.from({ length: 5 }, (_, i) => `item-${i}`);

    function Harness() {
      const [autoScroll, setAutoScroll] = createSignal(true);
      return (
        <>
          <span data-testid="auto-scroll-state">{autoScroll() ? "on" : "off"}</span>
          <VirtualList
            items={items}
            rowHeight={30}
            renderRow={(item) => <div>{item}</div>}
            autoScroll={autoScroll()}
            onScrolledUp={() => {
              onScrolledUp();
              setAutoScroll(false);
            }}
            class="virtual-list-test"
          />
        </>
      );
    }

    const { container } = render(() => <Harness />);
    // wasAtBottom starts as true, so scrolling away disables follow-tail.
    const state = container.querySelector('[data-testid="auto-scroll-state"]') as HTMLElement;
    const scroller = container.querySelector(".virtual-list-test") as HTMLElement;
    Object.defineProperty(scroller, "clientHeight", { value: 200, writable: true });
    Object.defineProperty(scroller, "scrollHeight", { value: 1200, writable: true });

    Object.defineProperty(scroller, "scrollTop", { value: 0, writable: true });
    scroller.dispatchEvent(new Event("scroll"));
    expect(onScrolledUp).toHaveBeenCalledOnce();
    expect(state.textContent).toBe("off");

    // Returning to the bottom manually should not re-enable follow-tail.
    Object.defineProperty(scroller, "scrollTop", { value: 1000, writable: true });
    scroller.dispatchEvent(new Event("scroll"));
    expect(state.textContent).toBe("off");
  });
});
