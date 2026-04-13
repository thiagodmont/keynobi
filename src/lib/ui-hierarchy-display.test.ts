import { describe, expect, it } from "vitest";
import type { UiNode } from "@/bindings";
import {
  boundsArea,
  boundsWidthHeight,
  collapseBoringChains,
  collectSearchMatchPaths,
  defaultExpandDepthForNodeCount,
  filterInteractiveTree,
  filterSearchTree,
  flattenNodesWithBounds,
  getNodeAtPath,
  inferScreenSizeFromRects,
  parentLayoutPath,
  isMinifiedClassName,
  nodeMatchesInteractive,
  parseBoundsRect,
  pathOverridesToRevealAncestorPath,
  pathOverridesToRevealPath,
  pickNodePathAtDevicePoint,
  shortClassName,
} from "./ui-hierarchy-display";

function node(partial: Partial<UiNode> & Pick<UiNode, "class" | "bounds">): UiNode {
  return {
    resourceId: "",
    text: "",
    contentDesc: "",
    package: "",
    clickable: false,
    enabled: true,
    focusable: false,
    focused: false,
    scrollable: false,
    longClickable: false,
    password: false,
    checkable: false,
    checked: false,
    editable: false,
    selected: false,
    isComposeHeuristic: false,
    children: [],
    ...partial,
  };
}

describe("boundsArea", () => {
  it("parses standard bounds", () => {
    expect(boundsArea("[0,0][1080,100]")).toBe(108_000);
  });

  it("returns 0 for invalid", () => {
    expect(boundsArea("")).toBe(0);
  });
});

describe("nodeMatchesInteractive", () => {
  it("matches clickable with area", () => {
    expect(
      nodeMatchesInteractive(
        node({ class: "android.widget.Button", bounds: "[0,0][10,10]", clickable: true })
      )
    ).toBe(true);
  });

  it("rejects zero area", () => {
    expect(
      nodeMatchesInteractive(
        node({ class: "android.view.View", bounds: "[0,0][0,0]", clickable: true })
      )
    ).toBe(false);
  });
});

describe("filterInteractiveTree", () => {
  it("keeps branch with button child", () => {
    const root = node({
      class: "FrameLayout",
      bounds: "[0,0][100,100]",
      children: [
        node({
          class: "Button",
          bounds: "[0,0][50,50]",
          clickable: true,
        }),
      ],
    });
    const out = filterInteractiveTree(root);
    expect(out).not.toBeNull();
    expect(out!.children.length).toBe(1);
  });
});

describe("filterSearchTree", () => {
  it("filters by class substring", () => {
    const root = node({
      class: "android.widget.FrameLayout",
      bounds: "[0,0][100,100]",
      children: [
        node({ class: "android.widget.Button", bounds: "[0,0][10,10]", text: "OK" }),
      ],
    });
    const out = filterSearchTree(root, "Button");
    expect(out).not.toBeNull();
    expect(out!.children.length).toBe(1);
  });
});

describe("shortClassName", () => {
  it("takes last segment", () => {
    expect(shortClassName("android.widget.TextView")).toBe("TextView");
  });
});

describe("boundsWidthHeight", () => {
  it("returns width and height", () => {
    expect(boundsWidthHeight("[0,0][1080,2400]")).toEqual({ w: 1080, h: 2400 });
  });
});

describe("isMinifiedClassName", () => {
  it("detects short obfuscated names", () => {
    expect(isMinifiedClassName("c60")).toBe(true);
    expect(isMinifiedClassName("android.view.View")).toBe(false);
  });
});

describe("collapseBoringChains", () => {
  it("collapses a same-bounds single-child boring stack", () => {
    const b = "[0,0][100,100]";
    const leaf = node({
      class: "android.widget.TextView",
      bounds: "[0,0][30,20]",
      text: "Hi",
    });
    const inner = node({
      class: "android.view.View",
      bounds: b,
      children: [leaf],
    });
    const mid = node({
      class: "android.view.View",
      bounds: b,
      children: [inner],
    });
    const root = node({
      class: "android.widget.FrameLayout",
      bounds: b,
      children: [mid],
    });
    const out = collapseBoringChains(root);
    expect(out.class).toContain("KeynobiCollapsed");
    expect(out.children.length).toBe(1);
    // Last non-boring ancestor (inner View) remains above the leaf with different bounds.
    expect(out.children[0]!.children[0]!.class).toBe("android.widget.TextView");
    expect(out.children[0]!.children[0]!.text).toBe("Hi");
  });

  it("preserves two siblings under one parent", () => {
    const b = "[0,0][100,100]";
    const a = node({ class: "android.view.View", bounds: "[0,0][10,10]" });
    const c = node({ class: "android.view.View", bounds: "[20,0][30,10]" });
    const root = node({
      class: "android.widget.FrameLayout",
      bounds: b,
      children: [a, c],
    });
    const out = collapseBoringChains(root);
    expect(out.children.length).toBe(2);
  });
});

describe("getNodeAtPath", () => {
  it("resolves empty path to root", () => {
    const r = node({ class: "R", bounds: "[0,0][1,1]", children: [] });
    expect(getNodeAtPath(r, "")).toBe(r);
  });

  it("resolves nested indices", () => {
    const leaf = node({ class: "L", bounds: "[0,0][1,1]" });
    const r = node({
      class: "R",
      bounds: "[0,0][10,10]",
      children: [
        node({
          class: "M",
          bounds: "[0,0][5,5]",
          children: [leaf],
        }),
      ],
    });
    expect(getNodeAtPath(r, "0")?.class).toContain("M");
    expect(getNodeAtPath(r, "0.0")).toBe(leaf);
  });
});

function collectLayoutPaths(root: UiNode): string[] {
  const out: string[] = [""];
  const walk = (n: UiNode, path: string): void => {
    n.children.forEach((c, i) => {
      const childPath = path === "" ? String(i) : `${path}.${i}`;
      out.push(childPath);
      walk(c, childPath);
    });
  };
  walk(root, "");
  return out;
}

describe("parentLayoutPath", () => {
  it("returns null for display root", () => {
    expect(parentLayoutPath("")).toBeNull();
  });

  it("returns empty string for top-level child", () => {
    expect(parentLayoutPath("0")).toBe("");
    expect(parentLayoutPath("3")).toBe("");
  });

  it("drops last segment for deeper paths", () => {
    expect(parentLayoutPath("0.1.2")).toBe("0.1");
  });

  it("handles multi-digit indices", () => {
    expect(parentLayoutPath("0.10")).toBe("0");
    expect(parentLayoutPath("2.10.3")).toBe("2.10");
  });

  it("ignores empty segments from stray dots", () => {
    expect(parentLayoutPath("0..1")).toBe("0");
  });

  it("walking parentLayoutPath repeatedly reaches the root path then stops", () => {
    const chain: string[] = [];
    let p: string = "0.1.2.3";
    for (;;) {
      const pp = parentLayoutPath(p);
      if (pp === null) {
        break;
      }
      chain.push(pp);
      p = pp;
    }
    expect(chain).toEqual(["0.1.2", "0.1", "0", ""]);
    expect(parentLayoutPath("")).toBeNull();
  });

  it("for every path in a tree, parent path resolves to the node that contains the child", () => {
    const leaf = node({ class: "L", bounds: "[0,0][1,1]" });
    const mid = node({
      class: "M",
      bounds: "[0,0][5,5]",
      children: [leaf, node({ class: "L2", bounds: "[0,0][1,1]" })],
    });
    const r = node({
      class: "R",
      bounds: "[0,0][10,10]",
      children: [
        mid,
        node({
          class: "S",
          bounds: "[0,0][3,3]",
          children: [node({ class: "T", bounds: "[0,0][1,1]", children: [] })],
        }),
      ],
    });
    for (const p of collectLayoutPaths(r)) {
      const pp = parentLayoutPath(p);
      if (pp === null) {
        expect(p).toBe("");
        continue;
      }
      const parentNode = getNodeAtPath(r, pp);
      const childNode = getNodeAtPath(r, p);
      expect(parentNode).not.toBeNull();
      expect(childNode).not.toBeNull();
      expect(parentNode!.children.includes(childNode!)).toBe(true);
    }
  });
});

describe("pathOverridesToRevealPath", () => {
  it("includes root and prefixes", () => {
    expect(pathOverridesToRevealPath("2.0.1")).toEqual({
      "": true,
      "2": true,
      "2.0": true,
      "2.0.1": true,
    });
  });
});

describe("pathOverridesToRevealAncestorPath", () => {
  it("returns empty for root selection", () => {
    expect(pathOverridesToRevealAncestorPath("")).toEqual({});
  });

  it("opens only display root for depth-one path", () => {
    expect(pathOverridesToRevealAncestorPath("4")).toEqual({ "": true });
  });

  it("opens ancestors but not the leaf path", () => {
    expect(pathOverridesToRevealAncestorPath("2.0.1")).toEqual({
      "": true,
      "2": true,
      "2.0": true,
    });
  });
});

describe("collectSearchMatchPaths", () => {
  it("finds matching node paths", () => {
    const root = node({
      class: "FrameLayout",
      bounds: "[0,0][1,1]",
      children: [
        node({ class: "Button", bounds: "[0,0][1,1]", text: "OK" }),
        node({ class: "TextView", bounds: "[0,0][1,1]", text: "Nope" }),
      ],
    });
    const paths = collectSearchMatchPaths(root, "Button");
    expect(paths).toContain("0");
  });
});

describe("defaultExpandDepthForNodeCount", () => {
  it("deepens expand for small trees", () => {
    expect(defaultExpandDepthForNodeCount(30)).toBe(8);
    expect(defaultExpandDepthForNodeCount(100)).toBe(5);
    expect(defaultExpandDepthForNodeCount(500)).toBe(3);
  });
});

describe("parseBoundsRect", () => {
  it("parses standard bounds (half-open right/bottom)", () => {
    expect(parseBoundsRect("[0,0][100,200]")).toEqual({
      left: 0,
      top: 0,
      right: 100,
      bottom: 200,
    });
  });

  it("rejects zero area", () => {
    expect(parseBoundsRect("[5,5][5,10]")).toBeNull();
    expect(parseBoundsRect("")).toBeNull();
  });
});

describe("flattenNodesWithBounds", () => {
  it("collects paths and skips empty bounds", () => {
    const root = node({
      class: "R",
      bounds: "[0,0][10,10]",
      children: [
        node({ class: "A", bounds: "[1,1][5,5]" }),
        node({ class: "B", bounds: "" }),
      ],
    });
    const flat = flattenNodesWithBounds(root);
    expect(flat.length).toBe(2);
    expect(flat.map((e) => e.path).sort()).toEqual(["", "0"]);
    expect(flat.find((e) => e.path === "0")!.node.class).toBe("A");
  });
});

describe("inferScreenSizeFromRects", () => {
  it("uses max extents", () => {
    const entries = [
      { path: "0", rect: { left: 0, top: 0, right: 50, bottom: 40 }, area: 1, node: node({ class: "x", bounds: "" }) },
    ];
    expect(inferScreenSizeFromRects(entries)).toEqual({ width: 50, height: 40 });
  });

  it("defaults when empty", () => {
    expect(inferScreenSizeFromRects([])).toEqual({ width: 1080, height: 2400 });
  });
});

describe("pickNodePathAtDevicePoint", () => {
  it("prefers smallest containing rect", () => {
    const big = node({ class: "Big", bounds: "[0,0][100,100]" });
    const small = node({ class: "Small", bounds: "[10,10][20,20]" });
    const root = node({
      class: "R",
      bounds: "[0,0][100,100]",
      children: [big, small],
    });
    const flat = flattenNodesWithBounds(root);
    expect(pickNodePathAtDevicePoint(flat, 15, 15)).toBe("1");
  });
});
