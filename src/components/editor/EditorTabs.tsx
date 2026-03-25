import { For, Show, type JSX } from "solid-js";
import {
  editorState,
  setActiveFile,
  removeOpenFile,
} from "@/stores/editor.store";
import { getEditorView } from "./CodeEditor";
import { saveEditorState as storeSaveEditorState } from "@/stores/editor.store";

// Exported so it can be called from the store
export async function promptSaveAndClose(path: string): Promise<boolean> {
  return new Promise((resolve) => {
    const file = editorState.openFiles[path];
    if (!file?.dirty) {
      removeOpenFile(path);
      resolve(true);
      return;
    }

    // Use browser confirm for now (modal in 1.7)
    const choice = window.confirm(
      `Save changes to "${file.name}" before closing?`
    );
    if (choice) {
      // Save then close
      import("@/lib/tauri-api")
        .then(({ writeFile }) =>
          writeFile(path, file.savedContent)
        )
        .then(() => {
          removeOpenFile(path);
          resolve(true);
        })
        .catch(() => resolve(false));
    } else {
      removeOpenFile(path);
      resolve(true);
    }
  });
}

function FileIcon(props: { language: string }): JSX.Element {
  const colors: Record<string, string> = {
    kotlin: "#a97bff",
    gradle: "#02b10a",
    xml: "#f0883e",
    json: "#e8c07d",
    text: "#858585",
  };
  const labels: Record<string, string> = {
    kotlin: "K",
    gradle: "G",
    xml: "X",
    json: "J",
    text: "T",
  };
  const color = colors[props.language] ?? "#858585";
  const label = labels[props.language] ?? "?";
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
        color,
        "flex-shrink": "0",
      }}
    >
      {label}
    </span>
  );
}

export function EditorTabs(): JSX.Element {
  function switchToTab(path: string) {
    const current = editorState.activeFilePath;
    if (current === path) return;

    // Save current editor state before switching
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

  function onMiddleClick(e: MouseEvent, path: string) {
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
              onMouseDown={(e) => onMiddleClick(e, path)}
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

              {/* Dirty indicator / close button */}
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
                title={file()?.dirty ? "Unsaved changes" : "Close"}
              >
                <Show
                  when={file()?.dirty}
                  fallback={
                    <svg
                      viewBox="0 0 16 16"
                      width="10"
                      height="10"
                      fill="currentColor"
                    >
                      <path d="M9.414 8l3.293-3.293a1 1 0 0 0-1.414-1.414L8 6.586 4.707 3.293a1 1 0 0 0-1.414 1.414L6.586 8l-3.293 3.293a1 1 0 1 0 1.414 1.414L8 9.414l3.293 3.293a1 1 0 0 0 1.414-1.414L9.414 8z" />
                    </svg>
                  }
                >
                  <svg
                    viewBox="0 0 16 16"
                    width="8"
                    height="8"
                    fill="var(--warning)"
                  >
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
