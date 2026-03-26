/**
 * project.service.ts
 *
 * Centralises the "open a project folder" flow so it is not duplicated
 * between App.tsx (Cmd+O keybinding) and FileTree.tsx (sidebar button).
 */

import { openFolderDialog, openProject, readFile, formatError, lspDidOpen, getGradleRoot, parseLspUri, lspDecompile, lspReadArchiveEntry } from "@/lib/tauri-api";
import { setProject, setLoading } from "@/stores/project.store";
import {
  editorState,
  isFileOpen,
  addOpenFile,
  setActiveFile,
  type OpenFile,
} from "@/stores/editor.store";
import { showToast } from "@/components/common/Toast";
import { detectLanguage } from "@/lib/file-utils";
import { pushNavigation } from "@/lib/navigation-history";
import type { FileNode } from "@/stores/project.store";
import { ensureLspReady } from "@/services/lsp.service";
import { navLog } from "@/lib/navigation-logger";

export interface OpenProjectResult {
  root: string;
  tree: FileNode;
  rootDirs: string[];
}

/**
 * Show the native folder picker, call the Rust `open_project` command,
 * update the project store, and return metadata the caller may need
 * (e.g. which dirs to auto-expand).
 *
 * Returns `null` when the user cancels the dialog or an error occurs.
 */
export async function openProjectFolder(): Promise<OpenProjectResult | null> {
  const path = await openFolderDialog();
  if (!path) return null;

  setLoading(true);
  try {
    const tree = await openProject(path);

    // Query the detected Gradle root (set during open_project on the Rust side).
    const gradleRoot = await getGradleRoot().catch(() => null);
    setProject(path, tree, gradleRoot);

    const rootDirs = (tree.children ?? [])
      .filter((c) => c.kind === "directory")
      .map((c) => c.path);

    // Kick off the LSP lifecycle in the background — check, download if needed,
    // then start.  Fire-and-forget: errors surface as toasts + status bar.
    ensureLspReady(path).catch(() => {});

    return { root: path, tree, rootDirs };
  } catch (err) {
    showToast(`Failed to open project: ${formatError(err)}`, "error");
    return null;
  } finally {
    setLoading(false);
  }
}

/**
 * Open a file at a specific location in the editor. Used by search results,
 * go-to-definition, symbol clicks, etc.
 */
export async function openFileAtLocation(
  path: string,
  line: number,
  col: number
): Promise<void> {
  try {
    // Push current position to navigation history before jumping
    const currentPath = editorState.activeFilePath;
    if (currentPath) {
      pushNavigation({
        path: currentPath,
        line: editorState.cursorLine ?? 1,
        col: editorState.cursorCol ?? 0,
      });
    }

    if (!isFileOpen(path)) {
      const content = await readFile(path);
      const name = path.split("/").pop() ?? path;
      const language = detectLanguage(path);
      const file: OpenFile = {
        path,
        name,
        savedContent: content,
        dirty: false,
        editorState: null,
        language,
      };
      addOpenFile(file);

      // Notify LSP about the newly opened file so it can provide navigation.
      // If LSP is still starting (the common case on first launch), this will
      // fail — registerOpenFilesWithLsp() will retry when the server is ready.
      if (language === "kotlin" || language === "gradle") {
        const name = file.name;
        navLog("debug", `Opening ${name} — sending lsp_did_open`);
        lspDidOpen(path, content, "kotlin").catch((err) => {
          navLog(
            "debug",
            `lsp_did_open deferred for ${name}: ${formatError(err)} (will retry on LSP ready)`
          );
        });
      }
    }
    setActiveFile(path);

    // Scroll to line after the editor has switched. EditorView is swapped
    // inside a SolidJS effect triggered by setActiveFile, so we wait a tick.
    setTimeout(async () => {
      const { getEditorView } = await import("@/components/editor/CodeEditor");
      const view = getEditorView();
      if (view) {
        const lineInfo = view.state.doc.line(Math.max(1, line));
        const pos = lineInfo.from + Math.max(0, col);
        view.dispatch({
          selection: { anchor: pos },
          scrollIntoView: true,
        });
        view.focus();
      }
    }, 50);
  } catch (err) {
    showToast(`Failed to open file: ${formatError(err)}`, "error");
  }
}

/**
 * Open a decompiled or archive-extracted file in a read-only editor tab.
 *
 * Called by navigation (go-to-definition / implementation) when the LSP
 * returns a `jar:`, `jrt:`, or archive-path (`!/`) URI pointing to library
 * source that is not on disk as a regular file.
 *
 * Strategy:
 *   1. Try the LSP `decompile` command first (works for `.class` files and
 *      gives nicely formatted Kotlin/Java source from the Fernflower decompiler
 *      built into IntelliJ).
 *   2. Fall back to reading the raw text from the ZIP/JAR archive on disk (for
 *      source JARs like `kotlin-stdlib-sources.jar` or the JDK `src.zip`).
 */
export async function openVirtualFile(
  uri: string,
  line: number,
  col: number
): Promise<void> {
  // Use the full URI as the "path" key so each unique decompiled file gets
  // its own tab and the tab can be identified unambiguously.
  const virtualPath = uri;

  // Push current navigation before jumping.
  const currentPath = editorState.activeFilePath;
  if (currentPath) {
    pushNavigation({
      path: currentPath,
      line: editorState.cursorLine ?? 1,
      col: editorState.cursorCol ?? 0,
    });
  }

  if (!isFileOpen(virtualPath)) {
    navLog("debug", `Opening virtual file: ${uri}`);

    // Determine a display name from the URI (last path component).
    const displayName = uri.split(/[/!]/).filter(Boolean).pop() ?? uri;
    const language = detectLanguage(displayName);

    let content = "";

    // Step 1: Try LSP decompile command.
    const decompiled = await lspDecompile(uri);
    if (decompiled?.code) {
      navLog("debug", `Decompiled ${displayName} via LSP (${decompiled.language})`);
      content = decompiled.code;
    } else {
      // Step 2: Try reading from archive directly (for source JARs).
      const parsed = parseLspUri(uri);
      if (parsed.kind === "jar" && parsed.archivePath && parsed.entryPath) {
        navLog("debug", `Extracting ${displayName} from archive ${parsed.archivePath}`);
        const extracted = await lspReadArchiveEntry(parsed.archivePath, parsed.entryPath);
        if (extracted !== null) {
          content = extracted;
        }
      }

      if (!content) {
        navLog("warn", `Could not retrieve content for ${uri} — showing empty placeholder`);
        content = `// Cannot decompile or extract: ${uri}\n// Try installing source JARs via Gradle.`;
      }
    }

    const file: OpenFile = {
      path: virtualPath,
      name: `[decompiled] ${displayName}`,
      savedContent: content,
      dirty: false,
      editorState: null,
      language,
      virtual: true,
    };
    addOpenFile(file);
  }

  setActiveFile(virtualPath);

  setTimeout(async () => {
    const { getEditorView } = await import("@/components/editor/CodeEditor");
    const view = getEditorView();
    if (view) {
      const safeLines = view.state.doc.lines;
      const targetLine = Math.max(1, Math.min(line, safeLines));
      const lineInfo = view.state.doc.line(targetLine);
      const pos = lineInfo.from + Math.max(0, col);
      view.dispatch({ selection: { anchor: pos }, scrollIntoView: true });
      view.focus();
    }
  }, 50);
}
