/**
 * navigation-logger.ts
 *
 * Lightweight client-side logger for the code-navigation flow.
 * Entries are pushed into `lspLogStore` and appear in the Output panel
 * under the source label "lsp:navigate" so developers can follow every
 * step from user gesture → LSP request → file opened.
 *
 * Entries are also persisted to the session log file via a fire-and-forget
 * Tauri command so they survive the in-memory store across restarts.
 *
 * IDs start at 900_000 to visually distinguish client entries from server
 * entries (which start at 0 and rarely exceed 2000 in a session).
 */

import { invoke } from "@tauri-apps/api/core";
import { lspLogStore } from "@/stores/log.store";
import type { LogLevel } from "@/bindings";

let _clientId = 900_000;

/**
 * Push a navigation log entry to the Output panel and persist it to the
 * session log file.
 *
 * @example
 * navLog("debug", `Cmd+click at ${file}:${line}:${col}`);
 * navLog("info",  `Navigated to ${targetFile}:${targetLine}`);
 * navLog("warn",  `Definition not found for symbol at ${line}:${col}`);
 */
export function navLog(level: LogLevel, message: string): void {
  lspLogStore.pushEntry({
    id: _clientId++,
    timestamp: new Date().toISOString(),
    level,
    source: "lsp:navigate",
    message,
  });
  // Fire-and-forget: persist to the session log file so navigation events
  // survive across panel reloads and can be included in bug reports.
  invoke("lsp_append_client_log", {
    message,
    level,
    source: "lsp:navigate",
  }).catch(() => {
    // Intentionally silent — a failed log write must never block the UI.
  });
}
