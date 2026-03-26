import { type JSX, Show, Switch, Match, onMount, onCleanup } from "solid-js";
import {
  uiState,
  setUIState,
  toggleSidebar,
  toggleBottomPanel,
  setSidebarWidth,
  setBottomPanelHeight,
  setActiveSidebarTab,
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
import { SearchPanel } from "@/components/search/SearchPanel";
import { SymbolsPanel } from "@/components/symbols/SymbolsPanel";
import { EditorTabs } from "@/components/editor/EditorTabs";
import { CodeEditor } from "@/components/editor/CodeEditor";
import { Breadcrumbs } from "@/components/editor/Breadcrumbs";
import { registerKeybinding, initKeybindings } from "@/lib/keybindings";
import { registerAction, type ActionCategory } from "@/lib/action-registry";
import { CommandPalette, openPalette } from "@/components/common/CommandPalette";
import { SettingsPanel, openSettings } from "@/components/settings/SettingsPanel";
import { loadSettings } from "@/stores/settings.store";
import { navigateBack, navigateForward } from "@/lib/navigation-history";
import { writeFile, formatError } from "@/lib/tauri-api";
import { openProjectFolder } from "@/services/project.service";
import { basename } from "@/lib/file-utils";

function formatShortcut(opts: { key: string; metaKey?: boolean; shiftKey?: boolean; altKey?: boolean }): string {
  const parts: string[] = [];
  if (opts.metaKey) parts.push("Cmd");
  if (opts.altKey) parts.push("Opt");
  if (opts.shiftKey) parts.push("Shift");
  parts.push(opts.key.length === 1 ? opts.key.toUpperCase() : opts.key);
  return parts.join("+");
}

function registerKeyAndAction(opts: {
  id: string;
  key: string;
  metaKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  label: string;
  category: ActionCategory;
  action: () => void;
}) {
  registerKeybinding({
    key: opts.key,
    metaKey: opts.metaKey,
    shiftKey: opts.shiftKey,
    altKey: opts.altKey,
    description: opts.label,
    context: "global",
    action: opts.action,
  });
  registerAction({
    id: opts.id,
    label: opts.label,
    category: opts.category,
    shortcut: formatShortcut(opts),
    action: opts.action,
  });
}

export function App(): JSX.Element {
  let unlistenClose: (() => void) | undefined;

  onMount(async () => {
    initKeybindings();
    loadSettings();

    // ── Settings ─────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "general.settings", key: ",", metaKey: true, label: "Open Settings", category: "General", action: () => openSettings() });

    // ── Layout ──────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "view.toggleSidebar", key: "b", metaKey: true, label: "Toggle Sidebar", category: "View", action: toggleSidebar });
    registerKeyAndAction({ id: "view.toggleBottomPanel", key: "j", metaKey: true, label: "Toggle Bottom Panel", category: "View", action: toggleBottomPanel });

    // ── File ────────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "file.openFolder", key: "o", metaKey: true, label: "Open Folder", category: "File", action: () => { openProjectFolder(); } });
    registerKeyAndAction({
      id: "file.closeTab", key: "w", metaKey: true, label: "Close Active Tab", category: "File",
      action: () => { const path = editorState.activeFilePath; if (path) promptSaveAndClose(path); },
    });
    registerKeyAndAction({
      id: "file.save", key: "s", metaKey: true, label: "Save Active File", category: "File",
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
    registerKeyAndAction({
      id: "file.saveAll", key: "s", metaKey: true, altKey: true, label: "Save All Files", category: "File",
      action: async () => {
        const dirtyPaths = editorState.tabOrder.filter((p) => editorState.openFiles[p]?.dirty);
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
            showToast(`Failed to save "${basename(path)}": ${formatError(err)}`, "error");
          }
        }
        if (savedCount > 0) {
          showToast(`Saved ${savedCount} file${savedCount > 1 ? "s" : ""}`, "success");
        }
      },
    });

    // ── Tab navigation ──────────────────────────────────────────────────────
    registerKeyAndAction({ id: "view.previousTab", key: "[", metaKey: true, shiftKey: true, label: "Previous Tab", category: "View", action: () => switchTab(-1) });
    registerKeyAndAction({ id: "view.nextTab", key: "]", metaKey: true, shiftKey: true, label: "Next Tab", category: "View", action: () => switchTab(+1) });

    // ── Search ──────────────────────────────────────────────────────────────
    registerKeyAndAction({
      id: "search.projectSearch", key: "f", metaKey: true, shiftKey: true, label: "Search in Project", category: "Search",
      action: () => { setActiveSidebarTab("search"); if (!uiState.sidebarVisible) toggleSidebar(); },
    });

    // ── Command Palette & Navigation ────────────────────────────────────────
    registerKeyAndAction({ id: "navigate.quickOpen", key: "p", metaKey: true, label: "Quick Open File", category: "Navigate", action: () => openPalette("files") });
    registerKeyAndAction({ id: "navigate.commandPalette", key: "p", metaKey: true, shiftKey: true, label: "Command Palette", category: "General", action: () => openPalette("commands") });
    registerKeyAndAction({ id: "navigate.goToSymbolInFile", key: "o", metaKey: true, shiftKey: true, label: "Go to Symbol in File", category: "Navigate", action: () => openPalette("documentSymbols") });
    registerKeyAndAction({ id: "navigate.goToSymbol", key: "t", metaKey: true, label: "Go to Symbol in Workspace", category: "Navigate", action: () => openPalette("symbols") });
    registerKeyAndAction({
      id: "navigate.back", key: "-", metaKey: true, label: "Navigate Back", category: "Navigate",
      action: async () => {
        const entry = navigateBack();
        if (entry) { const { openFileAtLocation } = await import("@/services/project.service"); openFileAtLocation(entry.path, entry.line, entry.col); }
      },
    });
    registerKeyAndAction({
      id: "navigate.forward", key: "-", metaKey: true, shiftKey: true, label: "Navigate Forward", category: "Navigate",
      action: async () => {
        const entry = navigateForward();
        if (entry) { const { openFileAtLocation } = await import("@/services/project.service"); openFileAtLocation(entry.path, entry.line, entry.col); }
      },
    });

    // ── Code Navigation (F12 family) ──────────────────────────────────────────
    // Note: The primary handlers (Cmd+Click, F12, Shift+F12, Cmd+F12) live in
    // lsp-extension.ts as CodeMirror keymaps. These App-level registrations
    // ensure they appear in the command palette (Cmd+Shift+P).
    registerAction({
      id: "navigate.goToDefinition",
      label: "Go to Definition",
      category: "Navigate",
      shortcut: "F12 or Cmd+Click",
      action: async () => {
        const path = editorState.activeFilePath;
        const view = getEditorView();
        if (!path || !view) return;
        const pos = view.state.selection.main.head;
        const line = view.state.doc.lineAt(pos);
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke("lsp_definition", { path, line: line.number - 1, col: pos - line.from }).catch(() => null);
        if (result) {
          const { uriToPath } = await import("@/lib/tauri-api");
          const { openFileAtLocation } = await import("@/services/project.service");
          const loc = Array.isArray(result) ? result[0] : result;
          if (loc?.uri) {
            openFileAtLocation(uriToPath(loc.uri), (loc.range?.start?.line ?? 0) + 1, loc.range?.start?.character ?? 0);
          }
        }
      },
    });
    registerAction({
      id: "navigate.findAllReferences",
      label: "Find All References",
      category: "Navigate",
      shortcut: "Shift+F12",
      action: async () => {
        const path = editorState.activeFilePath;
        const view = getEditorView();
        if (!path || !view) return;
        const pos = view.state.selection.main.head;
        const line = view.state.doc.lineAt(pos);
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke<any[]>("lsp_references", { path, line: line.number - 1, col: pos - line.from }).catch(() => null);
        if (result?.length) {
          const { showReferences } = await import("@/stores/references.store");
          const word = view.state.wordAt(pos);
          const query = word ? view.state.sliceDoc(word.from, word.to) : "references";
          showReferences(query, result);
        }
      },
    });
    registerAction({
      id: "navigate.goToImplementation",
      label: "Go to Implementation",
      category: "Navigate",
      shortcut: "Cmd+F12",
      action: async () => {
        const path = editorState.activeFilePath;
        const view = getEditorView();
        if (!path || !view) return;
        const pos = view.state.selection.main.head;
        const line = view.state.doc.lineAt(pos);
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke("lsp_implementation", { path, line: line.number - 1, col: pos - line.from }).catch(() => null);
        if (result) {
          const { uriToPath } = await import("@/lib/tauri-api");
          const { openFileAtLocation } = await import("@/services/project.service");
          const loc = Array.isArray(result) ? result[0] : result;
          if (loc?.uri) {
            openFileAtLocation(uriToPath(loc.uri), (loc.range?.start?.line ?? 0) + 1, loc.range?.start?.character ?? 0);
          }
        }
      },
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
            <Switch>
              <Match when={uiState.activeSidebarTab === "files"}>
                <FileTree />
              </Match>
              <Match when={uiState.activeSidebarTab === "search"}>
                <SearchPanel />
              </Match>
              <Match when={uiState.activeSidebarTab === "git"}>
                <SidebarPlaceholder label="Source Control" detail="Coming in Phase 6" />
              </Match>
              <Match when={uiState.activeSidebarTab === "symbols"}>
                <SymbolsPanel />
              </Match>
            </Switch>
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
              <Breadcrumbs />

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
      <CommandPalette />
      <SettingsPanel />
      <DialogHost />
    </div>
  );
}

function SidebarPlaceholder(props: {
  label: string;
  detail: string;
}): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        "justify-content": "center",
        height: "100%",
        color: "var(--text-muted)",
        "font-size": "12px",
        gap: "4px",
        padding: "16px",
        "text-align": "center",
      }}
    >
      <span>{props.label}</span>
      <span style={{ "font-size": "11px", opacity: "0.6" }}>{props.detail}</span>
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
