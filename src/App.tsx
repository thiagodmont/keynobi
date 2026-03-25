import { type JSX, Show, onMount } from "solid-js";
import { uiState, setUIState, toggleSidebar, toggleBottomPanel, setSidebarWidth, setBottomPanelHeight } from "@/stores/ui.store";
import { editorState } from "@/stores/editor.store";
import TitleBar from "@/components/layout/TitleBar";
import Sidebar from "@/components/layout/Sidebar";
import StatusBar from "@/components/layout/StatusBar";
import PanelContainer from "@/components/layout/PanelContainer";
import Resizable from "@/components/common/Resizable";
import { ToastContainer } from "@/components/common/Toast";
import { FileTree } from "@/components/filetree/FileTree";
import { EditorTabs } from "@/components/editor/EditorTabs";
import { CodeEditor } from "@/components/editor/CodeEditor";
import {
  registerKeybinding,
  initKeybindings,
} from "@/lib/keybindings";
import { openFolderDialog, openProject, formatError } from "@/lib/tauri-api";
import { setProject, setLoading } from "@/stores/project.store";
import { showToast } from "@/components/common/Toast";

export function App(): JSX.Element {
  onMount(() => {
    initKeybindings();

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

    registerKeybinding({
      key: "o",
      metaKey: true,
      action: async () => {
        const path = await openFolderDialog();
        if (!path) return;
        setLoading(true);
        try {
          const tree = await openProject(path);
          setProject(path, tree);
        } catch (err) {
          showToast(`Failed to open: ${formatError(err)}`, "error");
        } finally {
          setLoading(false);
        }
      },
      description: "Open Folder",
      context: "global",
    });
  });

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

      {/* Main content row */}
      <div style={{ display: "flex", flex: "1", overflow: "hidden", "min-height": "0" }}>
        {/* Sidebar with file tree */}
        <Sidebar>
          <FileTree />
        </Sidebar>

        {/* Horizontal resizer for sidebar */}
        <Show when={uiState.sidebarVisible}>
          <Resizable
            direction="horizontal"
            onResize={(delta) => setSidebarWidth(uiState.sidebarWidth + delta)}
            onReset={() => setSidebarWidth(240)}
          />
        </Show>

        {/* Editor + bottom panel column */}
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
            {/* Tab bar — only shown when files are open */}
            <Show when={editorState.tabOrder.length > 0}>
              <EditorTabs />
            </Show>

            {/* Editor or empty state */}
            <Show
              when={editorState.activeFilePath}
              fallback={
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
                    or press <kbd style={{ background: "var(--bg-tertiary)", padding: "1px 5px", "border-radius": "3px" }}>Cmd+O</kbd> to open a project
                  </span>
                </div>
              }
            >
              <CodeEditor />
            </Show>
          </div>

          {/* Vertical resizer for bottom panel */}
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

export default App;
