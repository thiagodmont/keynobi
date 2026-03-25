import {
  createSignal,
  Show,
  onMount,
  onCleanup,
  type JSX,
} from "solid-js";
import {
  projectState,
  setProject,
  setLoading,
  type FileNode,
} from "@/stores/project.store";
import {
  editorState,
  addOpenFile,
  setActiveFile,
  detectLanguage,
  isFileOpen,
  saveEditorState,
} from "@/stores/editor.store";
import {
  openFolderDialog,
  openProject,
  readFile,
  createFile,
  createDirectory,
  deletePath,
  renamePath,
  onFileChanged,
  formatError,
  type FileEvent,
} from "@/lib/tauri-api";
import { createEditorState, getEditorView } from "@/components/editor/CodeEditor";
import { showToast } from "@/components/common/Toast";
import FileTreeNode from "./FileTreeNode";

interface ContextMenuState {
  x: number;
  y: number;
  node: FileNode;
}

export function FileTree(): JSX.Element {
  const [expandedDirs, setExpandedDirs] = createSignal<Set<string>>(
    new Set()
  );
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(
    null
  );
  let unlistenFileChanged: (() => void) | null = null;

  onMount(async () => {
    unlistenFileChanged = await onFileChanged(handleFileEvent);

    // Close context menu on outside click
    document.addEventListener("click", closeContextMenu);
    document.addEventListener("contextmenu", closeContextMenuOnOutside);
  });

  onCleanup(() => {
    unlistenFileChanged?.();
    document.removeEventListener("click", closeContextMenu);
    document.removeEventListener("contextmenu", closeContextMenuOnOutside);
  });

  function closeContextMenu() {
    setContextMenu(null);
  }

  function closeContextMenuOnOutside(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (!target.closest("[data-context-menu]")) {
      setContextMenu(null);
    }
  }

  function handleFileEvent(_event: FileEvent) {
    // Refresh the file tree when changes happen
    // Re-fetch the project tree to stay in sync
    if (projectState.projectRoot) {
      // Lightweight: just invalidate – the tree will re-render
      setProject(projectState.projectRoot, projectState.fileTree!);
    }
  }

  function toggleDir(path: string) {
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  async function openFile(path: string) {
    // Dedup: if already open, just activate
    if (isFileOpen(path)) {
      // Save current editor state
      const view = getEditorView();
      const current = editorState.activeFilePath;
      if (view && current && current !== path) {
        saveEditorState(current, view.state);
      }
      setActiveFile(path);
      return;
    }

    try {
      const content = await readFile(path);
      const language = detectLanguage(path);
      const name = path.split("/").pop() ?? path;
      const edState = createEditorState(content, language, path);

      addOpenFile({
        path,
        name,
        savedContent: content,
        dirty: false,
        editorState: edState,
        language,
      });

      // Save current state before switching
      const view = getEditorView();
      const current = editorState.activeFilePath;
      if (view && current) {
        saveEditorState(current, view.state);
      }

      setActiveFile(path);
    } catch (err) {
      showToast(`Failed to open file: ${formatError(err)}`, "error");
    }
  }

  async function handleOpenFolder() {
    const path = await openFolderDialog();
    if (!path) return;

    setLoading(true);
    try {
      const tree = await openProject(path);
      setProject(path, tree);

      // Expand root-level directories
      if (tree.children) {
        const rootDirs = tree.children
          .filter((c) => c.kind === "directory")
          .map((c) => c.path);
        setExpandedDirs(new Set(rootDirs));
      }
    } catch (err) {
      showToast(`Failed to open project: ${formatError(err)}`, "error");
    } finally {
      setLoading(false);
    }
  }

  function showContextMenu(e: MouseEvent, node: FileNode) {
    setContextMenu({ x: e.clientX, y: e.clientY, node });
  }

  async function handleNewFile(parentPath: string) {
    const name = window.prompt("New file name:");
    if (!name) return;
    const fullPath = `${parentPath}/${name}`;
    try {
      await createFile(fullPath);
      showToast(`Created ${name}`, "success");
    } catch (err) {
      showToast(`Failed to create file: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleNewFolder(parentPath: string) {
    const name = window.prompt("New folder name:");
    if (!name) return;
    const fullPath = `${parentPath}/${name}`;
    try {
      await createDirectory(fullPath);
      showToast(`Created ${name}`, "success");
    } catch (err) {
      showToast(`Failed to create folder: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleDelete(node: FileNode) {
    const confirmed = window.confirm(`Move "${node.name}" to Trash?`);
    if (!confirmed) return;
    try {
      await deletePath(node.path);
      showToast(`Moved ${node.name} to Trash`, "info");
    } catch (err) {
      showToast(`Failed to delete: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleRename(node: FileNode) {
    const newName = window.prompt("Rename to:", node.name);
    if (!newName || newName === node.name) return;
    const parent = node.path.substring(0, node.path.lastIndexOf("/"));
    const newPath = `${parent}/${newName}`;
    try {
      await renamePath(node.path, newPath);
      showToast(`Renamed to ${newName}`, "success");
    } catch (err) {
      showToast(`Failed to rename: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  function handleCopyPath(path: string) {
    navigator.clipboard.writeText(path);
    showToast("Path copied", "info");
    setContextMenu(null);
  }

  return (
    <div
      style={{
        flex: "1",
        overflow: "auto",
        "font-size": "13px",
        position: "relative",
      }}
    >
      {/* Panel header */}
      <div
        style={{
          padding: "8px 12px 4px",
          "font-size": "11px",
          "font-weight": "600",
          color: "var(--text-muted)",
          "text-transform": "uppercase",
          "letter-spacing": "0.08em",
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
        }}
      >
        <span>
          {projectState.projectName
            ? projectState.projectName.toUpperCase()
            : "EXPLORER"}
        </span>
        <button
          onClick={handleOpenFolder}
          title="Open Folder"
          style={{
            color: "var(--text-muted)",
            cursor: "pointer",
            padding: "2px",
          }}
        >
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
            <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h2.764c.958 0 1.76.56 2.311 1.184C7.576 3.184 8 3.5 8 3.5h4.5A1.5 1.5 0 0 1 14 5v1H2V5a.5.5 0 0 0-.5-.5h-1V3.5zm1 4.5V11a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V8H2z" />
          </svg>
        </button>
      </div>

      <Show
        when={!projectState.loading}
        fallback={
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              padding: "24px",
              color: "var(--text-muted)",
              "font-size": "12px",
              gap: "8px",
            }}
          >
            Loading...
          </div>
        }
      >
        <Show
          when={projectState.fileTree}
          fallback={
            <div
              style={{
                padding: "24px 16px",
                color: "var(--text-muted)",
                "font-size": "12px",
                "text-align": "center",
              }}
            >
              <p style={{ "margin-bottom": "12px" }}>No folder open</p>
              <button
                onClick={handleOpenFolder}
                style={{
                  padding: "6px 12px",
                  background: "var(--accent)",
                  color: "white",
                  "border-radius": "4px",
                  cursor: "pointer",
                  "font-size": "12px",
                }}
              >
                Open Folder
              </button>
              <p style={{ "margin-top": "8px", "font-size": "11px" }}>
                or use Cmd+O
              </p>
            </div>
          }
        >
          {(tree) => (
            <Show when={tree().children}>
              {(children) =>
                children().map((child) => (
                  <FileTreeNode
                    node={child}
                    depth={0}
                    onOpenFile={openFile}
                    expandedDirs={expandedDirs}
                    toggleDir={toggleDir}
                    onContextMenu={showContextMenu}
                  />
                ))
              }
            </Show>
          )}
        </Show>
      </Show>

      {/* Context Menu */}
      <Show when={contextMenu()}>
        {(menu) => {
          const node = menu().node;
          const isDir = node.kind === "directory";
          const parentPath = isDir
            ? node.path
            : node.path.substring(0, node.path.lastIndexOf("/"));

          return (
            <div
              data-context-menu
              onClick={(e) => e.stopPropagation()}
              style={{
                position: "fixed",
                top: `${menu().y}px`,
                left: `${menu().x}px`,
                background: "var(--bg-tertiary)",
                border: "1px solid var(--border)",
                "border-radius": "4px",
                "box-shadow": "0 4px 12px rgba(0,0,0,0.5)",
                "z-index": "1000",
                "min-width": "180px",
                overflow: "hidden",
                "font-size": "12px",
              }}
            >
              <ContextMenuItem
                label="New File"
                onClick={() => handleNewFile(parentPath)}
              />
              <ContextMenuItem
                label="New Folder"
                onClick={() => handleNewFolder(parentPath)}
              />
              <div
                style={{
                  height: "1px",
                  background: "var(--border)",
                  margin: "2px 0",
                }}
              />
              <ContextMenuItem
                label="Rename"
                onClick={() => handleRename(node)}
              />
              <ContextMenuItem
                label="Delete"
                onClick={() => handleDelete(node)}
                danger
              />
              <div
                style={{
                  height: "1px",
                  background: "var(--border)",
                  margin: "2px 0",
                }}
              />
              <ContextMenuItem
                label="Copy Path"
                onClick={() => handleCopyPath(node.path)}
              />
            </div>
          );
        }}
      </Show>
    </div>
  );
}

function ContextMenuItem(props: {
  label: string;
  onClick: () => void;
  danger?: boolean;
}): JSX.Element {
  return (
    <button
      onClick={props.onClick}
      style={{
        display: "block",
        width: "100%",
        padding: "6px 12px",
        "text-align": "left",
        cursor: "pointer",
        color: props.danger ? "var(--error)" : "var(--text-primary)",
        background: "none",
        "font-size": "12px",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = "var(--bg-active)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = "none";
      }}
    >
      {props.label}
    </button>
  );
}

export default FileTree;
