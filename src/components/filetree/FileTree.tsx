import { createSignal, createMemo, Show, onMount, onCleanup, type JSX } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { projectState, type FileNode } from "@/stores/project.store";
import {
  editorState,
  addOpenFile,
  setActiveFile,
  isFileOpen,
  saveEditorState,
  updateSavedContent,
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

// ── Filename validation ────────────────────────────────────────────────────────

function validateFilename(name: string): string | null {
  if (!name.trim()) return "Name cannot be empty.";
  if (name.includes("/") || name.includes("\\"))
    return "Name must not contain path separators (/ or \\).";
  if (name.includes("\0")) return "Name must not contain null bytes.";
  if (name === "." || name === "..") return 'Name cannot be "." or "..".';
  if (/[\x00-\x1f]/.test(name)) return "Name must not contain control characters.";
  return null;
}

// ── Inline editing types ──────────────────────────────────────────────────────

type InlineNewMode = { kind: "file" | "folder"; parentPath: string } | null;

export function FileTree(): JSX.Element {
  const [expandedDirs, setExpandedDirs] = createSignal<Set<string>>(new Set());
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(null);
  const [focusedPath, setFocusedPath] = createSignal<string | null>(null);
  const [renamingPath, setRenamingPath] = createSignal<string | null>(null);
  const [inlineNew, setInlineNew] = createSignal<InlineNewMode>(null);

  const [dirCache, setDirCache] = createStore<Record<string, FileNode[]>>({});

  let treeContainerRef!: HTMLDivElement;
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

  // ── Children cache ────────────────────────────────────────────────────────

  function updateDirCache(dirPath: string, children: FileNode[]) {
    setDirCache(dirPath, reconcile(children, { key: "path" }));
  }

  // ── File watcher ───────────────────────────────────────────────────────────

  async function handleFileEvent(event: FileEvent) {
    if (!projectState.projectRoot) return;

    const affectedDirs = new Set<string>();
    affectedDirs.add(dirname(event.path));
    if (event.newPath) {
      affectedDirs.add(dirname(event.newPath));
    }

    for (const affectedDir of affectedDirs) {
      if (expandedDirs().has(affectedDir) || affectedDir === projectState.projectRoot) {
        try {
          const newChildren = await getDirectoryChildren(affectedDir);
          updateDirCache(affectedDir, newChildren);
        } catch {
          // Directory may have been deleted.
        }
      }
    }

    if (event.kind !== "modified") return;

    const filePath = event.path;
    if (!isFileOpen(filePath)) return;

    const file = editorState.openFiles[filePath];
    if (!file) return;

    if (!file.dirty) {
      try {
        const newContent = await readFile(filePath);
        if (newContent === file.savedContent) return;

        const newEditorState = createEditorState(newContent, file.language, filePath);
        updateSavedContent(filePath, newContent);
        saveEditorState(filePath, newEditorState);

        if (editorState.activeFilePath === filePath) {
          getEditorView()?.setState(newEditorState);
        }
      } catch {
        // File may have been deleted.
      }
    } else {
      showToast(
        `"${basename(filePath)}" was modified externally. Reload to get latest, or save to keep your changes.`,
        "warning"
      );
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

  async function expandDir(path: string) {
    if (expandedDirs().has(path)) return;
    const cached = dirCache[path];
    if (!cached || cached.length === 0) {
      try {
        const fetched = await getDirectoryChildren(path);
        updateDirCache(path, fetched);
      } catch {
        // ignore
      }
    }
    setExpandedDirs((prev) => new Set(prev).add(path));
  }

  function collapseDir(path: string) {
    if (!expandedDirs().has(path)) return;
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      next.delete(path);
      return next;
    });
  }

  // ── Open file ─────────────────────────────────────────────────────────────

  async function openFile(path: string) {
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

      const view = getEditorView();
      const current = editorState.activeFilePath;
      if (view && current) {
        saveEditorState(current, view.state);
      }

      addOpenFile({
        path,
        name,
        savedContent: content,
        dirty: false,
        editorState: edState,
        language,
      });
      setActiveFile(path);
    } catch (err) {
      showToast(`Failed to open file: ${formatError(err)}`, "error");
    }
  }

  // ── Open folder ───────────────────────────────────────────────────────────

  async function handleOpenFolder() {
    const result = await openProjectFolder();
    if (result) {
      // Use expandDir (which loads children) instead of raw setExpandedDirs.
      // Setting expandedDirs without loading children causes the folders to
      // appear open (chevron rotated, open-folder icon) but show no contents,
      // requiring two clicks to actually open them.
      for (const dirPath of result.rootDirs) {
        await expandDir(dirPath);
      }
    }
  }

  // ── Visible nodes list (for keyboard navigation) ──────────────────────────

  /**
   * Memoized flat ordered list of visible node paths (DFS, following expanded dirs).
   * Recomputes only when the tree structure changes (expand/collapse/refresh),
   * not on every keypress. For typical projects (< 500 visible nodes) this
   * takes < 1ms to compute.
   */
  const visiblePaths = createMemo<string[]>(() => {
    const tree = projectState.fileTree;
    if (!tree?.children) return [];

    const result: string[] = [];
    const expanded = expandedDirs();

    function walk(nodes: FileNode[]) {
      for (const node of nodes) {
        result.push(node.path);
        if (node.kind === "directory" && expanded.has(node.path)) {
          const cached = dirCache[node.path];
          const children = cached ?? node.children ?? [];
          walk(children);
        }
      }
    }

    walk(tree.children);
    return result;
  });

  // ── Keyboard navigation ────────────────────────────────────────────────────

  function handleKeyDown(e: KeyboardEvent) {
    // Don't intercept when an inline input is active.
    if (renamingPath() || inlineNew()) return;

    const focused = focusedPath();
    const visible = visiblePaths();
    if (visible.length === 0) return;

    const idx = focused ? visible.indexOf(focused) : -1;

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        const next = idx < visible.length - 1 ? idx + 1 : idx;
        setFocusedPath(visible[next]);
        scrollIntoView(visible[next]);
        break;
      }

      case "ArrowUp": {
        e.preventDefault();
        const prev = idx > 0 ? idx - 1 : 0;
        setFocusedPath(visible[prev]);
        scrollIntoView(visible[prev]);
        break;
      }

      case "ArrowRight": {
        e.preventDefault();
        if (!focused) break;
        const node = findNode(focused);
        if (node?.kind === "directory") {
          if (!expandedDirs().has(focused)) {
            expandDir(focused);
          } else {
            // Already expanded: move to first child.
            const children = dirCache[focused] ?? node.children ?? [];
            if (children.length > 0) {
              setFocusedPath(children[0].path);
              scrollIntoView(children[0].path);
            }
          }
        }
        break;
      }

      case "ArrowLeft": {
        e.preventDefault();
        if (!focused) break;
        const node = findNode(focused);
        if (node?.kind === "directory" && expandedDirs().has(focused)) {
          collapseDir(focused);
        } else {
          // Move to parent directory.
          const parent = dirname(focused);
          if (parent && parent !== focused && visible.includes(parent)) {
            setFocusedPath(parent);
            scrollIntoView(parent);
          }
        }
        break;
      }

      case "Enter": {
        e.preventDefault();
        if (!focused) break;
        const node = findNode(focused);
        if (node?.kind === "file") {
          openFile(focused);
        } else if (node?.kind === "directory") {
          toggleDir(focused);
        }
        break;
      }

      case " ": {
        e.preventDefault();
        if (!focused) break;
        const node = findNode(focused);
        if (node?.kind === "directory") {
          toggleDir(focused);
        }
        break;
      }

      case "Delete":
      case "Backspace": {
        e.preventDefault();
        if (!focused) break;
        const node = findNode(focused);
        if (node) handleDelete(node);
        break;
      }

      case "F2": {
        e.preventDefault();
        if (focused) setRenamingPath(focused);
        break;
      }

      default:
        return;
    }
  }

  function scrollIntoView(path: string) {
    const el = treeContainerRef?.querySelector(`[data-tree-path="${CSS.escape(path)}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }

  /** Walk the tree to find a node by path. */
  function findNode(path: string): FileNode | null {
    const tree = projectState.fileTree;
    if (!tree?.children) return null;

    function search(nodes: FileNode[]): FileNode | null {
      for (const node of nodes) {
        if (node.path === path) return node;
        if (node.kind === "directory") {
          const cached = dirCache[node.path];
          const children = cached ?? node.children ?? [];
          const found = search(children);
          if (found) return found;
        }
      }
      return null;
    }

    return search(tree.children);
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

  // ── Inline new file/folder ────────────────────────────────────────────────

  async function startInlineNew(parentPath: string, kind: "file" | "folder") {
    setContextMenu(null);
    // Ensure the parent directory is expanded so the input appears.
    await expandDir(parentPath);
    setInlineNew({ kind, parentPath });
  }

  async function handleInlineNewConfirm(name: string) {
    const mode = inlineNew();
    if (!mode) return;
    const err = validateFilename(name);
    if (err) {
      showToast(err, "error");
      return;
    }
    const fullPath = joinPath(mode.parentPath, name);
    try {
      if (mode.kind === "file") {
        await createFile(fullPath);
      } else {
        await createDirectory(fullPath);
      }
      showToast(`Created ${name}`, "success");
      setFocusedPath(fullPath);
    } catch (err) {
      showToast(
        `Failed to create ${mode.kind}: ${formatError(err)}`,
        "error"
      );
    }
    setInlineNew(null);
  }

  function handleInlineNewCancel() {
    setInlineNew(null);
  }

  // ── Inline rename ─────────────────────────────────────────────────────────

  async function handleRenameConfirm(oldPath: string, newName: string) {
    const err = validateFilename(newName);
    if (err) {
      showToast(err, "error");
      setRenamingPath(null);
      return;
    }
    const oldName = basename(oldPath);
    if (newName === oldName) {
      setRenamingPath(null);
      return;
    }
    const newPath = joinPath(dirname(oldPath), newName);
    try {
      await renamePath(oldPath, newPath);
      showToast(`Renamed to "${newName}"`, "success");
      setFocusedPath(newPath);
    } catch (err) {
      showToast(`Failed to rename: ${formatError(err)}`, "error");
    }
    setRenamingPath(null);
  }

  function handleRenameCancel() {
    setRenamingPath(null);
  }

  // ── Delete ────────────────────────────────────────────────────────────────

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
    <div
      ref={treeContainerRef}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      style={{
        flex: "1",
        overflow: "auto",
        "font-size": "13px",
        position: "relative",
        outline: "none",
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
        <span>{projectState.projectName?.toUpperCase() ?? "EXPLORER"}</span>
        <button
          onClick={handleOpenFolder}
          title="Open Folder"
          style={{ color: "var(--text-muted)", cursor: "pointer", padding: "2px" }}
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
              padding: "24px",
              color: "var(--text-muted)",
              "font-size": "12px",
              "text-align": "center",
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
                    dirCache={dirCache}
                    onDirLoaded={updateDirCache}
                    focusedPath={focusedPath}
                    onFocusNode={setFocusedPath}
                    renamingPath={renamingPath}
                    onRenameConfirm={handleRenameConfirm}
                    onRenameCancel={handleRenameCancel}
                    inlineNewParent={() => inlineNew()?.parentPath ?? null}
                    onInlineNewConfirm={handleInlineNewConfirm}
                    onInlineNewCancel={handleInlineNewCancel}
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
              <ContextMenuItem
                label="New File"
                onClick={() => startInlineNew(parentPath, "file")}
              />
              <ContextMenuItem
                label="New Folder"
                onClick={() => startInlineNew(parentPath, "folder")}
              />
              <Divider />
              <ContextMenuItem
                label="Rename"
                shortcut="F2"
                onClick={() => {
                  setContextMenu(null);
                  setRenamingPath(node.path);
                }}
              />
              <ContextMenuItem
                label="Delete"
                onClick={() => handleDelete(node)}
                danger
              />
              <Divider />
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

// ── Small helpers ─────────────────────────────────────────────────────────────

function Divider(): JSX.Element {
  return <div style={{ height: "1px", background: "var(--border)", margin: "2px 0" }} />;
}

function ContextMenuItem(props: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  shortcut?: string;
}): JSX.Element {
  return (
    <button
      onClick={props.onClick}
      style={{
        display: "flex",
        width: "100%",
        padding: "6px 12px",
        "justify-content": "space-between",
        "align-items": "center",
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
      <span>{props.label}</span>
      <Show when={props.shortcut}>
        <span style={{ color: "var(--text-muted)", "font-size": "11px" }}>
          {props.shortcut}
        </span>
      </Show>
    </button>
  );
}

export default FileTree;
