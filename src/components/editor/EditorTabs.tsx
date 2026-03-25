import { For, Show, type JSX } from "solid-js";
import {
  editorState,
  setActiveFile,
  removeOpenFile,
  getCurrentContent,
} from "@/stores/editor.store";
import { getEditorView } from "./CodeEditor";
import { saveEditorState as storeSaveEditorState } from "@/stores/editor.store";
import { writeFile, formatError } from "@/lib/tauri-api";
import { showToast } from "@/components/common/Toast";
import { getFileTypeInfo } from "@/lib/file-utils";
import type { Language } from "@/stores/editor.store";

/**
 * Ask the user whether to save a dirty file, then close the tab.
 * Returns true if the tab was closed (save or discard), false if cancelled.
 *
 * IMPORTANT: we read the *current document content* (from the live EditorView
 * for the active tab, or from the stored EditorState for background tabs)
 * rather than savedContent, which reflects the last on-disk snapshot.
 */
export async function promptSaveAndClose(path: string): Promise<boolean> {
  const file = editorState.openFiles[path];
  if (!file) return false;

  if (!file.dirty) {
    removeOpenFile(path);
    return true;
  }

  const choice = window.confirm(
    `Save changes to "${file.name}" before closing?`
  );

  if (choice) {
    // Write the real current content, not the stale savedContent snapshot.
    const content = getCurrentContent(path, getEditorView());
    try {
      await writeFile(path, content);
      removeOpenFile(path);
      return true;
    } catch (err) {
      showToast(`Failed to save "${file.name}": ${formatError(err)}`, "error");
      return false;
    }
  } else {
    // Discard changes — close without saving.
    removeOpenFile(path);
    return true;
  }
}

// ── File type badge ─────────────────────────────────────────────────────────

function FileIcon(props: { language: Language }): JSX.Element {
  const info = () => getFileTypeInfo(props.language);
  return (
    <span
      style={{
        display: "inline-flex",
        "align-items": "center",
        "justify-content": "center",
        width: "14px",
        height: "14px",
        "font-size": "9px",
        "font-weight": "700",
        color: info().color,
        "flex-shrink": "0",
      }}
    >
      {info().label}
    </span>
  );
}

// ── Tab bar ─────────────────────────────────────────────────────────────────

export function EditorTabs(): JSX.Element {
  function switchToTab(path: string) {
    const current = editorState.activeFilePath;
    if (current === path) return;

    // Persist current editor state before switching away.
    const view = getEditorView();
    if (view && current) {
      storeSaveEditorState(current, view.state);
    }

    setActiveFile(path);
  }

  async function closeTab(e: MouseEvent, path: string) {
    e.stopPropagation();
    await promptSaveAndClose(path);
  }

  function onMouseDown(e: MouseEvent, path: string) {
    // Middle-click closes the tab.
    if (e.button === 1) {
      e.preventDefault();
      closeTab(e, path);
    }
  }

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "row",
        height: "var(--tab-height)",
        background: "var(--bg-tertiary)",
        "border-bottom": "1px solid var(--border)",
        "overflow-x": "auto",
        "overflow-y": "hidden",
        "flex-shrink": "0",
        "scrollbar-width": "none",
      }}
    >
      <For each={editorState.tabOrder}>
        {(path) => {
          const file = () => editorState.openFiles[path];
          const isActive = () => editorState.activeFilePath === path;

          return (
            <div
              onClick={() => switchToTab(path)}
              onMouseDown={(e) => onMouseDown(e, path)}
              style={{
                display: "flex",
                "align-items": "center",
                gap: "6px",
                padding: "0 8px 0 12px",
                height: "100%",
                "min-width": "120px",
                "max-width": "200px",
                cursor: "pointer",
                background: isActive() ? "var(--bg-primary)" : "transparent",
                "border-top": isActive()
                  ? "1px solid var(--accent)"
                  : "1px solid transparent",
                "border-right": "1px solid var(--border)",
                "white-space": "nowrap",
                "flex-shrink": "0",
              }}
              title={path}
            >
              <FileIcon language={file()?.language ?? "text"} />

              <span
                style={{
                  flex: "1",
                  overflow: "hidden",
                  "text-overflow": "ellipsis",
                  "font-size": "12px",
                  color: isActive()
                    ? "var(--text-primary)"
                    : "var(--text-secondary)",
                }}
              >
                {file()?.name ?? ""}
              </span>

              {/* Unsaved-changes dot / close button */}
              <button
                onClick={(e) => closeTab(e, path)}
                style={{
                  width: "16px",
                  height: "16px",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  "border-radius": "3px",
                  "flex-shrink": "0",
                  color: "var(--text-muted)",
                  cursor: "pointer",
                }}
                title={file()?.dirty ? "Unsaved changes — click to close" : "Close tab"}
              >
                <Show
                  when={file()?.dirty}
                  fallback={
                    /* × close icon */
                    <svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor">
                      <path d="M9.414 8l3.293-3.293a1 1 0 0 0-1.414-1.414L8 6.586 4.707 3.293a1 1 0 0 0-1.414 1.414L6.586 8l-3.293 3.293a1 1 0 1 0 1.414 1.414L8 9.414l3.293 3.293a1 1 0 0 0 1.414-1.414L9.414 8z" />
                    </svg>
                  }
                >
                  {/* ● unsaved dot */}
                  <svg viewBox="0 0 16 16" width="8" height="8" fill="var(--warning)">
                    <circle cx="8" cy="8" r="5" />
                  </svg>
                </Show>
              </button>
            </div>
          );
        }}
      </For>
    </div>
  );
}

export default EditorTabs;
