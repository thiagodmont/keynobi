/**
 * A UI hierarchy attached to a debug session, read-only: one fixed-height row
 * per shown node (class, resource id, text, bounds), so a tree of thousands
 * of nodes stays responsive. Subtrees collapse. The tree is one tab stop:
 * Up/Down, Home, and End move; Right expands or moves to the first child;
 * Left collapses or moves to the parent; Enter toggles.
 */
import { type JSX, Show, createMemo, createResource, createSignal } from "solid-js";
import type { DebugSessionHierarchyNode } from "@/bindings";
import { formatError, getSessionHierarchy } from "@/lib/tauri-api";
import { Icon, Spinner, VirtualList, type VirtualListHandle } from "@/components/ui";
import styles from "./SessionsDialog.module.css";

export const HIERARCHY_ROW_HEIGHT = 20;
/** Indent per level, in pixels. */
const INDENT = 12;

/** The indexes of the nodes shown when the `collapsed` ones hide their subtrees. */
export function visibleNodes(
  nodes: readonly DebugSessionHierarchyNode[],
  collapsed: ReadonlySet<number>
): number[] {
  const shown: number[] = [];
  let hiddenBelow = Infinity;
  nodes.forEach((node, i) => {
    if (node.depth > hiddenBelow) return;
    hiddenBelow = collapsed.has(i) ? node.depth : Infinity;
    shown.push(i);
  });
  return shown;
}

function hasChildren(nodes: readonly DebugSessionHierarchyNode[], i: number): boolean {
  return (nodes[i + 1]?.depth ?? -1) > nodes[i].depth;
}

function parentOf(nodes: readonly DebugSessionHierarchyNode[], i: number): number | null {
  for (let j = i - 1; j >= 0; j--) {
    if (nodes[j].depth < nodes[i].depth) return j;
  }
  return null;
}

/** `android.widget.TextView` → `TextView`. */
function shortClass(name: string): string {
  return name.slice(name.lastIndexOf(".") + 1) || name;
}

/** `com.example:id/title` → `title`. */
function shortId(id: string): string {
  const at = id.indexOf(":id/");
  return at >= 0 ? id.slice(at + 4) : id;
}

function nodeTitle(node: DebugSessionHierarchyNode): string {
  return [
    node.class,
    node.resourceId && `id: ${node.resourceId}`,
    node.text && `text: ${node.text}`,
    node.contentDesc && `content-desc: ${node.contentDesc}`,
    node.bounds && `bounds: ${node.bounds}`,
  ]
    .filter(Boolean)
    .join("\n");
}

function HierarchyTree(props: {
  seq: number;
  nodes: readonly DebugSessionHierarchyNode[];
}): JSX.Element {
  const [collapsed, setCollapsed] = createSignal<ReadonlySet<number>>(new Set());
  const [active, setActive] = createSignal(0);
  const shown = createMemo(() => visibleNodes(props.nodes, collapsed()));
  let list: VirtualListHandle | undefined;
  let root!: HTMLDivElement;

  const rowId = (i: number) => `session-hierarchy-${props.seq}-${i}`;

  function toggle(i: number, open?: boolean): void {
    if (!hasChildren(props.nodes, i)) return;
    setCollapsed((prev) => {
      const next = new Set(prev);
      const close = open === undefined ? !next.has(i) : !open;
      if (close) next.add(i);
      else next.delete(i);
      return next;
    });
  }

  function moveTo(i: number): void {
    setActive(i);
    const row = root.querySelector<HTMLElement>(`#${rowId(i)}`);
    if (row) row.scrollIntoView?.({ block: "nearest" });
    else list?.scrollToIndex(shown().indexOf(i));
  }

  function handleKeyDown(e: KeyboardEvent): void {
    const rows = shown();
    const at = Math.max(0, rows.indexOf(active()));
    const i = rows[at];
    const handled = (() => {
      switch (e.key) {
        case "ArrowDown":
          moveTo(rows[Math.min(rows.length - 1, at + 1)]);
          return true;
        case "ArrowUp":
          moveTo(rows[Math.max(0, at - 1)]);
          return true;
        case "Home":
          moveTo(rows[0]);
          return true;
        case "End":
          moveTo(rows[rows.length - 1]);
          return true;
        case "ArrowRight":
          if (!hasChildren(props.nodes, i)) return false;
          if (collapsed().has(i)) toggle(i, true);
          else moveTo(i + 1);
          return true;
        case "ArrowLeft": {
          if (hasChildren(props.nodes, i) && !collapsed().has(i)) {
            toggle(i, false);
            return true;
          }
          const parent = parentOf(props.nodes, i);
          if (parent !== null) moveTo(parent);
          return parent !== null;
        }
        case "Enter":
          toggle(i);
          return true;
        default:
          return false;
      }
    })();
    if (handled) e.preventDefault();
  }

  return (
    <div
      ref={root}
      class={styles.hierarchy}
      role="tree"
      aria-label="UI hierarchy"
      tabIndex={0}
      aria-activedescendant={rowId(active())}
      onKeyDown={handleKeyDown}
    >
      <VirtualList
        items={shown()}
        rowHeight={HIERARCHY_ROW_HEIGHT}
        overscan={20}
        class={styles.hierarchyScroll}
        handle={(api) => (list = api)}
        renderRow={(i) => {
          const node = props.nodes[i];
          const parent = hasChildren(props.nodes, i);
          const open = () => !collapsed().has(i);
          const label = node.text || node.contentDesc;
          return (
            <div
              id={rowId(i)}
              role="treeitem"
              aria-level={node.depth + 1}
              aria-expanded={parent ? open() : undefined}
              aria-selected={active() === i ? "true" : "false"}
              class={styles.hierarchyRow}
              style={{ "padding-left": `${4 + node.depth * INDENT}px` }}
              title={nodeTitle(node)}
              onClick={() => {
                setActive(i);
                toggle(i);
              }}
            >
              <span class={styles.hierarchyToggle} aria-hidden="true">
                <Show when={parent}>
                  <Icon name={open() ? "chevron-down" : "chevron-right"} size={12} />
                </Show>
              </span>
              <span class={styles.hierarchyClass}>{shortClass(node.class)}</span>
              <Show when={node.resourceId}>
                <span class={styles.hierarchyId}>#{shortId(node.resourceId)}</span>
              </Show>
              <Show when={label}>
                <span class={styles.hierarchyText}>“{label}”</span>
              </Show>
              <span class={styles.hierarchyBounds}>{node.bounds}</span>
            </div>
          );
        }}
      />
    </div>
  );
}

/** The hierarchy of attachment event `seq`, loaded when shown. */
export function SessionHierarchy(props: { sessionId: string; seq: number }): JSX.Element {
  const [data] = createResource(
    () => ({ id: props.sessionId, seq: props.seq }),
    // Rethrown as an `Error`, which the resource keeps as it is.
    ({ id, seq }) =>
      getSessionHierarchy(id, seq).catch((e: unknown) => {
        throw new Error(formatError(e));
      })
  );
  return (
    <Show
      when={!data.error}
      fallback={<span class={styles.fieldError}>{formatError(data.error)}</span>}
    >
      <Show when={data()} fallback={<Spinner size="sm" />}>
        {(hierarchy) => (
          <div class={styles.capture}>
            <div class={styles.note}>
              <Show when={hierarchy().foregroundActivity}>
                {(activity) => <span class={styles.hierarchyActivity}>{activity()} · </span>}
              </Show>
              {hierarchy().nodes.length === 1 ? "1 node" : `${hierarchy().nodes.length} nodes`}
              <Show when={hierarchy().truncated}>
                {" "}
                · cut to the capture limits, so some nodes or text are missing
              </Show>
            </div>
            <HierarchyTree seq={props.seq} nodes={hierarchy().nodes} />
          </div>
        )}
      </Show>
    </Show>
  );
}
