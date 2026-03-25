import { createEffect, onMount, onCleanup, type JSX } from "solid-js";
import { EditorView, keymap } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { baseExtensions } from "@/lib/codemirror/setup";
import { kotlin } from "@/lib/codemirror/kotlin";
import { gradle } from "@/lib/codemirror/gradle";
import { javascript } from "@codemirror/lang-javascript";
import { xml } from "@codemirror/lang-xml";
import {
  editorState,
  markDirty,
  markClean,
  saveEditorState,
  updateCursor,
  updateSavedContent,
  type Language,
} from "@/stores/editor.store";
import { writeFile, formatError } from "@/lib/tauri-api";
import { showToast } from "@/components/common/Toast";

let editorView: EditorView | null = null;

// Reuse a single empty state instead of creating a new EditorState on every
// effect run when no file is active. EditorState.create is not free — it
// initialises all extensions, keymaps, and facets.
const EMPTY_EDITOR_STATE = EditorState.create({
  doc: "",
  extensions: [baseExtensions],
});

function getLanguageExtension(language: Language) {
  switch (language) {
    case "kotlin":
      return kotlin();
    case "gradle":
      return gradle();
    case "xml":
      return xml();
    case "json":
      return javascript({ typescript: false });
    default:
      return [];
  }
}

export function createEditorState(
  content: string,
  language: Language,
  path: string
): EditorState {
  return EditorState.create({
    doc: content,
    extensions: [
      baseExtensions,
      getLanguageExtension(language),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && editorView) {
          const activePath = editorState.activeFilePath;
          if (activePath === path) {
            const file = editorState.openFiles[activePath];
            if (file) {
              // Fast path: compare document length first (O(1)).
              // Only fall through to the expensive toString() comparison when
              // lengths match, which is rare — most edits change the length.
              const docLen = update.state.doc.length;
              const savedLen = file.savedContent.length;
              if (docLen !== savedLen) {
                if (!file.dirty) markDirty(activePath);
              } else {
                const currentContent = update.state.doc.toString();
                const isDirty = currentContent !== file.savedContent;
                if (isDirty && !file.dirty) {
                  markDirty(activePath);
                } else if (!isDirty && file.dirty) {
                  markClean(activePath);
                }
              }
            }
          }
        }
        if (update.selectionSet && editorView) {
          const activePath = editorState.activeFilePath;
          if (activePath === path) {
            const pos = update.state.selection.main.head;
            const line = update.state.doc.lineAt(pos);
            updateCursor(line.number, pos - line.from + 1);
          }
        }
      }),
      keymap.of([
        {
          key: "Mod-s",
          run: () => {
            const activePath = editorState.activeFilePath;
            if (activePath && editorView) {
              const content = editorView.state.doc.toString();
              writeFile(activePath, content)
                .then(() => {
                  updateSavedContent(activePath, content);
                })
                .catch((err) => {
                  showToast(`Failed to save: ${formatError(err)}`, "error");
                });
            }
            return true;
          },
        },
      ]),
    ],
  });
}

export function getEditorView(): EditorView | null {
  return editorView;
}

interface CodeEditorProps {
  class?: string;
}

export function CodeEditor(props: CodeEditorProps): JSX.Element {
  let containerRef!: HTMLDivElement;

  onMount(() => {
    editorView = new EditorView({
      parent: containerRef,
      state: EMPTY_EDITOR_STATE,
    });

    const activePath = editorState.activeFilePath;
    if (activePath) {
      const file = editorState.openFiles[activePath];
      if (file?.editorState) {
        editorView.setState(file.editorState);
      }
    }
  });

  onCleanup(() => {
    if (editorView) {
      editorView.destroy();
      editorView = null;
    }
  });

  createEffect(() => {
    const activePath = editorState.activeFilePath;
    if (!editorView) return;

    if (!activePath) {
      editorView.setState(EMPTY_EDITOR_STATE);
      return;
    }

    const file = editorState.openFiles[activePath];
    if (!file) return;

    if (file.editorState) {
      editorView.setState(file.editorState);
    } else {
      const newState = createEditorState(file.savedContent, file.language, activePath);
      saveEditorState(activePath, newState);
      editorView.setState(newState);
    }

    setTimeout(() => editorView?.focus(), 0);
  });

  return (
    <div
      ref={containerRef}
      class={props.class}
      style={{
        flex: "1",
        overflow: "hidden",
        "min-height": "0",
      }}
    />
  );
}

export default CodeEditor;
