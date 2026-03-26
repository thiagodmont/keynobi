import { type JSX, onMount, onCleanup } from "solid-js";
import { LogViewer } from "@/components/common/LogViewer";
import { lspLogStore } from "@/stores/log.store";
import { listenLspLog, lspGetLogs } from "@/lib/tauri-api";

/**
 * Bottom-panel tab that streams Kotlin LSP server logs in real time.
 *
 * On mount it:
 *  1. Loads any buffered entries that arrived before the panel was opened.
 *  2. Subscribes to `lsp:log` Tauri events for live updates.
 *
 * The underlying `LogViewer` component is reusable — Logcat will use the
 * same component wired to a different store and event source.
 */
export function LspLogsPanel(): JSX.Element {
  let unlistenFn: (() => void) | undefined;

  onMount(async () => {
    // 1. Hydrate with entries buffered before the panel was first opened.
    try {
      const buffered = await lspGetLogs();
      if (buffered.length > 0) {
        lspLogStore.pushEntries(buffered);
      }
    } catch {
      // LSP may not be running yet — silently ignore.
    }

    // 2. Subscribe to live events.
    unlistenFn = await listenLspLog((entry) => {
      lspLogStore.pushEntry(entry);
    });
  });

  onCleanup(() => {
    unlistenFn?.();
  });

  return (
    <LogViewer
      entries={lspLogStore.entries}
      onClear={lspLogStore.clearEntries}
      showSource={true}
      emptyMessage="No LSP output — start the Kotlin Language Server to see logs"
    />
  );
}
