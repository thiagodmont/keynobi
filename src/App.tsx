import { type JSX, Show, Switch, Match, onMount, onCleanup } from "solid-js";
import {
  uiState,
  setUIState,
  toggleSidebar,
  toggleBottomPanel,
  setSidebarWidth,
  setBottomPanelHeight,
  setActiveSidebarTab,
  openOutputPanel,
  setActiveBottomTab,
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
import { HealthPanel, openHealthPanel } from "@/components/health/HealthPanel";
import { loadSettings } from "@/stores/settings.store";
import { setLspStatus, setDownloadProgress, setServerCapabilities, setIndexingProgress, setIndexingJustCompleted } from "@/stores/lsp.store";
import { navigateBack, navigateForward } from "@/lib/navigation-history";
import { writeFile, formatError, listenLspStatus, listenLspDownloadProgress, listenLspCapabilities, listenLspProgress } from "@/lib/tauri-api";
import { openProjectFolder } from "@/services/project.service";
import { registerOpenFilesWithLsp } from "@/services/lsp.service";
import { basename } from "@/lib/file-utils";
// Phase 3: Build + Device initialization
import { initBuildService, runBuild, runAndDeploy, cancelBuild } from "@/services/build.service";
import { initDevices } from "@/stores/device.store";
import { openVariantPicker } from "@/components/build/VariantSelector";
import { CodeActionsPopup } from "@/components/editor/CodeActionsPopup";

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
  let unlistenLspStatus: (() => void) | undefined;
  let unlistenLspDownload: (() => void) | undefined;
  let unlistenLspCaps: (() => void) | undefined;
  let unlistenLspProgress: (() => void) | undefined;
  // Track previous LSP state to detect indexing→ready/error transitions.
  let prevLspStatusState: string = "stopped";

  onMount(async () => {
    initKeybindings();
    loadSettings();

    // Phase 3: Initialize build service and device polling.
    initBuildService().catch(console.error);
    initDevices().catch(console.error);

    // ── LSP event bridge ──────────────────────────────────────────────────────
    // Wire Tauri events to the lsp store so the status bar and settings panel
    // always reflect the real server state.
    unlistenLspStatus = await listenLspStatus((status) => {
      const wasIndexing = prevLspStatusState === "indexing";
      setLspStatus(status.state, status.message ?? undefined);

      // Detect the indexing → ready/error transition to trigger the
      // completion flash in the status bar.
      if (wasIndexing && status.state === "ready") {
        setIndexingProgress(null);
        setIndexingJustCompleted("success");
        setTimeout(() => setIndexingJustCompleted(null), 3000);
      } else if (wasIndexing && status.state === "error") {
        setIndexingProgress(null);
        setIndexingJustCompleted("error");
      } else if (status.state === "indexing") {
        // Clear any stale completion flash from a previous cycle.
        setIndexingJustCompleted(null);
      }

      prevLspStatusState = status.state;

      // When the server becomes ready, re-register all files that were already
      // open in the editor.  Without this, any file opened while the server was
      // still starting will be invisible to the LSP, causing Cmd+click, hover,
      // completions, and diagnostics to silently return nothing.
      if (status.state === "ready") {
        registerOpenFilesWithLsp().catch(() => {});
      }
      // Auto-open the Output panel when the LSP starts or hits an error so
      // the developer can see what's happening without manually navigating.
      if (status.state === "starting" || status.state === "error") {
        openOutputPanel();
      }
    });

    // Extract percentage from raw progress events and update the progress bar.
    unlistenLspProgress = await listenLspProgress((params: any) => {
      const pct = params?.value?.percentage;
      if (typeof pct === "number") {
        setIndexingProgress(Math.min(100, Math.max(0, pct)));
      }
    });

    unlistenLspDownload = await listenLspDownloadProgress((progress) => {
      setDownloadProgress({
        downloadedBytes: Number(progress.downloadedBytes),
        totalBytes: progress.totalBytes !== null ? Number(progress.totalBytes) : null,
        percent: progress.percent,
      });
      setLspStatus("downloading");
    });

    // Store server capabilities so the editor can skip unsupported requests.
    unlistenLspCaps = await listenLspCapabilities((caps) => {
      setServerCapabilities(caps as any);
    });

    // ── Settings ─────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "general.settings", key: ",", metaKey: true, label: "Open Settings", category: "General", action: () => openSettings() });

    // ── Layout ──────────────────────────────────────────────────────────────
    registerKeyAndAction({ id: "view.toggleSidebar", key: "b", metaKey: true, label: "Toggle Sidebar", category: "View", action: toggleSidebar });
    registerKeyAndAction({ id: "view.toggleBottomPanel", key: "j", metaKey: true, label: "Toggle Bottom Panel", category: "View", action: toggleBottomPanel });
    registerKeyAndAction({ id: "view.openOutput", key: "u", metaKey: true, shiftKey: true, label: "Open Output Panel", category: "View", action: openOutputPanel });
    registerKeyAndAction({ id: "view.healthCenter", key: "h", metaKey: true, shiftKey: true, label: "Open Health Center", category: "View", action: openHealthPanel });

    // ── LSP / Developer Tools ───────────────────────────────────────────────
    registerAction({
      id: "lsp.exportWorkspace",
      label: "Fix Package Errors — Generate Workspace JSON",
      category: "Developer" as ActionCategory,
      action: async () => {
        const { lspExportWorkspace } = await import("@/lib/tauri-api");
        try {
          showToast("Scanning project modules and generating workspace.json…", "info");
          const path = await lspExportWorkspace();
          showToast(
            `workspace.json generated at ${path}. Restart the LSP for it to take effect.`,
            "info"
          );
          openOutputPanel();
        } catch (err) {
          showToast(
            `Could not generate workspace.json: ${formatError(err)}. Make sure a project is open.`,
            "error"
          );
        }
      },
    });
    registerAction({
      id: "lsp.restartLsp",
      label: "Restart Kotlin LSP",
      category: "Developer" as ActionCategory,
      action: async () => {
        const { invoke: inv } = await import("@tauri-apps/api/core");
        const projectRoot = (await import("@/stores/project.store")).projectState.projectRoot;
        if (!projectRoot) { showToast("No project open", "info"); return; }
        try {
          await inv("lsp_stop");
          showToast("LSP stopped — restarting…", "info");
        } catch {
          // ignore stop errors (already stopped)
        }
        try {
          await inv("lsp_start", { projectRoot });
        } catch (err) {
          showToast(`Failed to restart LSP: ${formatError(err)}`, "error");
        }
      },
    });

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
          const { parseLspUri } = await import("@/lib/tauri-api");
          const { openFileAtLocation, openVirtualFile } = await import("@/services/project.service");
          const loc = Array.isArray(result) ? result[0] : result as any;
          if (loc?.uri) {
            const parsed = parseLspUri(loc.uri);
            const targetLine = (loc.range?.start?.line ?? 0) + 1;
            const targetCol = loc.range?.start?.character ?? 0;
            if (parsed.kind === "file") {
              openFileAtLocation(parsed.path, targetLine, targetCol);
            } else if (parsed.kind === "jar" || parsed.kind === "jrt") {
              openVirtualFile(loc.uri, targetLine, targetCol);
            } else {
              openFileAtLocation(loc.uri, targetLine, targetCol);
            }
          } else {
            const { lspState } = await import("@/stores/lsp.store");
            showToast(lspState.status.state === "indexing" ? "LSP is still loading the project — try again in a moment" : "No definition found at this position", "info");
          }
        } else {
          const { lspState } = await import("@/stores/lsp.store");
          showToast(lspState.status.state === "indexing" ? "LSP is still loading the project — try again in a moment" : "No definition found at this position", "info");
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
          const { parseLspUri } = await import("@/lib/tauri-api");
          const { openFileAtLocation, openVirtualFile } = await import("@/services/project.service");
          const loc = Array.isArray(result) ? result[0] : result as any;
          if (loc?.uri) {
            const parsed = parseLspUri(loc.uri);
            const targetLine = (loc.range?.start?.line ?? 0) + 1;
            const targetCol = loc.range?.start?.character ?? 0;
            if (parsed.kind === "file") {
              openFileAtLocation(parsed.path, targetLine, targetCol);
            } else if (parsed.kind === "jar" || parsed.kind === "jrt") {
              openVirtualFile(loc.uri, targetLine, targetCol);
            } else {
              openFileAtLocation(loc.uri, targetLine, targetCol);
            }
          }
        }
      },
    });

    registerKeyAndAction({
      id: "editor.organizeImports",
      key: "o",
      metaKey: true,
      shiftKey: true,
      label: "Organize Imports",
      category: "Edit" as ActionCategory,
      action: async () => {
        const path = editorState.activeFilePath;
        const view = getEditorView();
        if (!path || !view) return;
        const { lspCodeActionFiltered } = await import("@/lib/tauri-api");
        const { applyWorkspaceEdit } = await import("@/lib/codemirror/lsp-extension");
        try {
          const actions = await lspCodeActionFiltered(
            path, 0, 0, view.state.doc.lines - 1, 0,
            ["source.organizeImports"]
          );
          if (!actions?.length) {
            showToast("No imports to organize", "info");
            return;
          }
          // Execute the first organize-imports action.
          const action = actions[0] as any;
          if (action?.edit) {
            await applyWorkspaceEdit(action.edit);
          } else if (action?.command) {
            const { invoke } = await import("@tauri-apps/api/core");
            await invoke("lsp_decompile", { uri: action.command.command }).catch(() => {});
          }
        } catch (err) {
          showToast(`Organize imports failed: ${err}`, "error");
        }
      },
    });

    // ── Build & Run ────────────────────────────────────────────────────────────
    registerKeyAndAction({
      id: "build.run", key: "r", metaKey: true, label: "Run App", category: "Build" as ActionCategory,
      action: async () => {
        try {
          await runAndDeploy();
        } catch (e) {
          showToast(typeof e === "string" ? e : (e as Error).message ?? "Run failed", "error");
        }
      },
    });
    registerKeyAndAction({
      id: "build.runOnly", key: "r", metaKey: true, shiftKey: true, label: "Build Only (no deploy)", category: "Build" as ActionCategory,
      action: async () => {
        try {
          await runBuild();
        } catch (e) {
          showToast(typeof e === "string" ? e : (e as Error).message ?? "Build failed", "error");
        }
      },
    });
    registerAction({
      id: "build.cancel",
      label: "Cancel Build",
      category: "Build" as ActionCategory,
      action: async () => {
        await cancelBuild().catch(console.error);
      },
    });
    registerAction({
      id: "build.clean",
      label: "Clean Project",
      category: "Build" as ActionCategory,
      action: async () => {
        try {
          await runBuild("clean");
        } catch (e) {
          showToast(typeof e === "string" ? e : (e as Error).message ?? "Clean failed", "error");
        }
      },
    });
    registerKeyAndAction({
      id: "build.selectVariant", key: "v", metaKey: true, shiftKey: true, label: "Select Build Variant", category: "Build" as ActionCategory,
      action: () => openVariantPicker(),
    });
    registerAction({
      id: "build.selectDevice",
      label: "Select Device",
      category: "Build" as ActionCategory,
      action: () => {
        // Opens the status bar device panel (implemented in StatusBar).
        document.getElementById("device-selector-btn")?.click();
      },
    });
    registerAction({
      id: "view.buildPanel",
      label: "Open Build Panel",
      category: "View",
      action: () => {
        setUIState("bottomPanelVisible", true);
        setActiveBottomTab("build");
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
    unlistenLspStatus?.();
    unlistenLspDownload?.();
    unlistenLspCaps?.();
    unlistenLspProgress?.();
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
      <HealthPanel />
      <DialogHost />
      <CodeActionsPopup />
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
