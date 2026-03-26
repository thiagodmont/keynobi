/**
 * lsp.service.ts
 *
 * Manages the full Kotlin LSP lifecycle: install check → download → start,
 * plus file registration when the server becomes ready.
 *
 * Design principles:
 *  - Non-blocking: always fire-and-forget from the call site; errors are
 *    surfaced as toasts and status-bar updates, never thrown.
 *  - Idempotent: safe to call multiple times; guards prevent double-starts.
 *  - Transparent: every stage updates `lspState` so the status bar and
 *    Output panel reflect what is happening in real time.
 */

import { invoke } from "@tauri-apps/api/core";
import { lspState, setLspStatus, setDownloadProgress } from "@/stores/lsp.store";
import { editorState } from "@/stores/editor.store";
import { showToast } from "@/components/common/Toast";
import { formatError, lspDidOpen } from "@/lib/tauri-api";
import { navLog } from "@/lib/navigation-logger";

/** Prevents a second concurrent start attempt. */
let starting = false;

/**
 * Ensure the Kotlin LSP server is running for `projectRoot`.
 *
 * Flow:
 *  1. Guard — return immediately if already running or a start is in progress.
 *  2. `lsp_check_installed` — probe whether the sidecar binary exists.
 *  3. If missing → `lsp_download` (Rust streams progress via `lsp:download_progress`
 *     events, which `App.tsx` forwards to `lspState`).
 *  4. `lsp_start` — Rust validates the path, spawns the server process, and
 *     emits `lsp:status` events as the server moves through starting → ready.
 *
 * The function never throws; all errors are reported via toast + status bar.
 */
export async function ensureLspReady(projectRoot: string): Promise<void> {
  const state = lspState.status.state;

  // Already up or in the middle of starting — nothing to do.
  if (
    state === "ready" ||
    state === "starting" ||
    state === "indexing" ||
    state === "downloading"
  ) {
    return;
  }

  // If the previous run left an error, attempt a clean stop first so the
  // Rust side can tear down any zombie client before we re-create it.
  if (state === "error") {
    try {
      await invoke("lsp_stop");
    } catch {
      // Ignore — we still want to try starting.
    }
  }

  if (starting) return;
  starting = true;

  try {
    // ── Step 1: Check installation ──────────────────────────────────────────
    const installation = await invoke<unknown | null>("lsp_check_installed");

    if (!installation) {
      // ── Step 2: Download ────────────────────────────────────────────────
      // Set status now so the status bar shows "Downloading" immediately.
      // Granular byte-level progress will flow through lsp:download_progress
      // events → App.tsx → lspState.downloadProgress automatically.
      setLspStatus("downloading", "Downloading Kotlin Language Server…");

      await invoke("lsp_download");

      // Clean up the progress indicator; lsp_start will set "starting" next.
      setDownloadProgress(null);
    }

    // ── Step 3: Start the server ────────────────────────────────────────────
    // lsp_start emits lsp:status {starting} then {ready} via Tauri events.
    // App.tsx forwards those to lspState, keeping the status bar in sync.
    await invoke("lsp_start", { projectRoot });
  } catch (err) {
    const msg = formatError(err);
    setLspStatus("error", msg);
    showToast(`Kotlin LSP failed to start: ${msg}`, "error");
  } finally {
    starting = false;
  }
}

/**
 * Re-send `textDocument/didOpen` for every Kotlin/Gradle file that is already
 * open in the editor.
 *
 * This is the fix for the race condition where files are opened in the editor
 * before the LSP server finishes starting.  In that window, `lspDidOpen` fails
 * silently because the server isn't listening yet.  `isFileOpen()` returns
 * true for those files, so `openFileAtLocation` won't call `lspDidOpen` again.
 * When the server eventually emits `lsp:status {ready}`, this function runs and
 * gives the server context for all already-open files so that Cmd+click,
 * completions, and diagnostics work immediately.
 *
 * Also sends `textDocument/didChange` for the active file when it has unsaved
 * edits, so the server has the user's current in-progress content.
 */
export async function registerOpenFilesWithLsp(): Promise<void> {
  const files = Object.values(editorState.openFiles).filter(
    (f) => f.language === "kotlin" || f.language === "gradle"
  );

  if (files.length === 0) return;

  navLog("info", `LSP ready — registering ${files.length} open file(s) with server`);

  // Import lazily to avoid a module initialisation cycle.
  const { getEditorView } = await import("@/components/editor/CodeEditor");
  const activeView = getEditorView();

  let registered = 0;
  for (const file of files) {
    try {
      // For the active file, prefer the live editor content so the server gets
      // any unsaved edits the user has already typed.
      const content =
        file.path === editorState.activeFilePath && activeView
          ? activeView.state.doc.toString()
          : file.savedContent;

      await lspDidOpen(file.path, content, "kotlin");
      registered++;
      navLog("debug", `LSP registered: ${file.name}`);
    } catch (err) {
      navLog("warn", `LSP registration failed for ${file.name}: ${formatError(err)}`);
    }
  }

  navLog("info", `LSP file registration complete (${registered}/${files.length} succeeded)`);
}
