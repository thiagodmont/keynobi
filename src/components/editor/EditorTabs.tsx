import { For, Show, type JSX } from "solid-js";
import {
  editorState,
  setActiveFile,
  removeOpenFile,
  getCurrentContent,
  saveEditorState as storeSaveEditorState,
  type Language,
} from "@/stores/editor.store";
import { getEditorView } from "@/components/editor/CodeEditor";
import { writeFile, formatError } from "@/lib/tauri-api";
import { showToast } from "@/components/common/Toast";
import { showSaveDialog } from "@/components/common/Dialog";
import { getFileTypeInfo } from "@/lib/file-utils";
import { invoke } from "@tauri-apps/api/core";

// ── Static style constants ────────────────────────────────────────────────────

const TAB_BAR_STYLE = {
  display: "flex",
  "flex-direction": "row",
  height: "var(--tab-height)",
  background: "var(--bg-tertiary)",
  "border-bottom": "1px solid var(--border)",
  "overflow-x": "auto",
  "overflow-y": "hidden",
  "flex-shrink": "0",
  "scrollbar-width": "none",
} as const;

const FILENAME_STYLE = {
  flex: "1",
  overflow: "hidden",
  "text-overflow": "ellipsis",
  "font-size": "12px",
} as const;

const CLOSE_BTN_STYLE = {
  width: "16px",
  height: "16px",
  display: "flex",
  "align-items": "center",
  "justify-content": "center",
  "border-radius": "3px",
  "flex-shrink": "0",
  color: "var(--text-muted)",
  cursor: "pointer",
} as const;

/**
 * Ask the user whether to save a dirty file, then close the tab.
 * Returns true if the tab was closed (save or discard), false if cancelled.
 *
 * Uses the proper three-option dialog (Save / Don't Save / Cancel) so the user
 * can always abort the close and keep editing.
 */
export async function promptSaveAndClose(path: string): Promise<boolean> {
  const file = editorState.openFiles[path];
  if (!file) return false;

  if (!file.dirty) {
    removeOpenFile(path);
    // Notify LSP that the file is closed
    const lang = file.language;
    if (lang === "kotlin" || lang === "gradle") {
      invoke("lsp_did_close", { path }).catch(() => {});
    }
    return true;
  }

  const result = await showSaveDialog(file.name);

  if (result === "save") {
    const content = getCurrentContent(path, getEditorView());
    try {
      await writeFile(path, content);
      removeOpenFile(path);
      if (file.language === "kotlin" || file.language === "gradle") {
        invoke("lsp_did_close", { path }).catch(() => {});
      }
      return true;
    } catch (err) {
      showToast(`Failed to save "${file.name}": ${formatError(err)}`, "error");
      return false;
    }
  } else if (result === "discard") {
    removeOpenFile(path);
    if (file.language === "kotlin" || file.language === "gradle") {
      invoke("lsp_did_close", { path }).catch(() => {});
    }
    return true;
  } else {
    // "cancel" — abort the close operation
    return false;
  }
}

// ── File type badge ───────────────────────────────────────────────────────────

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

// ── Tab bar ───────────────────────────────────────────────────────────────────

export function EditorTabs(): JSX.Element {
  function switchToTab(path: string) {
    const current = editorState.activeFilePath;
    if (current === path) return;

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
    if (e.button === 1) {
      e.preventDefault();
      closeTab(e, path);
    }
  }

  return (
    <div style={TAB_BAR_STYLE}>
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
                  ...FILENAME_STYLE,
                  color: isActive() ? "var(--text-primary)" : "var(--text-secondary)",
                }}
              >
                {file()?.name ?? ""}
              </span>

              {/* Unsaved-changes dot / close button */}
              <button
                onClick={(e) => closeTab(e, path)}
                style={CLOSE_BTN_STYLE}
                title={file()?.dirty ? "Unsaved changes — click to close" : "Close tab"}
              >
                <Show
                  when={file()?.dirty}
                  fallback={
                    <svg viewBox="0 0 16 16" width="10" height="10" fill="currentColor">
                      <path d="M9.414 8l3.293-3.293a1 1 0 0 0-1.414-1.414L8 6.586 4.707 3.293a1 1 0 0 0-1.414 1.414L6.586 8l-3.293 3.293a1 1 0 1 0 1.414 1.414L8 9.414l3.293 3.293a1 1 0 0 0 1.414-1.414L9.414 8z" />
                    </svg>
                  }
                >
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
