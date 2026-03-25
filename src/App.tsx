import { type JSX, Show, onMount } from "solid-js";
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
import { FileTree } from "@/components/filetree/FileTree";
import { EditorTabs } from "@/components/editor/EditorTabs";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { registerKeybinding, initKeybindings } from "@/lib/keybindings";
import { writeFile, formatError } from "@/lib/tauri-api";
import { openProjectFolder } from "@/services/project.service";

export function App(): JSX.Element {
  onMount(() => {
    initKeybindings();

    // ── Layout shortcuts ───────────────────────────────────────────────────
    registerKeybinding({ key: "b", metaKey: true, action: toggleSidebar,     description: "Toggle Sidebar",      context: "global" });
    registerKeybinding({ key: "j", metaKey: true, action: toggleBottomPanel, description: "Toggle Bottom Panel", context: "global" });

    // ── File shortcuts ─────────────────────────────────────────────────────
    registerKeybinding({
      key: "o",
      metaKey: true,
      description: "Open Folder",
      context: "global",
      action: () => { openProjectFolder(); },
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
          // updateSavedContent is handled inside the CM6 keymap as well;
          // this path handles saves triggered from outside the editor focus.
          const { updateSavedContent } = await import("@/stores/editor.store");
          updateSavedContent(path, content);
        } catch (err) {
          showToast(`Failed to save: ${formatError(err)}`, "error");
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

        <div style={{ flex: "1", display: "flex", "flex-direction": "column", overflow: "hidden", "min-width": "0" }}>
          {/* Editor area */}
          <div style={{ flex: "1", display: "flex", "flex-direction": "column", overflow: "hidden", "min-height": "0" }}>
            <Show when={editorState.tabOrder.length > 0}>
              <EditorTabs />
            </Show>

            <Show
              when={editorState.activeFilePath}
              fallback={<EmptyEditorState />}
            >
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

      <StatusBar />
      <ToastContainer />
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
        <kbd style={{ background: "var(--bg-tertiary)", padding: "1px 5px", "border-radius": "3px" }}>
          Cmd+O
        </kbd>{" "}
        to open a project
      </span>
    </div>
  );
}

export default App;
