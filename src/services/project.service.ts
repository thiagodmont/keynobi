/**
 * project.service.ts
 *
 * Centralises the "open a project folder" flow so it is not duplicated
 * between App.tsx (Cmd+O keybinding) and FileTree.tsx (sidebar button).
 */

import { openFolderDialog, openProject, readFile, formatError, lspDidOpen } from "@/lib/tauri-api";
import { setProject, setLoading } from "@/stores/project.store";
import {
  editorState,
  isFileOpen,
  addOpenFile,
  setActiveFile,
  type OpenFile,
} from "@/stores/editor.store";
import { showToast } from "@/components/common/Toast";
import { detectLanguage } from "@/lib/file-utils";
import { pushNavigation } from "@/lib/navigation-history";
import type { FileNode } from "@/stores/project.store";

export interface OpenProjectResult {
  root: string;
  tree: FileNode;
  rootDirs: string[];
}

/**
 * Show the native folder picker, call the Rust `open_project` command,
 * update the project store, and return metadata the caller may need
 * (e.g. which dirs to auto-expand).
 *
 * Returns `null` when the user cancels the dialog or an error occurs.
 */
export async function openProjectFolder(): Promise<OpenProjectResult | null> {
  const path = await openFolderDialog();
  if (!path) return null;

  setLoading(true);
  try {
    const tree = await openProject(path);
    setProject(path, tree);

    const rootDirs = (tree.children ?? [])
      .filter((c) => c.kind === "directory")
      .map((c) => c.path);

    return { root: path, tree, rootDirs };
  } catch (err) {
    showToast(`Failed to open project: ${formatError(err)}`, "error");
    return null;
  } finally {
    setLoading(false);
  }
}

/**
 * Open a file at a specific location in the editor. Used by search results,
 * go-to-definition, symbol clicks, etc.
 */
export async function openFileAtLocation(
  path: string,
  line: number,
  col: number
): Promise<void> {
  try {
    // Push current position to navigation history before jumping
    const currentPath = editorState.activeFilePath;
    if (currentPath) {
      pushNavigation({
        path: currentPath,
        line: editorState.cursorLine ?? 1,
        col: editorState.cursorCol ?? 0,
      });
    }

    if (!isFileOpen(path)) {
      const content = await readFile(path);
      const name = path.split("/").pop() ?? path;
      const language = detectLanguage(path);
      const file: OpenFile = {
        path,
        name,
        savedContent: content,
        dirty: false,
        editorState: null,
        language,
      };
      addOpenFile(file);

      // Notify LSP about the newly opened file so it can provide navigation
      if (language === "kotlin" || language === "gradle") {
        lspDidOpen(path, content, "kotlin").catch(() => {});
      }
    }
    setActiveFile(path);

    // Scroll to line after the editor has switched. EditorView is swapped
    // inside a SolidJS effect triggered by setActiveFile, so we wait a tick.
    setTimeout(async () => {
      const { getEditorView } = await import("@/components/editor/CodeEditor");
      const view = getEditorView();
      if (view) {
        const lineInfo = view.state.doc.line(Math.max(1, line));
        const pos = lineInfo.from + Math.max(0, col);
        view.dispatch({
          selection: { anchor: pos },
          scrollIntoView: true,
        });
        view.focus();
      }
    }, 50);
  } catch (err) {
    showToast(`Failed to open file: ${formatError(err)}`, "error");
  }
}
