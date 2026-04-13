import {
  type JSX,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
} from "solid-js";
import type { UiNode } from "@/bindings";
import Icon from "@/components/common/Icon";
import {
  COLLAPSED_WRAPPERS_CLASS,
  buildNodeSummaryLine,
  collapseBoringChains,
  collectSearchMatchPaths,
  defaultExpandDepthForNodeCount,
  filterInteractiveTree,
  filterSearchTree,
  formatBoundsWithSize,
  formatRowSnippet,
  getNodeAtPath,
  inferDominantPackage,
  parentLayoutPath,
  isLikelyFullScreenChrome,
  isMergedTapTargetHeuristic,
  isMinifiedClassName,
  pathOverridesToRevealAncestorPath,
  pathOverridesToRevealPath,
  shortClassName,
  countTreeNodes,
} from "@/lib/ui-hierarchy-display";
import {
  layoutViewerState,
  refreshLayoutHierarchy,
  setAutoRefreshInterval,
  setLayoutHideBoilerplate,
  setLayoutInteractiveOnly,
  setLayoutSearchQuery,
  setLayoutSelectedPath,
  setSearchMatchIndex,
  setSearchMatchPaths,
} from "@/stores/layoutViewer.store";
import { selectedDevice } from "@/stores/device.store";
import { LayoutWireframe } from "@/components/ui-hierarchy/LayoutWireframe";
import { layoutDetailGetNode } from "@/components/ui-hierarchy/layout-detail-get-node";

function displayRoot(): UiNode | null {
  const snap = layoutViewerState.snapshot;
  if (!snap) return null;
  let r: UiNode = snap.root;
  if (layoutViewerState.hideBoilerplate) {
    r = collapseBoringChains(r);
  }
  if (layoutViewerState.interactiveOnly) {
    const f = filterInteractiveTree(r);
    if (!f) return null;
    r = f;
  }
  const q = layoutViewerState.searchQuery.trim();
  if (q) {
    const f = filterSearchTree(r, q);
    return f;
  }
  return r;
}

async function copyToClipboard(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // ignore
  }
}

function HierarchyTree(props: {
  node: UiNode;
  path: string;
  depth: number;
  dominantPackage: string | null;
  interactiveOnly: boolean;
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
  isOpen: (path: string, depth: number) => boolean;
  toggle: (path: string, depth: number) => void;
}): JSX.Element {
  const hasChildren = () => props.node.children.length > 0;
  const open = () => props.isOpen(props.path, props.depth);
  const selected = () => props.selectedPath === props.path;
  const snippet = () => formatRowSnippet(props.node, props.interactiveOnly);
  const dimChrome = () => isLikelyFullScreenChrome(props.node);
  const showPkg = () =>
    props.node.package.length > 0 &&
    props.dominantPackage !== null &&
    props.node.package !== props.dominantPackage;

  return (
    <div style={{ "margin-left": props.depth === 0 ? "0" : "14px" }}>
      <div
        data-layout-path={props.path}
        role="treeitem"
        aria-expanded={hasChildren() ? open() : undefined}
        aria-selected={selected()}
        aria-level={props.depth + 1}
        tabIndex={selected() ? 0 : -1}
        style={{
          display: "flex",
          "align-items": "flex-start",
          gap: "4px",
          "font-size": "12px",
          "line-height": "1.4",
          padding: "3px 4px",
          "border-radius": "4px",
          cursor: "pointer",
          background: selected() ? "var(--bg-secondary)" : "transparent",
          outline: selected() ? "1px solid var(--accent)" : "none",
          opacity: dimChrome() ? 0.72 : 1,
        }}
        onClick={() => props.onSelectPath(props.path)}
      >
        <Show
          when={hasChildren()}
          fallback={<span style={{ width: "16px", "flex-shrink": "0" }} />}
        >
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              props.toggle(props.path, props.depth);
            }}
            style={{
              background: "none",
              border: "none",
              padding: "0",
              cursor: "pointer",
              color: "var(--text-muted)",
              width: "16px",
              "flex-shrink": "0",
            }}
            title={open() ? "Collapse" : "Expand"}
          >
            <Icon name={open() ? "chevron-down" : "chevron-right"} size={14} />
          </button>
        </Show>
        <div style={{ "min-width": "0", flex: "1" }}>
          <div
            style={{
              display: "flex",
              "flex-wrap": "wrap",
              "align-items": "baseline",
              gap: "6px",
            }}
          >
            <span
              style={{
                "font-family": "var(--font-mono, ui-monospace, monospace)",
                "font-weight": "600",
                color: "var(--text-primary)",
              }}
            >
              {shortClassName(props.node.class)}
            </span>
            <Show when={props.node.resourceId.length > 0}>
              <span
                title={props.node.resourceId}
                style={{
                  "font-size": "10px",
                  padding: "1px 6px",
                  "border-radius": "4px",
                  background: "var(--bg-secondary)",
                  color: "var(--accent)",
                  border: "1px solid var(--border)",
                  "max-width": "100%",
                  overflow: "hidden",
                  "text-overflow": "ellipsis",
                  "white-space": "nowrap",
                }}
              >
                {props.node.resourceId.includes("/")
                  ? props.node.resourceId.split("/").pop()
                  : props.node.resourceId}
              </span>
            </Show>
            <Show when={isMinifiedClassName(props.node.class)}>
              <span
                style={{
                  "font-size": "9px",
                  padding: "0 4px",
                  "border-radius": "3px",
                  background: "rgba(251,191,36,0.15)",
                  color: "var(--warning, #fbbf24)",
                }}
                title="Short class name — likely minified (R8/Compose); see full class in detail"
              >
                minified
              </span>
            </Show>
            <Show when={props.node.isComposeHeuristic}>
              <span
                style={{
                  "font-size": "9px",
                  color: "var(--accent)",
                }}
                title="Likely Jetpack Compose container (heuristic)"
              >
                Compose
              </span>
            </Show>
            <Show when={isMergedTapTargetHeuristic(props.node)}>
              <span
                style={{
                  "font-size": "9px",
                  color: "var(--text-muted)",
                }}
                title="Clickable parent with labeled children — common Compose semantics merge"
              >
                merged target
              </span>
            </Show>
            <Show when={props.node.selected}>
              <span
                style={{
                  "font-size": "9px",
                  padding: "0 4px",
                  "border-radius": "3px",
                  background: "rgba(96,165,250,0.2)",
                  color: "var(--accent)",
                }}
                title="UI Automator selected (e.g. tab)"
              >
                selected
              </span>
            </Show>
            <Show when={props.node.class === COLLAPSED_WRAPPERS_CLASS}>
              <span style={{ "font-size": "10px", color: "var(--text-muted)" }}>
                (collapsed chain)
              </span>
            </Show>
          </div>
          <Show when={snippet().text.length > 0}>
            <div
              style={{
                "font-size": "11px",
                color: "var(--text-primary)",
                "margin-top": "2px",
                "word-break": "break-word",
              }}
            >
              {snippet().text.startsWith("→ ") ? (
                <span style={{ color: "var(--text-muted)" }}>{snippet().text}</span>
              ) : (
                <span>&quot;{snippet().text}&quot;</span>
              )}
            </div>
          </Show>
          <Show when={snippet().desc.length > 0}>
            <div
              style={{
                "font-size": "11px",
                color: "var(--text-muted)",
                "margin-top": "2px",
                "word-break": "break-word",
              }}
              title={props.node.contentDesc}
            >
              desc: {snippet().desc}
            </div>
          </Show>
          <div style={{ color: "var(--text-muted)", "font-size": "11px", "margin-top": "2px" }}>
            {formatBoundsWithSize(props.node.bounds)}
            <Show when={showPkg()}>
              <span style={{ "margin-left": "8px" }}>pkg: {props.node.package}</span>
            </Show>
            <Show when={props.node.clickable}>
              <span style={{ "margin-left": "8px" }}>clickable</span>
            </Show>
            <Show when={props.node.editable}>
              <span style={{ "margin-left": "8px" }}>editable</span>
            </Show>
            <Show when={props.node.scrollable}>
              <span style={{ "margin-left": "8px" }}>scrollable</span>
            </Show>
            <Show when={props.node.checkable || props.node.checked}>
              <span style={{ "margin-left": "8px" }}>
                {props.node.checked ? "checked" : "checkable"}
              </span>
            </Show>
          </div>
        </div>
      </div>
      <Show when={hasChildren() && open()}>
        <div role="group">
        <For each={props.node.children}>
          {(child, i) => (
              <HierarchyTree
                node={child}
                path={props.path === "" ? String(i()) : `${props.path}.${i()}`}
                depth={props.depth + 1}
                dominantPackage={props.dominantPackage}
                interactiveOnly={props.interactiveOnly}
                selectedPath={props.selectedPath}
                onSelectPath={props.onSelectPath}
                isOpen={props.isOpen}
                toggle={props.toggle}
              />
          )}
        </For>
        </div>
      </Show>
    </div>
  );
}

/** Detail panel reads `layoutViewerState.selectedLayoutPath` via memos so it stays in sync after Find parent (Solid `Show` children are not always re-run when `when` stays truthy). */
function NodeDetailPanel(props: {
  getNode: () => UiNode;
  getNodeForPath: (path: string) => UiNode | null;
}): JSX.Element {
  const parentPath = createMemo(() => {
    const p = layoutViewerState.selectedLayoutPath;
    if (p === null) {
      return null;
    }
    return parentLayoutPath(p);
  });

  /** Build the ancestor path list from root to the selected node. */
  const breadcrumbSegments = createMemo((): Array<{ path: string; label: string }> => {
    const p = layoutViewerState.selectedLayoutPath;
    if (p === null) return [];
    const paths: string[] = [""];
    if (p !== "") {
      const parts = p.split(".");
      for (let i = 0; i < parts.length; i++) {
        paths.push(parts.slice(0, i + 1).join("."));
      }
    }
    return paths.map((ap) => {
      const node = props.getNodeForPath(ap);
      return { path: ap, label: node ? shortClassName(node.class) : ap === "" ? "root" : ap };
    });
  });

  const flagRows = (): [string, boolean][] => {
    const n = props.getNode();
    return [
      ["clickable", n.clickable],
      ["enabled", n.enabled],
      ["focusable", n.focusable],
      ["focused", n.focused],
      ["scrollable", n.scrollable],
      ["longClickable", n.longClickable],
      ["password", n.password],
      ["checkable", n.checkable],
      ["checked", n.checked],
      ["editable", n.editable],
      ["selected", n.selected],
      ["composeHeuristic", n.isComposeHeuristic],
    ];
  };
  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        gap: "10px",
        height: "100%",
        overflow: "auto",
        padding: "8px 10px",
        "font-size": "11px",
        color: "var(--text-primary)",
      }}
    >
      {/* Breadcrumb ancestor path */}
      <Show when={breadcrumbSegments().length > 1}>
        <div
          style={{
            display: "flex",
            "flex-wrap": "nowrap",
            "align-items": "center",
            gap: "2px",
            "overflow-x": "auto",
            "padding-bottom": "2px",
          }}
        >
          <For each={breadcrumbSegments()}>
            {(seg, i) => {
              const isLast = () => i() === breadcrumbSegments().length - 1;
              return (
                <>
                  <button
                    type="button"
                    title={seg.path === "" ? "(root)" : seg.path}
                    onClick={() => setLayoutSelectedPath(seg.path)}
                    style={{
                      background: "none",
                      border: "none",
                      padding: "1px 3px",
                      "font-size": "10px",
                      "font-family": "var(--font-mono, ui-monospace, monospace)",
                      "border-radius": "3px",
                      cursor: "pointer",
                      color: isLast() ? "var(--text-primary)" : "var(--accent)",
                      "font-weight": isLast() ? "600" : "400",
                      "white-space": "nowrap",
                      "flex-shrink": 0,
                    }}
                  >
                    {seg.label}
                  </button>
                  <Show when={!isLast()}>
                    <span
                      style={{
                        "font-size": "9px",
                        color: "var(--text-muted)",
                        "flex-shrink": 0,
                        "line-height": "1",
                      }}
                    >
                      ›
                    </span>
                  </Show>
                </>
              );
            }}
          </For>
        </div>
      </Show>
      <div style={{ "font-weight": "600", "font-size": "12px" }}>Node detail</div>
      <div>
        <div style={{ color: "var(--text-muted)", "margin-bottom": "2px" }}>Class</div>
        <div
          style={{
            "font-family": "var(--font-mono, ui-monospace, monospace)",
            "word-break": "break-all",
          }}
        >
          {props.getNode().class}
        </div>
      </div>
      <div>
        <div style={{ color: "var(--text-muted)", "margin-bottom": "2px" }}>resource-id</div>
        <div style={{ "word-break": "break-all" }}>{props.getNode().resourceId || "—"}</div>
      </div>
      <div>
        <div style={{ color: "var(--text-muted)", "margin-bottom": "2px" }}>Text</div>
        <div style={{ "white-space": "pre-wrap", "word-break": "break-word" }}>
          {props.getNode().text || "—"}
        </div>
      </div>
      <div>
        <div style={{ color: "var(--text-muted)", "margin-bottom": "2px" }}>content-desc</div>
        <div style={{ "white-space": "pre-wrap", "word-break": "break-word" }}>
          {props.getNode().contentDesc || "—"}
        </div>
      </div>
      <div>
        <div style={{ color: "var(--text-muted)", "margin-bottom": "2px" }}>Bounds</div>
        <div>{formatBoundsWithSize(props.getNode().bounds)}</div>
      </div>
      <div>
        <div style={{ color: "var(--text-muted)", "margin-bottom": "4px" }}>Flags</div>
        <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
          <For each={flagRows()}>
            {(row) => (
              <span
                style={{
                  padding: "2px 6px",
                  "border-radius": "4px",
                  background: row[1] ? "var(--bg-secondary)" : "transparent",
                  opacity: row[1] ? 1 : 0.45,
                  border: "1px solid var(--border)",
                }}
              >
                {row[0]}: {String(row[1])}
              </span>
            )}
          </For>
        </div>
      </div>
      <div style={{ color: "var(--text-muted)" }}>Package: {props.getNode().package || "—"}</div>
      <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
        <button
          type="button"
          style={miniBtnStyle}
          onClick={() => void copyToClipboard(props.getNode().bounds)}
        >
          Copy bounds
        </button>
        <button
          type="button"
          style={miniBtnStyle}
          onClick={() => void copyToClipboard(props.getNode().resourceId)}
        >
          Copy id
        </button>
        <button
          type="button"
          style={miniBtnStyle}
          onClick={() => void copyToClipboard(buildNodeSummaryLine(props.getNode()))}
        >
          Copy summary
        </button>
        <button
          type="button"
          style={{
            ...miniBtnStyle,
            ...(parentPath() === null
              ? { opacity: 0.45, cursor: "not-allowed" as const }
              : {}),
          }}
          disabled={parentPath() === null}
          title={
            parentPath() === null
              ? "The display root has no parent in this tree"
              : "Select the direct parent node in the tree and wireframe (repeat until root)"
          }
          onClick={() => {
            const cur = layoutViewerState.selectedLayoutPath;
            if (cur === null) {
              return;
            }
            const pp = parentLayoutPath(cur);
            if (pp !== null) {
              setLayoutSelectedPath(pp);
            }
          }}
        >
          Find parent
        </button>
      </div>
      <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "10px" }}>
        Scrollable and other flags can be wrong in dumps (e.g. HorizontalScrollView sometimes reports
        scrollable=false). Treat as hints, not ground truth.
      </p>
    </div>
  );
}

export function LayoutViewerPanel(): JSX.Element {
  const root = createMemo(displayRoot);
  const [pathOverrides, setPathOverrides] = createSignal<Record<string, boolean>>({});
  const [globalExpand, setGlobalExpand] = createSignal<"auto" | "all" | "none">("auto");

  const dominantPkg = createMemo(() => {
    const s = layoutViewerState.snapshot?.root;
    return s ? inferDominantPackage(s) : null;
  });

  const expandBasis = createMemo(() => {
    const s = layoutViewerState.snapshot?.root;
    if (!s) return { depth: 3 };
    let t: UiNode = s;
    if (layoutViewerState.hideBoilerplate) {
      t = collapseBoringChains(t);
    }
    const n = countTreeNodes(t);
    return { depth: defaultExpandDepthForNodeCount(n) };
  });

  const searchRevealOverrides = createMemo(() => {
    const q = layoutViewerState.searchQuery.trim();
    const paths = layoutViewerState.searchMatchPaths;
    const idx = layoutViewerState.searchMatchIndex;
    if (!q || paths.length === 0) return {} as Record<string, boolean>;
    const p = paths[idx];
    if (p === undefined) return {};
    return pathOverridesToRevealPath(p);
  });

  const isOpen = (path: string, depth: number): boolean => {
    const g = globalExpand();
    if (g === "all") return true;
    if (g === "none") return false;
    const po = pathOverrides();
    if (po[path] !== undefined) return po[path]!;
    const sr = searchRevealOverrides();
    if (sr[path] !== undefined) return sr[path]!;
    return depth < expandBasis().depth;
  };

  const toggle = (path: string, depth: number): void => {
    const g = globalExpand();
    const cur =
      g === "all" ? true : g === "none" ? false : pathOverrides()[path] ?? depth < expandBasis().depth;
    setGlobalExpand("auto");
    setPathOverrides((prev) => ({ ...prev, [path]: !cur }));
  };

  const expandAll = (): void => {
    setPathOverrides({});
    setGlobalExpand("all");
  };

  const collapseAll = (): void => {
    setPathOverrides({});
    setGlobalExpand("none");
  };

  createEffect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    layoutViewerState.snapshot?.capturedAt; // reactive dependency: reset tree state on each new snapshot
    setPathOverrides({});
    setGlobalExpand("auto");
  });

  createEffect(() => {
    const r = root();
    const q = layoutViewerState.searchQuery.trim();
    if (!r || !q) {
      setSearchMatchPaths([]);
      setSearchMatchIndex(0);
      return;
    }
    const paths = collectSearchMatchPaths(r, q);
    setSearchMatchPaths(paths);
    const idx = layoutViewerState.searchMatchIndex;
    if (paths.length === 0) {
      setSearchMatchIndex(0);
    } else if (idx >= paths.length) {
      setSearchMatchIndex(0);
    }
  });

  createEffect(() => {
    const r = root();
    const p = layoutViewerState.selectedLayoutPath;
    if (p === null || !r) return;
    if (!getNodeAtPath(r, p)) {
      setLayoutSelectedPath(null);
    }
  });

  const selectedNode = createMemo(() => {
    const r = root();
    const p = layoutViewerState.selectedLayoutPath;
    if (!r || p === null) return null;
    return getNodeAtPath(r, p);
  });

  const searchMatchCount = () => layoutViewerState.searchMatchPaths.length;
  const searchMatchLabel = () => {
    const n = searchMatchCount();
    if (n === 0) return "";
    return `${layoutViewerState.searchMatchIndex + 1} / ${n}`;
  };

  const goSearchMatch = (delta: number): void => {
    const paths = layoutViewerState.searchMatchPaths;
    if (paths.length === 0) return;
    let i = layoutViewerState.searchMatchIndex + delta;
    if (i < 0) i = paths.length - 1;
    if (i >= paths.length) i = 0;
    setSearchMatchIndex(i);
    const p = paths[i];
    if (p !== undefined) {
      setLayoutSelectedPath(p);
    }
  };

  let treeScrollContainer: HTMLDivElement | undefined;

  createEffect(() => {
    const p = layoutViewerState.selectedLayoutPath;
    if (p === null) return;

    const reveal = pathOverridesToRevealAncestorPath(p);
    if (Object.keys(reveal).length > 0) {
      setGlobalExpand("auto");
      setPathOverrides((prev) => ({ ...prev, ...reveal }));
    }

    const el = treeScrollContainer;
    if (!el) return;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const sel = `[data-layout-path="${CSS.escape(p)}"]`;
        el.querySelector(sel)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
      });
    });
  });

  createEffect(() => {
    const ms = layoutViewerState.autoRefreshIntervalMs;
    if (!ms) return;
    const id = setInterval(() => {
      if (!layoutViewerState.loading) {
        void refreshLayoutHierarchy();
      }
    }, ms);
    onCleanup(() => clearInterval(id));
  });

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        overflow: "hidden",
        background: "var(--bg-primary)",
      }}
    >
      <div
        style={{
          display: "flex",
          "flex-wrap": "wrap",
          "align-items": "center",
          gap: "8px",
          padding: "8px 12px",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        <button
          type="button"
          disabled={layoutViewerState.loading}
          onClick={() => void refreshLayoutHierarchy()}
          style={{
            padding: "4px 12px",
            "font-size": "12px",
            background: "var(--accent)",
            color: "var(--bg-primary)",
            border: "none",
            "border-radius": "4px",
            cursor: layoutViewerState.loading ? "wait" : "pointer",
            opacity: layoutViewerState.loading ? 0.7 : 1,
          }}
        >
          {layoutViewerState.loading ? "Refreshing…" : "Refresh"}
        </button>
        <label style={labelRowStyle}>
          <input
            type="checkbox"
            checked={layoutViewerState.interactiveOnly}
            onChange={(e) => setLayoutInteractiveOnly(e.currentTarget.checked)}
          />
          Interactive only
        </label>
        <label style={labelRowStyle}>
          <input
            type="checkbox"
            checked={layoutViewerState.hideBoilerplate}
            onChange={(e) => setLayoutHideBoilerplate(e.currentTarget.checked)}
          />
          Hide boilerplate
        </label>
        <Show when={layoutViewerState.hideBoilerplate}>
          <span
            title="When 'Hide boilerplate' is on, the tree collapses wrapper chains into synthetic nodes. Tree paths shown here may differ from MCP tool paths (find_ui_elements, ui_tap, etc.)."
            style={{
              "font-size": "10px",
              padding: "2px 6px",
              "border-radius": "3px",
              background: "rgba(251,191,36,0.15)",
              color: "var(--warning, #fbbf24)",
              cursor: "help",
              "flex-shrink": 0,
            }}
          >
            ⚠ Paths differ from MCP
          </span>
        </Show>
        <input
          type="search"
          placeholder="Filter…"
          value={layoutViewerState.searchQuery}
          onInput={(e) => setLayoutSearchQuery(e.currentTarget.value)}
          style={{
            flex: "1",
            "min-width": "140px",
            padding: "4px 8px",
            "font-size": "12px",
            background: "var(--bg-secondary)",
            color: "var(--text-primary)",
            border: "1px solid var(--border)",
            "border-radius": "4px",
          }}
        />
        <Show when={layoutViewerState.searchQuery.trim().length > 0}>
          <button type="button" style={toolbarBtnStyle} onClick={() => goSearchMatch(-1)}>
            Prev
          </button>
          <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>{searchMatchLabel()}</span>
          <button type="button" style={toolbarBtnStyle} onClick={() => goSearchMatch(1)}>
            Next
          </button>
        </Show>
        <button type="button" onClick={expandAll} style={toolbarBtnStyle}>
          Expand all
        </button>
        <button type="button" onClick={collapseAll} style={toolbarBtnStyle}>
          Collapse all
        </button>
        <select
          title="Auto-refresh interval"
          value={layoutViewerState.autoRefreshIntervalMs === null ? "off" : String(layoutViewerState.autoRefreshIntervalMs)}
          onChange={(e) => {
            const v = e.currentTarget.value;
            setAutoRefreshInterval(v === "off" ? null : Number(v));
          }}
          style={{
            padding: "4px 6px",
            "font-size": "12px",
            background: "var(--bg-secondary)",
            color: layoutViewerState.autoRefreshIntervalMs !== null ? "var(--accent)" : "var(--text-muted)",
            border: "1px solid var(--border)",
            "border-radius": "4px",
            cursor: "pointer",
          }}
        >
          <option value="off">Auto off</option>
          <option value="2000">Auto 2s</option>
          <option value="5000">Auto 5s</option>
          <option value="10000">Auto 10s</option>
        </select>
      </div>

      <div
        style={{
          padding: "4px 12px",
          "font-size": "11px",
          color: "var(--text-muted)",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        <Show when={selectedDevice()} fallback={<span>No device selected — use the Devices sidebar.</span>}>
          {(d) => (
            <span>
              Device: <span style={{ color: "var(--text-primary)" }}>{d().name}</span> ({d().serial})
              <Show when={dominantPkg()}>
                {(pkgName) => (
                  <span style={{ "margin-left": "10px" }}>
                    Package:{" "}
                    <span style={{ color: "var(--text-primary)", "font-family": "var(--font-mono)" }}>
                      {pkgName()}
                    </span>{" "}
                    <span style={{ opacity: 0.8 }}>(omitted on rows when unchanged)</span>
                  </span>
                )}
              </Show>
            </span>
          )}
        </Show>
      </div>

      <Show when={layoutViewerState.error}>
        {(err) => (
          <div
            style={{
              margin: "8px 12px",
              padding: "8px",
              background: "rgba(248,113,113,0.12)",
              color: "var(--error, #f87171)",
              "font-size": "12px",
              "border-radius": "4px",
            }}
          >
            {err()}
          </div>
        )}
      </Show>

      <div
        style={{
          flex: "1",
          display: "flex",
          "min-height": "0",
          overflow: "hidden",
        }}
      >
        <Show
          when={layoutViewerState.loading}
          fallback={
            <Show
              when={root()}
              keyed
              fallback={
                <div
                  style={{
                    flex: "1",
                    "min-width": "0",
                    overflow: "auto",
                    padding: "8px 12px",
                    color: "var(--text-muted)",
                    "font-size": "12px",
                  }}
                >
                  <Show
                    when={layoutViewerState.snapshot}
                    fallback={
                      <p style={{ margin: 0 }}>
                        Press <strong>Refresh</strong> to capture the focused window hierarchy from the
                        device (native Views and Jetpack Compose via accessibility).
                      </p>
                    }
                  >
                    <p style={{ margin: "0 0 8px 0" }}>
                      No nodes match the current filter — try turning off filters, clear the search box, or
                      refresh after navigating on the device.
                    </p>
                  </Show>
                  <p style={{ margin: "8px 0 0 0", "font-size": "11px" }}>
                    Shallow trees are normal for Compose when semantics are merged or cleared. See{" "}
                    <a
                      href="https://developer.android.com/develop/ui/compose/accessibility/semantics"
                      target="_blank"
                      rel="noreferrer"
                      style={{ color: "var(--accent)" }}
                    >
                      Semantics in Compose
                    </a>
                    .
                  </p>
                </div>
              }
            >
              {(r) => (
                <>
                  <LayoutWireframe
                    root={r}
                    selectedPath={layoutViewerState.selectedLayoutPath}
                    onSelectPath={(p) => setLayoutSelectedPath(p)}
                    screenshotB64={layoutViewerState.snapshot?.screenshotB64 ?? undefined}
                  />
                  <div
                    ref={(el) => {
                      treeScrollContainer = el;
                    }}
                    style={{ flex: "1", "min-width": "0", overflow: "auto", padding: "8px 12px" }}
                  >
                    <Show when={layoutViewerState.snapshot?.truncated}>
                      <div
                        style={{
                          "font-size": "11px",
                          color: "var(--warning, #fbbf24)",
                          "margin-bottom": "8px",
                        }}
                      >
                        Snapshot was truncated (size or node limits). Check warnings in metadata below.
                      </div>
                    </Show>
                    <div role="tree" aria-label="UI hierarchy tree">
                    <HierarchyTree
                      node={r}
                      path=""
                      depth={0}
                      dominantPackage={dominantPkg()}
                      interactiveOnly={layoutViewerState.interactiveOnly}
                      selectedPath={layoutViewerState.selectedLayoutPath}
                      onSelectPath={(p) => setLayoutSelectedPath(p)}
                      isOpen={isOpen}
                      toggle={toggle}
                    />
                    </div>
                    <Show when={(layoutViewerState.snapshot?.warnings.length ?? 0) > 0}>
                      <div
                        style={{
                          "margin-top": "16px",
                          padding: "8px",
                          background: "var(--bg-secondary)",
                          "border-radius": "4px",
                          "font-size": "11px",
                          color: "var(--text-muted)",
                        }}
                      >
                        <div style={{ "font-weight": "600", "margin-bottom": "4px" }}>Warnings</div>
                        <For each={layoutViewerState.snapshot?.warnings ?? []}>
                          {(w) => <div style={{ "margin-top": "2px" }}>{w}</div>}
                        </For>
                      </div>
                    </Show>
                    <div
                      style={{
                        "margin-top": "12px",
                        "font-size": "11px",
                        color: "var(--text-muted)",
                      }}
                    >
                      <div>Captured: {layoutViewerState.snapshot?.capturedAt}</div>
                      <div>Screen hash: {layoutViewerState.snapshot?.screenHash}</div>
                      <div>Interactive nodes: {layoutViewerState.snapshot?.interactiveCount}</div>
                      <Show when={layoutViewerState.snapshot?.foregroundActivity}>
                        {(fa) => (
                          <div style={{ "margin-top": "4px", "word-break": "break-all" }}>{fa()}</div>
                        )}
                      </Show>
                    </div>
                  </div>
                </>
              )}
            </Show>
          }
        >
          <div
            style={{
              flex: "1",
              "min-width": "0",
              overflow: "auto",
              padding: "8px 12px",
              color: "var(--text-muted)",
              "font-size": "12px",
            }}
          >
            Loading…
          </div>
        </Show>

        <div
          style={{
            width: "300px",
            "max-width": "38vw",
            "flex-shrink": 0,
            "border-left": "1px solid var(--border)",
            "min-height": "0",
            overflow: "hidden",
            background: "var(--bg-secondary)",
          }}
        >
          <Show
            when={selectedNode()}
            keyed
            fallback={
              <div style={{ padding: "12px", color: "var(--text-muted)", "font-size": "12px" }}>
                Select a node in the tree to inspect fields and copy bounds or resource-id.
              </div>
            }
          >
            <NodeDetailPanel
              getNode={layoutDetailGetNode(selectedNode)}
              getNodeForPath={(p) => {
                const r = root();
                return r ? getNodeAtPath(r, p) : null;
              }}
            />
          </Show>
        </div>
      </div>

      <Show when={layoutViewerState.snapshot}>
        {(snap) => (
          <Show when={snap().commandLog.length > 0}>
            <div
              style={{
                "flex-shrink": 0,
                "border-top": "1px solid var(--border)",
                "max-height": "min(220px, 38vh)",
                overflow: "auto",
                padding: "8px 12px",
                background: "var(--bg-secondary)",
              }}
            >
              <div
                style={{
                  "font-size": "11px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                  color: "var(--text-muted)",
                }}
              >
                ADB commands (this capture)
              </div>
              <For each={snap().commandLog}>
                {(cmd) => (
                  <pre
                    style={{
                      margin: "0 0 6px 0",
                      "font-size": "10px",
                      "line-height": "1.35",
                      "font-family": "var(--font-mono, ui-monospace, monospace)",
                      "white-space": "pre-wrap",
                      "word-break": "break-all",
                      color: "var(--text-primary)",
                    }}
                  >
                    {cmd}
                  </pre>
                )}
              </For>
            </div>
          </Show>
        )}
      </Show>
    </div>
  );
}

const toolbarBtnStyle: JSX.CSSProperties = {
  padding: "4px 10px",
  "font-size": "12px",
  background: "var(--bg-secondary)",
  color: "var(--text-primary)",
  border: "1px solid var(--border)",
  "border-radius": "4px",
  cursor: "pointer",
};

const labelRowStyle: JSX.CSSProperties = {
  display: "flex",
  "align-items": "center",
  gap: "6px",
  "font-size": "12px",
  color: "var(--text-muted)",
  cursor: "pointer",
};

const miniBtnStyle: JSX.CSSProperties = {
  padding: "4px 8px",
  "font-size": "10px",
  background: "var(--bg-primary)",
  color: "var(--text-primary)",
  border: "1px solid var(--border)",
  "border-radius": "4px",
  cursor: "pointer",
};
