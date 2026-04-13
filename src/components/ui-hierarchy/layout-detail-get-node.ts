import type { UiNode } from "@/bindings";

/**
 * Builds the `getNode` callback for {@link NodeDetailPanel}.
 *
 * Do **not** pass `Show`'s render-prop argument as `getNode` (e.g. `getNode={n}` or
 * `getNode={() => n()}`): in Solid 1.9 + Vite, that value is a props proxy, not a callable
 * accessor — you get runtime errors like "`n` is not a function" / "`getNode` is not a function".
 * Always read the selected node from a memo or `() => selectedNode()` in this closure instead.
 */
export function layoutDetailGetNode(getSelected: () => UiNode | null): () => UiNode {
  return () => {
    const node = getSelected();
    if (node === null) {
      throw new Error("layoutDetailGetNode: expected a selected node");
    }
    return node;
  };
}
