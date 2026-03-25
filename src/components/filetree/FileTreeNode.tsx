import { createSignal, For, Show, type JSX } from "solid-js";
import type { FileNode } from "@/stores/project.store";
import { editorState } from "@/stores/editor.store";
import { Icon } from "@/components/common/Icon";
import { getDirectoryChildren } from "@/lib/tauri-api";
import { showToast } from "@/components/common/Toast";
import { InlineInput } from "./InlineInput";

// ── Static style constants ────────────────────────────────────────────────────

const CHEVRON_STYLE = {
  display: "inline-flex",
  "align-items": "center",
  width: "12px",
  height: "12px",
  color: "var(--text-muted)",
  "flex-shrink": "0",
  transition: "transform 0.15s ease",
} as const;

const CHEVRON_SPACER_STYLE = {
  width: "12px",
  "flex-shrink": "0",
} as const;

const FILENAME_STYLE = {
  flex: "1",
  overflow: "hidden",
  "text-overflow": "ellipsis",
} as const;

const LOADING_INDICATOR_STYLE = {
  color: "var(--text-muted)",
  "font-size": "10px",
} as const;

// ── File type helpers ─────────────────────────────────────────────────────────

function getExtensionColor(extension?: string): string {
  switch (extension) {
    case "kt":
    case "kts":
      return "#a97bff";
    case "gradle":
      return "#02b10a";
    case "xml":
      return "#f0883e";
    case "json":
      return "#e8c07d";
    case "md":
      return "#519aba";
    case "properties":
      return "#858585";
    case "png":
    case "jpg":
    case "jpeg":
    case "webp":
    case "svg":
      return "#26a7de";
    default:
      return "#7f8fa4";
  }
}

function getExtensionLabel(extension?: string, name?: string): string {
  if (name?.endsWith(".gradle.kts")) return "G";
  switch (extension) {
    case "kt":
      return "K";
    case "xml":
      return "X";
    case "json":
      return "J";
    case "md":
      return "M";
    case "gradle":
      return "G";
    case "properties":
      return "P";
    default:
      return "·";
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

export interface FileTreeNodeProps {
  node: FileNode;
  depth: number;
  onOpenFile: (path: string) => void;
  expandedDirs: () => Set<string>;
  toggleDir: (path: string) => void;
  onContextMenu: (e: MouseEvent, node: FileNode) => void;
  dirCache: Record<string, FileNode[]>;
  onDirLoaded: (path: string, children: FileNode[]) => void;
  /** Path of the node that currently has keyboard focus (for highlight). */
  focusedPath: () => string | null;
  /** Called when this node is clicked to take focus. */
  onFocusNode: (path: string) => void;
  /** Whether this node is being renamed inline. */
  renamingPath: () => string | null;
  /** Callback when inline rename confirms. */
  onRenameConfirm: (oldPath: string, newName: string) => void;
  /** Callback when inline rename cancels. */
  onRenameCancel: () => void;
  /** Inline input state for new file/folder inside this node's directory. */
  inlineNewParent: () => string | null;
  onInlineNewConfirm: (name: string) => void;
  onInlineNewCancel: () => void;
}

export function FileTreeNode(props: FileTreeNodeProps): JSX.Element {
  const isDir = () => props.node.kind === "directory";
  const isExpanded = () => props.expandedDirs().has(props.node.path);
  const isActive = () => editorState.activeFilePath === props.node.path;
  const isFocused = () => props.focusedPath() === props.node.path;
  const isRenaming = () => props.renamingPath() === props.node.path;

  const [loading, setLoading] = createSignal(false);

  const currentChildren = () => {
    const cached = props.dirCache[props.node.path];
    if (cached) return cached;
    return props.node.children ?? [];
  };

  async function expand() {
    if (!isDir()) return;

    const cached = props.dirCache[props.node.path];
    if (!isExpanded() && (!cached || cached.length === 0)) {
      setLoading(true);
      try {
        const fetched = await getDirectoryChildren(props.node.path);
        props.onDirLoaded(props.node.path, fetched);
      } catch (err) {
        showToast(`Failed to load directory: ${String(err)}`, "error");
      } finally {
        setLoading(false);
      }
    }

    props.toggleDir(props.node.path);
  }

  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    props.onFocusNode(props.node.path);
    if (isDir()) expand();
  }

  function handleDblClick(e: MouseEvent) {
    e.stopPropagation();
    if (!isDir()) props.onOpenFile(props.node.path);
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    props.onFocusNode(props.node.path);
    props.onContextMenu(e, props.node);
  }

  const indent = () => props.depth * 12;

  // Show the inline new-file input AFTER this node's children if this is the
  // target directory and it is expanded.
  const showInlineNew = () =>
    isDir() && isExpanded() && props.inlineNewParent() === props.node.path;

  return (
    <div data-tree-path={props.node.path}>
      {/* Node row — either normal or inline rename */}
      <Show
        when={!isRenaming()}
        fallback={
          <InlineInput
            initialValue={props.node.name}
            indent={indent()}
            onConfirm={(name) => props.onRenameConfirm(props.node.path, name)}
            onCancel={props.onRenameCancel}
          />
        }
      >
        <div
          onClick={handleClick}
          onDblClick={handleDblClick}
          onContextMenu={handleContextMenu}
          data-tree-row
          style={{
            display: "flex",
            "align-items": "center",
            gap: "4px",
            padding: `2px 8px 2px ${indent() + 8}px`,
            cursor: isDir() ? "pointer" : "default",
            background: isFocused()
              ? "var(--bg-active)"
              : isActive()
                ? "var(--accent-bg)"
                : "transparent",
            "border-left": isActive()
              ? "1px solid var(--accent)"
              : "1px solid transparent",
            outline: isFocused() ? "1px solid var(--accent)" : "none",
            "outline-offset": "-1px",
            "user-select": "none",
            "font-size": "13px",
            color: "var(--text-primary)",
            height: "22px",
            "white-space": "nowrap",
            overflow: "hidden",
          }}
          onMouseEnter={(e) => {
            if (!isActive() && !isFocused())
              (e.currentTarget as HTMLElement).style.background = "var(--bg-hover)";
          }}
          onMouseLeave={(e) => {
            if (!isActive() && !isFocused())
              (e.currentTarget as HTMLElement).style.background = "transparent";
          }}
          title={props.node.path}
        >
          {/* Chevron */}
          <Show when={isDir()} fallback={<span style={CHEVRON_SPACER_STYLE} />}>
            <span
              style={{
                ...CHEVRON_STYLE,
                transform: isExpanded() ? "rotate(90deg)" : "rotate(0deg)",
              }}
            >
              <Icon name="chevron-right" size={12} />
            </span>
          </Show>

          {/* File/folder type icon */}
          <Show
            when={isDir()}
            fallback={
              <span
                style={{
                  "font-size": "9px",
                  "font-weight": "700",
                  color: getExtensionColor(props.node.extension),
                  width: "12px",
                  "text-align": "center",
                  "flex-shrink": "0",
                }}
              >
                {getExtensionLabel(props.node.extension, props.node.name)}
              </span>
            }
          >
            <Icon name={isExpanded() ? "folder-open" : "folder"} size={14} color="#e8ab65" />
          </Show>

          {/* Filename */}
          <span style={FILENAME_STYLE}>{props.node.name}</span>

          <Show when={loading()}>
            <span style={LOADING_INDICATOR_STYLE}>…</span>
          </Show>
        </div>
      </Show>

      {/* Recursive children */}
      <Show when={isDir() && isExpanded()}>
        <For each={currentChildren()}>
          {(child) => (
            <FileTreeNode
              node={child}
              depth={props.depth + 1}
              onOpenFile={props.onOpenFile}
              expandedDirs={props.expandedDirs}
              toggleDir={props.toggleDir}
              onContextMenu={props.onContextMenu}
              dirCache={props.dirCache}
              onDirLoaded={props.onDirLoaded}
              focusedPath={props.focusedPath}
              onFocusNode={props.onFocusNode}
              renamingPath={props.renamingPath}
              onRenameConfirm={props.onRenameConfirm}
              onRenameCancel={props.onRenameCancel}
              inlineNewParent={props.inlineNewParent}
              onInlineNewConfirm={props.onInlineNewConfirm}
              onInlineNewCancel={props.onInlineNewCancel}
            />
          )}
        </For>

        {/* Inline input for new file/folder — appears after all children */}
        <Show when={showInlineNew()}>
          <InlineInput
            indent={(props.depth + 1) * 12}
            onConfirm={props.onInlineNewConfirm}
            onCancel={props.onInlineNewCancel}
          />
        </Show>
      </Show>
    </div>
  );
}

export default FileTreeNode;
