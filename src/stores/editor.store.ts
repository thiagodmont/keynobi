import { createStore, produce } from "solid-js/store";
import type { EditorState } from "@codemirror/state";

export type Language = "kotlin" | "gradle" | "xml" | "json" | "text";

export interface OpenFile {
  path: string;
  name: string;
  savedContent: string;
  dirty: boolean;
  editorState: EditorState | null;
  language: Language;
}

interface EditorStoreState {
  openFiles: Record<string, OpenFile>;
  tabOrder: string[];
  activeFilePath: string | null;
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

export function detectLanguage(path: string): Language {
  if (path.endsWith(".kt")) return "kotlin";
  if (path.endsWith(".gradle.kts") || path.endsWith(".gradle")) return "gradle";
  if (path.endsWith(".xml")) return "xml";
  if (path.endsWith(".json")) return "json";
  return "text";
}

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
      // Track in recent files
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
      s.tabOrder.splice(idx, 1);
      if (s.activeFilePath === path) {
        // Activate nearest tab
        const newActive =
          s.tabOrder[idx] ?? s.tabOrder[idx - 1] ?? null;
        s.activeFilePath = newActive;
      }
    })
  );
}

export function setActiveFile(path: string | null) {
  setEditorState("activeFilePath", path);
  if (path) {
    const file = editorState.openFiles[path];
    if (file) {
      setEditorState("activeLanguage", file.language);
    }
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
