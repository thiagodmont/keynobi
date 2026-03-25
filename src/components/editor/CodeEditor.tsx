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
      // Pass baseExtensions as a nested array — CM6 flattens extension arrays
      // at creation time, so we avoid copying the ~15-item array on every open.
      baseExtensions,
      getLanguageExtension(language),
      // Cursor position tracking and dirty detection
      EditorView.updateListener.of((update) => {
        if (update.docChanged && editorView) {
          const activePath = editorState.activeFilePath;
          if (activePath === path) {
            const file = editorState.openFiles[activePath];
            if (file) {
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
        if (update.selectionSet && editorView) {
          const activePath = editorState.activeFilePath;
          if (activePath === path) {
            const pos = update.state.selection.main.head;
            const line = update.state.doc.lineAt(pos);
            updateCursor(line.number, pos - line.from + 1);
          }
        }
      }),
      // Cmd+S save (handled inside CM6 so it only fires when editor is focused)
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
      state: EditorState.create({
        doc: "",
        extensions: [baseExtensions],
      }),
    });

    // If there's already an active file, load it immediately.
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

  // React to active file changes — swap EditorState without recreating the DOM.
  createEffect(() => {
    const activePath = editorState.activeFilePath;
    if (!editorView) return;

    if (!activePath) {
      editorView.setState(
        EditorState.create({
          doc: "",
          extensions: [baseExtensions],
        })
      );
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

    // Ensure editor has focus after tab switch.
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
