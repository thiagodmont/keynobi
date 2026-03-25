import {
  createSignal,
  For,
  Show,
  type JSX,
} from "solid-js";
import type { FileNode } from "@/stores/project.store";
import { editorState } from "@/stores/editor.store";
import { Icon } from "@/components/common/Icon";
import { getDirectoryChildren } from "@/lib/tauri-api";

interface FileTreeNodeProps {
  node: FileNode;
  depth: number;
  onOpenFile: (path: string) => void;
  expandedDirs: () => Set<string>;
  toggleDir: (path: string) => void;
  onContextMenu: (e: MouseEvent, node: FileNode) => void;
}

function getFileIconColor(extension?: string): string {
  switch (extension) {
    case "kt":
      return "#a97bff";
    case "kts":
      return "#a97bff";
    case "xml":
      return "#f0883e";
    case "json":
      return "#e8c07d";
    case "md":
      return "#519aba";
    case "gradle":
      return "#02b10a";
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

function getFileIconLabel(extension?: string, name?: string): string {
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
    default:
      return "·";
  }
}

export function FileTreeNode(props: FileTreeNodeProps): JSX.Element {
  const isDir = () => props.node.kind === "directory";
  const isExpanded = () => props.expandedDirs().has(props.node.path);
  const isActive = () => editorState.activeFilePath === props.node.path;

  const [children, setChildren] = createSignal<FileNode[]>(
    props.node.children ?? []
  );
  const [loading, setLoading] = createSignal(false);
  const [renaming] = createSignal(false);
  const [renameValue, setRenameValue] = createSignal("");

  async function expand() {
    if (!isDir()) return;

    if (!isExpanded() && children().length === 0) {
      setLoading(true);
      try {
        const fetched = await getDirectoryChildren(props.node.path);
        setChildren(fetched);
      } catch (e) {
        console.error("Failed to load children:", e);
      } finally {
        setLoading(false);
      }
    }
    props.toggleDir(props.node.path);
  }

  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    if (isDir()) {
      expand();
    }
  }

  function handleDblClick(e: MouseEvent) {
    e.stopPropagation();
    if (!isDir()) {
      props.onOpenFile(props.node.path);
    }
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    props.onContextMenu(e, props.node);
  }

  const indent = props.depth * 12;

  return (
    <div>
      <div
        onClick={handleClick}
        onDblClick={handleDblClick}
        onContextMenu={handleContextMenu}
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: `2px 8px 2px ${indent + 8}px`,
          cursor: isDir() ? "pointer" : "default",
          background: isActive()
            ? "var(--accent-bg)"
            : "transparent",
          "border-left": isActive()
            ? "1px solid var(--accent)"
            : "1px solid transparent",
          "user-select": "none",
          "font-size": "13px",
          color: "var(--text-primary)",
          height: "22px",
          "white-space": "nowrap",
          overflow: "hidden",
        }}
        onMouseEnter={(e) => {
          if (!isActive())
            (e.currentTarget as HTMLElement).style.background =
              "var(--bg-hover)";
        }}
        onMouseLeave={(e) => {
          if (!isActive())
            (e.currentTarget as HTMLElement).style.background = "transparent";
        }}
        title={props.node.path}
      >
        {/* Chevron for directories */}
        <Show when={isDir()}>
          <span
            style={{
              display: "inline-flex",
              "align-items": "center",
              width: "12px",
              height: "12px",
              color: "var(--text-muted)",
              "flex-shrink": "0",
              transition: "transform 0.15s ease",
              transform: isExpanded() ? "rotate(90deg)" : "rotate(0deg)",
            }}
          >
            <Icon name="chevron-right" size={12} />
          </span>
        </Show>

        {/* File offset when not a dir */}
        <Show when={!isDir()}>
          <span style={{ width: "12px", "flex-shrink": "0" }} />
        </Show>

        {/* Icon */}
        <Show
          when={isDir()}
          fallback={
            <span
              style={{
                "font-size": "9px",
                "font-weight": "700",
                color: getFileIconColor(props.node.extension),
                width: "12px",
                "text-align": "center",
                "flex-shrink": "0",
              }}
            >
              {getFileIconLabel(props.node.extension, props.node.name)}
            </span>
          }
        >
          <Icon
            name={isExpanded() ? "folder-open" : "folder"}
            size={14}
            color="#e8ab65"
          />
        </Show>

        {/* Name */}
        <Show
          when={!renaming()}
          fallback={
            <input
              value={renameValue()}
              onInput={(e) => setRenameValue(e.currentTarget.value)}
              onClick={(e) => e.stopPropagation()}
              style={{
                flex: "1",
                background: "var(--bg-active)",
                border: "1px solid var(--accent)",
                color: "var(--text-primary)",
                "font-size": "13px",
                padding: "0 4px",
                outline: "none",
              }}
            />
          }
        >
          <span
            style={{
              flex: "1",
              overflow: "hidden",
              "text-overflow": "ellipsis",
            }}
          >
            {props.node.name}
          </span>
        </Show>

        <Show when={loading()}>
          <span style={{ color: "var(--text-muted)", "font-size": "10px" }}>
            ...
          </span>
        </Show>
      </div>

      {/* Children */}
      <Show when={isDir() && isExpanded()}>
        <For each={children()}>
          {(child) => (
            <FileTreeNode
              node={child}
              depth={props.depth + 1}
              onOpenFile={props.onOpenFile}
              expandedDirs={props.expandedDirs}
              toggleDir={props.toggleDir}
              onContextMenu={props.onContextMenu}
            />
          )}
        </For>
      </Show>
    </div>
  );
}

export default FileTreeNode;
