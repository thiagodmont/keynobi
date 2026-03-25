import { createSignal, Show, onMount, onCleanup, type JSX } from "solid-js";
import { projectState, type FileNode } from "@/stores/project.store";
import {
  editorState,
  addOpenFile,
  setActiveFile,
  isFileOpen,
  saveEditorState,
} from "@/stores/editor.store";
import {
  readFile,
  createFile,
  createDirectory,
  deletePath,
  renamePath,
  getDirectoryChildren,
  onFileChanged,
  formatError,
  type FileEvent,
} from "@/lib/tauri-api";
import { detectLanguage, basename, dirname, joinPath } from "@/lib/file-utils";
import { createEditorState, getEditorView } from "@/components/editor/CodeEditor";
import { showToast } from "@/components/common/Toast";
import { openProjectFolder } from "@/services/project.service";
import FileTreeNode from "./FileTreeNode";

interface ContextMenuState {
  x: number;
  y: number;
  node: FileNode;
}

export function FileTree(): JSX.Element {
  const [expandedDirs, setExpandedDirs] = createSignal<Set<string>>(new Set());
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(null);

  let unlistenFileChanged: (() => void) | null = null;

  onMount(async () => {
    unlistenFileChanged = await onFileChanged(handleFileEvent);
    document.addEventListener("click", closeContextMenu);
    document.addEventListener("contextmenu", closeContextMenuOnOutside);
  });

  onCleanup(() => {
    unlistenFileChanged?.();
    document.removeEventListener("click", closeContextMenu);
    document.removeEventListener("contextmenu", closeContextMenuOnOutside);
  });

  // ── File watcher ───────────────────────────────────────────────────────────

  /**
   * When a file changes externally, refresh the children of the affected
   * directory. This is a targeted update — we do NOT reload the entire tree,
   * which would collapse all expanded directories.
   */
  async function handleFileEvent(event: FileEvent) {
    if (!projectState.projectRoot) return;

    // Determine which directory to refresh.
    // For a file event the parent dir; for a dir event the dir itself.
    const affectedDir =
      event.kind === "created" || event.kind === "deleted" || event.kind === "renamed"
        ? dirname(event.path)
        : dirname(event.path);

    // Only refresh dirs that are currently expanded (visible to the user).
    if (!expandedDirs().has(affectedDir) && affectedDir !== projectState.projectRoot) {
      return;
    }

    try {
      await getDirectoryChildren(affectedDir);
      // Trigger a re-render by toggling expanded state momentarily.
      // A cleaner solution will come in Phase 2 when we add a proper
      // directory-node cache; for now a lightweight signal poke is enough.
      setExpandedDirs((prev) => new Set(prev));
    } catch {
      // Silently ignore — the dir may have been deleted.
    }
  }

  // ── Directory expand/collapse ─────────────────────────────────────────────

  function toggleDir(path: string) {
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      next.has(path) ? next.delete(path) : next.add(path);
      return next;
    });
  }

  // ── Open file ─────────────────────────────────────────────────────────────

  async function openFile(path: string) {
    // Dedup: reuse existing tab if already open.
    if (isFileOpen(path)) {
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
      const name = basename(path);
      const edState = createEditorState(content, language, path);

      // Capture current editor state before switching tabs.
      const view = getEditorView();
      const current = editorState.activeFilePath;
      if (view && current) {
        saveEditorState(current, view.state);
      }

      addOpenFile({ path, name, savedContent: content, dirty: false, editorState: edState, language });
      setActiveFile(path);
    } catch (err) {
      showToast(`Failed to open file: ${formatError(err)}`, "error");
    }
  }

  // ── Open folder ───────────────────────────────────────────────────────────

  async function handleOpenFolder() {
    const result = await openProjectFolder();
    if (result) {
      setExpandedDirs(new Set(result.rootDirs));
    }
  }

  // ── Context menu ──────────────────────────────────────────────────────────

  function showContextMenu(e: MouseEvent, node: FileNode) {
    setContextMenu({ x: e.clientX, y: e.clientY, node });
  }

  function closeContextMenu() {
    setContextMenu(null);
  }

  function closeContextMenuOnOutside(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest("[data-context-menu]")) {
      setContextMenu(null);
    }
  }

  async function handleNewFile(parentPath: string) {
    const name = window.prompt("New file name:");
    if (!name?.trim()) return;
    try {
      await createFile(joinPath(parentPath, name.trim()));
      showToast(`Created ${name}`, "success");
    } catch (err) {
      showToast(`Failed to create file: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleNewFolder(parentPath: string) {
    const name = window.prompt("New folder name:");
    if (!name?.trim()) return;
    try {
      await createDirectory(joinPath(parentPath, name.trim()));
      showToast(`Created ${name}`, "success");
    } catch (err) {
      showToast(`Failed to create folder: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleDelete(node: FileNode) {
    if (!window.confirm(`Move "${node.name}" to Trash?`)) return;
    try {
      await deletePath(node.path);
      showToast(`Moved "${node.name}" to Trash`, "info");
    } catch (err) {
      showToast(`Failed to delete: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleRename(node: FileNode) {
    const newName = window.prompt("Rename to:", node.name);
    if (!newName?.trim() || newName === node.name) return;
    const newPath = joinPath(dirname(node.path), newName.trim());
    try {
      await renamePath(node.path, newPath);
      showToast(`Renamed to "${newName}"`, "success");
    } catch (err) {
      showToast(`Failed to rename: ${formatError(err)}`, "error");
    }
    setContextMenu(null);
  }

  async function handleCopyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      showToast("Path copied to clipboard", "info");
    } catch {
      showToast("Failed to copy path (clipboard access denied)", "error");
    }
    setContextMenu(null);
  }

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div style={{ flex: "1", overflow: "auto", "font-size": "13px", position: "relative" }}>
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
        <span>{projectState.projectName?.toUpperCase() ?? "EXPLORER"}</span>
        <button onClick={handleOpenFolder} title="Open Folder" style={{ color: "var(--text-muted)", cursor: "pointer", padding: "2px" }}>
          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
            <path d="M1 3.5A1.5 1.5 0 0 1 2.5 2h2.764c.958 0 1.76.56 2.311 1.184C7.576 3.184 8 3.5 8 3.5h4.5A1.5 1.5 0 0 1 14 5v1H2V5a.5.5 0 0 0-.5-.5h-1V3.5zm1 4.5V11a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V8H2z" />
          </svg>
        </button>
      </div>

      <Show
        when={!projectState.loading}
        fallback={<div style={{ padding: "24px", color: "var(--text-muted)", "font-size": "12px", "text-align": "center" }}>Loading...</div>}
      >
        <Show
          when={projectState.fileTree}
          fallback={
            <div style={{ padding: "24px 16px", color: "var(--text-muted)", "font-size": "12px", "text-align": "center" }}>
              <p style={{ "margin-bottom": "12px" }}>No folder open</p>
              <button
                onClick={handleOpenFolder}
                style={{ padding: "6px 12px", background: "var(--accent)", color: "white", "border-radius": "4px", cursor: "pointer", "font-size": "12px" }}
              >
                Open Folder
              </button>
              <p style={{ "margin-top": "8px", "font-size": "11px" }}>or press Cmd+O</p>
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

      {/* Context menu */}
      <Show when={contextMenu()}>
        {(menu) => {
          const node = menu().node;
          const parentPath = node.kind === "directory" ? node.path : dirname(node.path);
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
              <ContextMenuItem label="New File"   onClick={() => handleNewFile(parentPath)} />
              <ContextMenuItem label="New Folder" onClick={() => handleNewFolder(parentPath)} />
              <Divider />
              <ContextMenuItem label="Rename" onClick={() => handleRename(node)} />
              <ContextMenuItem label="Delete" onClick={() => handleDelete(node)} danger />
              <Divider />
              <ContextMenuItem label="Copy Path" onClick={() => handleCopyPath(node.path)} />
            </div>
          );
        }}
      </Show>
    </div>
  );
}

// ── Small helpers ─────────────────────────────────────────────────────────────

function Divider(): JSX.Element {
  return <div style={{ height: "1px", background: "var(--border)", margin: "2px 0" }} />;
}

function ContextMenuItem(props: { label: string; onClick: () => void; danger?: boolean }): JSX.Element {
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
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-active)"; }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "none"; }}
    >
      {props.label}
    </button>
  );
}

export default FileTree;
