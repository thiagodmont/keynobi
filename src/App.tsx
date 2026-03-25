import { type JSX, Show, onMount, onCleanup } from "solid-js";
import {
  uiState,
  setUIState,
  toggleSidebar,
  toggleBottomPanel,
  setSidebarWidth,
  setBottomPanelHeight,
} from "@/stores/ui.store";
import {
  editorState,
  setActiveFile,
  saveEditorState,
  getCurrentContent,
} from "@/stores/editor.store";
import { getEditorView } from "@/components/editor/CodeEditor";
import { promptSaveAndClose } from "@/components/editor/EditorTabs";
import TitleBar from "@/components/layout/TitleBar";
import Sidebar from "@/components/layout/Sidebar";
import StatusBar from "@/components/layout/StatusBar";
import PanelContainer from "@/components/layout/PanelContainer";
import Resizable from "@/components/common/Resizable";
import { ToastContainer, showToast } from "@/components/common/Toast";
import { DialogHost, showCloseDialog } from "@/components/common/Dialog";
import { AppErrorBoundary } from "@/components/common/ErrorBoundary";
import { FileTree } from "@/components/filetree/FileTree";
import { EditorTabs } from "@/components/editor/EditorTabs";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { registerKeybinding, initKeybindings } from "@/lib/keybindings";
import { writeFile, formatError } from "@/lib/tauri-api";
import { openProjectFolder } from "@/services/project.service";
import { basename } from "@/lib/file-utils";

export function App(): JSX.Element {
  let unlistenClose: (() => void) | undefined;

  onMount(async () => {
    initKeybindings();

    // ── Layout shortcuts ───────────────────────────────────────────────────
    registerKeybinding({
      key: "b",
      metaKey: true,
      action: toggleSidebar,
      description: "Toggle Sidebar",
      context: "global",
    });
    registerKeybinding({
      key: "j",
      metaKey: true,
      action: toggleBottomPanel,
      description: "Toggle Bottom Panel",
      context: "global",
    });

    // ── File shortcuts ─────────────────────────────────────────────────────
    registerKeybinding({
      key: "o",
      metaKey: true,
      description: "Open Folder",
      context: "global",
      action: () => {
        openProjectFolder();
      },
    });

    registerKeybinding({
      key: "w",
      metaKey: true,
      description: "Close Active Tab",
      context: "global",
      action: () => {
        const path = editorState.activeFilePath;
        if (path) promptSaveAndClose(path);
      },
    });

    registerKeybinding({
      key: "s",
      metaKey: true,
      description: "Save Active File",
      context: "global",
      action: async () => {
        const path = editorState.activeFilePath;
        if (!path) return;
        const content = getCurrentContent(path, getEditorView());
        try {
          await writeFile(path, content);
          const { updateSavedContent } = await import("@/stores/editor.store");
          updateSavedContent(path, content);
        } catch (err) {
          showToast(`Failed to save: ${formatError(err)}`, "error");
        }
      },
    });

    // ── Save All (Cmd+Option+S) ────────────────────────────────────────────
    registerKeybinding({
      key: "s",
      metaKey: true,
      altKey: true,
      description: "Save All Files",
      context: "global",
      action: async () => {
        const dirtyPaths = editorState.tabOrder.filter(
          (p) => editorState.openFiles[p]?.dirty
        );
        if (dirtyPaths.length === 0) return;

        let savedCount = 0;
        for (const path of dirtyPaths) {
          const content = getCurrentContent(path, getEditorView());
          try {
            await writeFile(path, content);
            const { updateSavedContent } = await import("@/stores/editor.store");
            updateSavedContent(path, content);
            savedCount++;
          } catch (err) {
            showToast(
              `Failed to save "${basename(path)}": ${formatError(err)}`,
              "error"
            );
          }
        }
        if (savedCount > 0) {
          showToast(
            `Saved ${savedCount} file${savedCount > 1 ? "s" : ""}`,
            "success"
          );
        }
      },
    });

    // ── Tab navigation ─────────────────────────────────────────────────────
    registerKeybinding({
      key: "[",
      metaKey: true,
      shiftKey: true,
      description: "Previous Tab",
      context: "global",
      action: () => switchTab(-1),
    });

    registerKeybinding({
      key: "]",
      metaKey: true,
      shiftKey: true,
      description: "Next Tab",
      context: "global",
      action: () => switchTab(+1),
    });

    // ── Window close guard ─────────────────────────────────────────────────
    // Intercept the native window close so users are not surprised by data
    // loss. We prevent the close, ask via our three-option dialog, and then
    // either save-all + close or discard + close.
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const appWindow = getCurrentWindow();
      const unlisten = await appWindow.onCloseRequested(async (event) => {
        const dirtyFiles = Object.values(editorState.openFiles).filter((f) => f.dirty);
        if (dirtyFiles.length === 0) return; // no unsaved changes, let it close

        // Prevent the immediate close so we can show our dialog.
        event.preventDefault();

        const choice = await showCloseDialog(dirtyFiles.length);

        if (choice === "save-all") {
          for (const file of dirtyFiles) {
            const content = getCurrentContent(file.path, getEditorView());
            try {
              await writeFile(file.path, content);
              const { updateSavedContent } = await import("@/stores/editor.store");
              updateSavedContent(file.path, content);
            } catch (err) {
              showToast(
                `Failed to save "${file.name}": ${formatError(err)}`,
                "error"
              );
              // Abort the close on save failure so the user doesn't lose work.
              return;
            }
          }
          appWindow.close();
        } else if (choice === "discard-all") {
          appWindow.close();
        }
        // "cancel" → do nothing; window stays open
      });

      unlistenClose = unlisten;
    } catch {
      // Not running inside Tauri (e.g. vitest environment) — skip window hooks.
    }
  });

  onCleanup(() => {
    unlistenClose?.();
  });

  function switchTab(direction: -1 | 1) {
    const current = editorState.activeFilePath;
    const idx = current ? editorState.tabOrder.indexOf(current) : -1;
    const target = editorState.tabOrder[idx + direction];
    if (!target) return;

    const view = getEditorView();
    if (view && current) saveEditorState(current, view.state);
    setActiveFile(target);
  }

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100vh",
        width: "100vw",
        overflow: "hidden",
        background: "var(--bg-primary)",
      }}
    >
      <TitleBar />

      <AppErrorBoundary>
        <div style={{ display: "flex", flex: "1", overflow: "hidden", "min-height": "0" }}>
          <Sidebar>
            <FileTree />
          </Sidebar>

          <Show when={uiState.sidebarVisible}>
            <Resizable
              direction="horizontal"
              onResize={(delta) => setSidebarWidth(uiState.sidebarWidth + delta)}
              onReset={() => setSidebarWidth(240)}
            />
          </Show>

          <div
            style={{
              flex: "1",
              display: "flex",
              "flex-direction": "column",
              overflow: "hidden",
              "min-width": "0",
            }}
          >
            {/* Editor area */}
            <div
              style={{
                flex: "1",
                display: "flex",
                "flex-direction": "column",
                overflow: "hidden",
                "min-height": "0",
              }}
            >
              <Show when={editorState.tabOrder.length > 0}>
                <EditorTabs />
              </Show>

              <Show when={editorState.activeFilePath} fallback={<EmptyEditorState />}>
                <CodeEditor />
              </Show>
            </div>

            <Show when={uiState.bottomPanelVisible}>
              <Resizable
                direction="vertical"
                onResize={(delta) => setBottomPanelHeight(uiState.bottomPanelHeight - delta)}
                onReset={() => setUIState("bottomPanelHeight", 250)}
              />
              <PanelContainer height={uiState.bottomPanelHeight} />
            </Show>
          </div>
        </div>
      </AppErrorBoundary>

      <StatusBar />
      <ToastContainer />
      {/* DialogHost must be mounted at app root to render above all other content */}
      <DialogHost />
    </div>
  );
}

function EmptyEditorState(): JSX.Element {
  return (
    <div
      style={{
        flex: "1",
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        "flex-direction": "column",
        gap: "8px",
        color: "var(--text-muted)",
        "font-size": "13px",
      }}
    >
      <span style={{ "font-size": "32px", opacity: "0.3" }}>⌨</span>
      <span>Open a file from the sidebar</span>
      <span style={{ "font-size": "11px" }}>
        or press{" "}
        <kbd
          style={{
            background: "var(--bg-tertiary)",
            padding: "1px 5px",
            "border-radius": "3px",
          }}
        >
          Cmd+O
        </kbd>{" "}
        to open a project
      </span>
    </div>
  );
}

export default App;
