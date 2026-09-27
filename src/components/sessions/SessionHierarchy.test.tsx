import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import type { DebugSessionHierarchy, DebugSessionHierarchyNode } from "@/bindings";
import { SessionHierarchy, visibleNodes } from "./SessionHierarchy";

const node = (depth: number, text = ""): DebugSessionHierarchyNode => ({
  depth,
  class: "android.view.View",
  resourceId: "",
  text,
  contentDesc: "",
  bounds: "[0,0][1,1]",
});

describe("visibleNodes", () => {
  const nodes = [node(0), node(1), node(2), node(1), node(0), node(1)];

  it("shows every node when nothing is collapsed", () => {
    expect(visibleNodes(nodes, new Set())).toEqual([0, 1, 2, 3, 4, 5]);
  });

  it("hides the subtree of a collapsed node, and only that", () => {
    expect(visibleNodes(nodes, new Set([1]))).toEqual([0, 1, 3, 4, 5]);
    expect(visibleNodes(nodes, new Set([0]))).toEqual([0, 4, 5]);
    expect(visibleNodes(nodes, new Set([0, 4]))).toEqual([0, 4]);
    // A collapsed node inside a collapsed one changes nothing more.
    expect(visibleNodes(nodes, new Set([0, 1]))).toEqual([0, 4, 5]);
  });
});

describe("SessionHierarchy", () => {
  beforeEach(() => {
    if (!window.ResizeObserver) {
      class MockResizeObserver {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      }
      window.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;
    }
  });

  afterEach(() => {
    cleanup();
    vi.mocked(invoke).mockReset();
  });

  it("renders only the rows in view of a large tree, and says when it was cut", async () => {
    const nodes = Array.from({ length: 8000 }, (_, i) => node(i === 0 ? 0 : 1, `row ${i}`));
    const hierarchy: DebugSessionHierarchy = {
      capturedAt: "2026-09-25T10:40:00Z",
      foregroundActivity: null,
      truncated: true,
      nodes,
    };
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === "get_session_hierarchy") return hierarchy;
      throw new Error(`unexpected ${cmd}`);
    });
    render(() => <SessionHierarchy sessionId="s-1" seq={3} />);
    const tree = await screen.findByRole("tree", { name: "UI hierarchy" });
    const rows = tree.querySelectorAll('[role="treeitem"]');
    expect(rows.length).toBeGreaterThan(0);
    expect(rows.length).toBeLessThan(200);
    expect(document.body.textContent).toContain("8000 nodes");
    expect(document.body.textContent).toContain("cut to the capture limits");
  });

  it("shows why a hierarchy cannot be read", async () => {
    vi.mocked(invoke).mockRejectedValue({
      kind: "notFound",
      message: "Debug session s-1 has no attachment for event 3",
    });
    render(() => <SessionHierarchy sessionId="s-1" seq={3} />);
    expect(await screen.findByText(/has no attachment for event 3/)).toBeTruthy();
  });
});
