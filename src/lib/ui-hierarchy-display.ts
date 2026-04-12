import type { UiNode } from "@/bindings";

/** Synthetic node: collapsed same-bounds wrapper chain (see `collapseBoringChains`). */
export const COLLAPSED_WRAPPERS_CLASS = "android.view.KeynobiCollapsedWrappers";

/** Max rectangles drawn in the layout wireframe (performance). */
export const WIREFRAME_RECT_CAP = 2000;

const TEXT_PREVIEW_ROW = 48;

/** Parse bounds `[l,t][r,b]` → width × height (null if invalid). */
export function boundsWidthHeight(bounds: string): { w: number; h: number } | null {
  const s = bounds.replace(/\]\[/g, ",").replace(/[\[\]]/g, "");
  const parts = s.split(",").map((p) => Number(p.trim()));
  if (parts.length !== 4 || parts.some((n) => Number.isNaN(n))) return null;
  const w = Math.max(0, parts[2] - parts[0]);
  const h = Math.max(0, parts[3] - parts[1]);
  return { w, h };
}

/** Parse UI Automator bounds → area (0 if invalid). */
export function boundsArea(bounds: string): number {
  const wh = boundsWidthHeight(bounds);
  return wh ? wh.w * wh.h : 0;
}

/** Bounds string plus ` (W×H)` for quick scanning. */
export function formatBoundsWithSize(bounds: string): string {
  const wh = boundsWidthHeight(bounds);
  if (!wh) return bounds;
  return `${bounds} (${wh.w}×${wh.h})`;
}

export function shortClassName(className: string): string {
  const i = className.lastIndexOf(".");
  return i >= 0 ? className.slice(i + 1) : className;
}

/** Heuristic for minified Compose / R8 short names (e.g. `c60`). */
export function isMinifiedClassName(className: string): boolean {
  const short = shortClassName(className);
  return /^[a-z][a-z0-9]{0,4}$/.test(short);
}

function isKeynobiInternalClass(className: string): boolean {
  return className.includes("Keynobi");
}

/** True if this node is a plausible no-op wrapper for same-bounds chain collapse. */
export function isBoringWrapperForCollapse(n: UiNode): boolean {
  if (isKeynobiInternalClass(n.class)) return false;
  if (n.text.length > 0 || n.contentDesc.length > 0 || n.resourceId.length > 0) return false;
  if (n.clickable || n.longClickable || n.scrollable || n.editable) return false;
  if (n.checkable || n.checked || n.selected) return false;
  if (n.focusable) return false;
  return true;
}

function makeCollapsedSynthetic(count: number, bounds: string): UiNode {
  return {
    class: COLLAPSED_WRAPPERS_CLASS,
    resourceId: "",
    text: "",
    contentDesc: `${count}× same-bounds wrapper (collapsed)`,
    package: "",
    bounds,
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
  };
}

/**
 * Collapse vertical chains of single-child, same-bounds, boring wrappers (sibling-safe).
 */
export function collapseBoringChains(n: UiNode): UiNode {
  if (isKeynobiInternalClass(n.class)) {
    return { ...n, children: n.children.map(collapseBoringChains) };
  }
  if (n.children.length !== 1) {
    return { ...n, children: n.children.map(collapseBoringChains) };
  }
  let cur = n;
  let depth = 0;
  while (
    cur.children.length === 1 &&
    isBoringWrapperForCollapse(cur) &&
    cur.bounds === cur.children[0].bounds &&
    !isKeynobiInternalClass(cur.children[0].class)
  ) {
    depth += 1;
    cur = cur.children[0];
  }
  if (depth === 0) {
    return { ...n, children: n.children.map(collapseBoringChains) };
  }
  const synth = makeCollapsedSynthetic(depth, n.bounds);
  synth.children = [collapseBoringChains(cur)];
  return synth;
}

/** Total node count (including root) for expand-depth heuristics. */
export function countTreeNodes(n: UiNode): number {
  let c = 1;
  for (const ch of n.children) {
    c += countTreeNodes(ch);
  }
  return c;
}

/** Deeper default expand when the (display) tree is small enough (e.g. NiA). */
export function defaultExpandDepthForNodeCount(nodeCount: number): number {
  if (nodeCount <= 60) return 8;
  if (nodeCount <= 200) return 5;
  return 3;
}

/** Dim full-screen chrome: huge area, no id/text/desc, non-interactive, common container classes. */
export function isLikelyFullScreenChrome(n: UiNode): boolean {
  if (n.class === COLLAPSED_WRAPPERS_CLASS) return false;
  const area = boundsArea(n.bounds);
  if (area < 500_000) return false;
  if (n.text.length > 0 || n.contentDesc.length > 0 || n.resourceId.length > 0) return false;
  if (n.clickable || n.scrollable || n.editable || n.longClickable || n.selected) return false;
  const sc = shortClassName(n.class);
  return (
    sc === "FrameLayout" ||
    sc === "LinearLayout" ||
    sc === "View" ||
    n.class === "androidx.compose.ui.platform.ComposeView"
  );
}

function truncatePreview(s: string, max: number): string {
  if (s.length <= max) return s;
  return `${s.slice(0, max)}…`;
}

/** Shallow BFS for first non-empty text or content-desc under `n` (for interactive-only rows). */
export function firstDescendantTextOrDesc(n: UiNode, maxDepth: number): string {
  const q: { node: UiNode; d: number }[] = [{ node: n, d: 0 }];
  while (q.length > 0) {
    const item = q.shift();
    if (!item) break;
    const { node, d } = item;
    if (node.class === COLLAPSED_WRAPPERS_CLASS) {
      for (const c of node.children) {
        q.push({ node: c, d });
      }
      continue;
    }
    if (node.text.length > 0) {
      return truncatePreview(node.text, 72);
    }
    if (node.contentDesc.length > 0) {
      return truncatePreview(node.contentDesc, 72);
    }
    if (d >= maxDepth) continue;
    for (const c of node.children) {
      q.push({ node: c, d: d + 1 });
    }
  }
  return "";
}

function hasNonClickableButtonLikeDescendant(n: UiNode): boolean {
  const sc = shortClassName(n.class);
  if ((sc === "Button" || sc === "CheckBox" || n.class.includes("Button")) && !n.clickable) {
    return true;
  }
  return n.children.some(hasNonClickableButtonLikeDescendant);
}

/** Compose-style merged semantics: clickable parent, labeled child, non-clickable control leaf. */
export function isMergedTapTargetHeuristic(n: UiNode): boolean {
  if (!n.clickable) return false;
  const labeledChild = n.children.some((c) => c.text.length > 0 || c.contentDesc.length > 0);
  if (!labeledChild) return false;
  return hasNonClickableButtonLikeDescendant(n);
}

/** Single-line copy for debugging / Slack. */
export function buildNodeSummaryLine(n: UiNode): string {
  const parts: string[] = [shortClassName(n.class)];
  if (n.resourceId) parts.push(`id=${n.resourceId}`);
  if (n.text) parts.push(`text=${truncatePreview(n.text, 80)}`);
  if (n.contentDesc) parts.push(`desc=${truncatePreview(n.contentDesc, 80)}`);
  parts.push(n.bounds);
  return parts.join(" | ");
}

function walkPackages(n: UiNode, out: Map<string, number>, budget: number): number {
  if (budget <= 0) return 0;
  let b = budget - 1;
  if (n.package.length > 0) {
    out.set(n.package, (out.get(n.package) ?? 0) + 1);
  }
  for (const c of n.children) {
    b = walkPackages(c, out, b);
    if (b <= 0) break;
  }
  return b;
}

/** Dominant `package` among first ~80 nodes (for toolbar de-dupe). */
export function inferDominantPackage(root: UiNode): string | null {
  const m = new Map<string, number>();
  walkPackages(root, m, 80);
  let best: string | null = null;
  let bestN = 0;
  for (const [k, v] of m) {
    if (v > bestN) {
      best = k;
      bestN = v;
    }
  }
  return best;
}

/** Primary row snippet: text and/or content-desc (never hide desc when text empty). */
export function formatRowSnippet(n: UiNode, interactiveOnly: boolean): { text: string; desc: string } {
  let text = n.text.length > 0 ? truncatePreview(n.text, TEXT_PREVIEW_ROW) : "";
  let desc =
    n.contentDesc.length > 0 ? truncatePreview(n.contentDesc, TEXT_PREVIEW_ROW) : "";
  if (interactiveOnly && !text && !desc) {
    const inherited = firstDescendantTextOrDesc(n, 10);
    if (inherited.length > 0) {
      text = `→ ${inherited}`;
    }
  }
  return { text, desc };
}

/** Droidclaw-style: interactive targets or nodes with visible text/description. */
export function nodeMatchesInteractive(n: UiNode): boolean {
  if (n.class === COLLAPSED_WRAPPERS_CLASS) {
    return false;
  }
  const hasContent = n.text.length > 0 || n.contentDesc.length > 0;
  const interactive =
    n.clickable || n.longClickable || n.scrollable || n.editable;
  if (!interactive && !hasContent) return false;
  return boundsArea(n.bounds) > 0;
}

export function filterInteractiveTree(n: UiNode): UiNode | null {
  if (n.class === COLLAPSED_WRAPPERS_CLASS) {
    const children = n.children
      .map((c) => filterInteractiveTree(c))
      .filter((x): x is UiNode => x !== null);
    if (children.length === 0) return null;
    return { ...n, children };
  }
  const children = n.children
    .map((c) => filterInteractiveTree(c))
    .filter((x): x is UiNode => x !== null);
  const selfMatch = nodeMatchesInteractive(n);
  if (selfMatch || children.length > 0) {
    return { ...n, children };
  }
  return null;
}

export function nodeMatchesSearch(n: UiNode, q: string): boolean {
  const s = q.toLowerCase();
  return (
    n.class.toLowerCase().includes(s) ||
    n.resourceId.toLowerCase().includes(s) ||
    n.text.toLowerCase().includes(s) ||
    n.contentDesc.toLowerCase().includes(s) ||
    n.package.toLowerCase().includes(s)
  );
}

export function filterSearchTree(n: UiNode, q: string): UiNode | null {
  const trimmed = q.trim();
  if (!trimmed) {
    return n;
  }
  const children = n.children
    .map((c) => filterSearchTree(c, trimmed))
    .filter((x): x is UiNode => x !== null);
  const selfMatch = nodeMatchesSearch(n, trimmed);
  if (selfMatch || children.length > 0) {
    return { ...n, children };
  }
  return null;
}

/** DFS paths (e.g. `0.1.2`) to nodes matching search under `root`. */
export function collectSearchMatchPaths(root: UiNode, query: string): string[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return [];
  const out: string[] = [];
  const walk = (n: UiNode, path: string): void => {
    if (nodeMatchesSearch(n, trimmed)) {
      out.push(path);
    }
    n.children.forEach((c, i) => {
      const childPath = path === "" ? String(i) : `${path}.${i}`;
      walk(c, childPath);
    });
  };
  walk(root, "");
  return out;
}

/** Resolve `path` from display-tree root (`""` = root; `0.1` = first child’s second child). */
export function getNodeAtPath(root: UiNode, path: string): UiNode | null {
  if (path === "") {
    return root;
  }
  const segments = path.split(".").filter((s) => s.length > 0);
  let cur: UiNode = root;
  for (const seg of segments) {
    const idx = Number(seg);
    if (!Number.isFinite(idx) || idx < 0 || idx >= cur.children.length) {
      return null;
    }
    cur = cur.children[idx]!;
  }
  return cur;
}

/**
 * Direct parent path in the displayed tree (`""` = display root row).
 * `null` only when `path` is the root (`""`) — every other node has exactly one parent.
 */
export function parentLayoutPath(path: string): string | null {
  if (path === "") {
    return null;
  }
  const parts = path.split(".").filter((s) => s.length > 0);
  if (parts.length <= 1) {
    return "";
  }
  return parts.slice(0, -1).join(".");
}

/**
 * Open-state overrides so `path` is visible (expand display root and each ancestor prefix).
 */
export function pathOverridesToRevealPath(path: string): Record<string, boolean> {
  const o: Record<string, boolean> = {};
  if (!path) return o;
  o[""] = true;
  const parts = path.split(".").filter((s) => s.length > 0);
  let acc = parts[0]!;
  o[acc] = true;
  for (let i = 1; i < parts.length; i++) {
    acc = `${acc}.${parts[i]!}`;
    o[acc] = true;
  }
  return o;
}

/**
 * Open-state overrides so `path` is **reachable** in the tree: expand the display root and each
 * ancestor prefix, but **not** `path` itself (the selected row stays visible; its subtree stays
 * collapsed unless default depth / other rules open it). Use for wireframe or list selection sync
 * without forcing the whole branch under the leaf open.
 */
export function pathOverridesToRevealAncestorPath(path: string): Record<string, boolean> {
  const o: Record<string, boolean> = {};
  if (!path) return o;
  o[""] = true;
  const parts = path.split(".").filter((s) => s.length > 0);
  if (parts.length <= 1) {
    return o;
  }
  let acc = parts[0]!;
  o[acc] = true;
  for (let i = 1; i < parts.length - 1; i++) {
    acc = `${acc}.${parts[i]!}`;
    o[acc] = true;
  }
  return o;
}

/** Device-pixel rectangle from UI Automator `bounds` (`[l,t][r,b]`, right/bottom treated as exclusive). */
export function parseBoundsRect(
  bounds: string
): { left: number; top: number; right: number; bottom: number } | null {
  const s = bounds.replace(/\]\[/g, ",").replace(/[\[\]]/g, "");
  const parts = s.split(",").map((p) => Number(p.trim()));
  if (parts.length !== 4 || parts.some((n) => Number.isNaN(n))) return null;
  const left = parts[0]!;
  const top = parts[1]!;
  const right = parts[2]!;
  const bottom = parts[3]!;
  if (right <= left || bottom <= top) return null;
  return { left, top, right, bottom };
}

export interface WireframeRectEntry {
  path: string;
  rect: { left: number; top: number; right: number; bottom: number };
  area: number;
  node: UiNode;
}

/**
 * All nodes under `root` with valid positive-area bounds, tree paths matching the hierarchy viewer.
 */
export function flattenNodesWithBounds(root: UiNode): WireframeRectEntry[] {
  const out: WireframeRectEntry[] = [];
  const walk = (n: UiNode, path: string): void => {
    const pr = parseBoundsRect(n.bounds);
    if (pr) {
      const area = (pr.right - pr.left) * (pr.bottom - pr.top);
      if (area > 0) {
        out.push({ path, rect: pr, area, node: n });
      }
    }
    n.children.forEach((c, i) => {
      const childPath = path === "" ? String(i) : `${path}.${i}`;
      walk(c, childPath);
    });
  };
  walk(root, "");
  return out;
}

const DEFAULT_SCREEN_W = 1080;
const DEFAULT_SCREEN_H = 2400;

/** Infer logical screen size from rect extents (max right/bottom). */
export function inferScreenSizeFromRects(
  rects: WireframeRectEntry[]
): { width: number; height: number } {
  if (rects.length === 0) {
    return { width: DEFAULT_SCREEN_W, height: DEFAULT_SCREEN_H };
  }
  let maxR = 0;
  let maxB = 0;
  for (const e of rects) {
    maxR = Math.max(maxR, e.rect.right);
    maxB = Math.max(maxB, e.rect.bottom);
  }
  if (maxR <= 0 || maxB <= 0) {
    return { width: DEFAULT_SCREEN_W, height: DEFAULT_SCREEN_H };
  }
  return { width: maxR, height: maxB };
}

/**
 * Hit-test: smallest-area rect containing `(x,y)` in device coordinates (right/bottom exclusive).
 */
export function pickNodePathAtDevicePoint(
  entries: WireframeRectEntry[],
  x: number,
  y: number
): string | null {
  const containing = entries.filter((e) => {
    const { left, top, right, bottom } = e.rect;
    return x >= left && x < right && y >= top && y < bottom;
  });
  if (containing.length === 0) return null;
  containing.sort((a, b) => a.area - b.area);
  return containing[0]!.path;
}

/**
 * Capped rect list for wireframe draw + hit-test (same set so picks match visible rects).
 * `drawOrder`: largest area first (painted back), smallest last (on top).
 */
export function prepareWireframeDrawList(
  root: UiNode,
  cap: number = WIREFRAME_RECT_CAP
): {
  entries: WireframeRectEntry[];
  drawOrder: WireframeRectEntry[];
  truncated: boolean;
  screen: { width: number; height: number };
} {
  const all = flattenNodesWithBounds(root);
  const screen = inferScreenSizeFromRects(all);
  const truncated = all.length > cap;
  const entries = truncated ? all.slice(0, cap) : all;
  const drawOrder = [...entries].sort((a, b) => b.area - a.area);
  return { entries, drawOrder, truncated, screen };
}
