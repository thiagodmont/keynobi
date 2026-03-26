import { createStore, produce } from "solid-js/store";
import type { EditorState } from "@codemirror/state";
import { detectLanguage } from "@/lib/file-utils";

// Re-export so existing imports from this module keep working.
export { detectLanguage };

export type Language = "kotlin" | "gradle" | "xml" | "json" | "text";

export interface OpenFile {
  path: string;
  name: string;
  /** Content that matches what is currently on disk. Used for dirty detection. */
  savedContent: string;
  dirty: boolean;
  editorState: EditorState | null;
  language: Language;
  /**
   * When true, this file was opened from a JAR/archive or via decompilation
   * and has no on-disk representation.  It is read-only and should not be
   * saved or modified.
   */
  virtual?: boolean;
}

interface EditorStoreState {
  openFiles: Record<string, OpenFile>;
  /** Ordered list of open file paths — drives the tab bar. */
  tabOrder: string[];
  activeFilePath: string | null;
  /** Last 20 opened paths. Used by Cmd+P file picker (Phase 3). */
  recentFiles: string[];
  cursorLine: number | null;
  cursorCol: number | null;
  activeLanguage: Language | null;
}

const [editorState, setEditorState] = createStore<EditorStoreState>({
  openFiles: {},
  tabOrder: [],
  activeFilePath: null,
  recentFiles: [],
  cursorLine: null,
  cursorCol: null,
  activeLanguage: null,
});

export { editorState, setEditorState };

export function isFileOpen(path: string): boolean {
  return path in editorState.openFiles;
}

export function addOpenFile(file: OpenFile) {
  setEditorState(
    produce((s) => {
      s.openFiles[file.path] = file;
      if (!s.tabOrder.includes(file.path)) {
        s.tabOrder.push(file.path);
      }
      s.recentFiles = [
        file.path,
        ...s.recentFiles.filter((p) => p !== file.path),
      ].slice(0, 20);
    })
  );
}

export function removeOpenFile(path: string) {
  setEditorState(
    produce((s) => {
      delete s.openFiles[path];

      const idx = s.tabOrder.indexOf(path);
      // Guard: only splice if the path was actually in the list.
      if (idx !== -1) {
        s.tabOrder.splice(idx, 1);
      }

      if (s.activeFilePath === path) {
        // Prefer the tab to the right of the closed one, then left, then null.
        s.activeFilePath = s.tabOrder[idx] ?? s.tabOrder[idx - 1] ?? null;
      }
    })
  );
}

export function setActiveFile(path: string | null) {
  setEditorState("activeFilePath", path);
  if (path) {
    const file = editorState.openFiles[path];
    if (file) setEditorState("activeLanguage", file.language);
  } else {
    setEditorState("activeLanguage", null);
    setEditorState("cursorLine", null);
    setEditorState("cursorCol", null);
  }
}

export function markDirty(path: string) {
  if (editorState.openFiles[path]) {
    setEditorState("openFiles", path, "dirty", true);
  }
}

export function markClean(path: string) {
  if (editorState.openFiles[path]) {
    setEditorState("openFiles", path, "dirty", false);
  }
}

/**
 * Call this after a successful disk write. Updates savedContent and clears
 * the dirty flag atomically.
 */
export function updateSavedContent(path: string, content: string) {
  if (editorState.openFiles[path]) {
    setEditorState("openFiles", path, "savedContent", content);
    markClean(path);
  }
}

export function saveEditorState(path: string, state: EditorState) {
  if (editorState.openFiles[path]) {
    setEditorState("openFiles", path, "editorState", state);
  }
}

export function updateCursor(line: number, col: number) {
  setEditorState("cursorLine", line);
  setEditorState("cursorCol", col);
}

/**
 * Returns the current document text for a file.
 * For the active tab this reads from the live EditorView; for background
 * tabs it reads from the stored EditorState (which is kept in sync on tab
 * switch). Falls back to savedContent when neither is available.
 */
export function getCurrentContent(
  path: string,
  activeView: import("@codemirror/view").EditorView | null
): string {
  const file = editorState.openFiles[path];
  if (!file) return "";

  if (editorState.activeFilePath === path && activeView) {
    return activeView.state.doc.toString();
  }

  if (file.editorState) {
    return file.editorState.doc.toString();
  }

  return file.savedContent;
}
