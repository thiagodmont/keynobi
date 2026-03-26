import { createStore, produce } from "solid-js/store";
import type { LogEntry, LogLevel } from "@/bindings";

export type { LogEntry, LogLevel };

interface LogStoreOptions {
  maxEntries?: number;
}

export interface LogStore {
  entries: LogEntry[];
  pushEntry: (entry: LogEntry) => void;
  pushEntries: (entries: LogEntry[]) => void;
  clearEntries: () => void;
}

/**
 * Factory that creates an independent, bounded reactive log store.
 * Call once per log source (LSP, Logcat, build) so each has its own
 * reactive list with its own cap and clear action.
 *
 * @example
 * export const lspLogStore = createLogStore({ maxEntries: 2000 });
 * export const logcatStore = createLogStore({ maxEntries: 5000 });
 */
export function createLogStore(options: LogStoreOptions = {}): LogStore {
  const { maxEntries = 2000 } = options;

  const [entries, setEntries] = createStore<LogEntry[]>([]);

  function pushEntry(entry: LogEntry): void {
    setEntries(
      produce((arr) => {
        arr.push(entry);
        if (arr.length > maxEntries) {
          arr.splice(0, arr.length - maxEntries);
        }
      })
    );
  }

  function pushEntries(newEntries: LogEntry[]): void {
    if (newEntries.length === 0) return;
    setEntries(
      produce((arr) => {
        for (const e of newEntries) arr.push(e);
        if (arr.length > maxEntries) {
          arr.splice(0, arr.length - maxEntries);
        }
      })
    );
  }

  function clearEntries(): void {
    setEntries([]);
  }

  return { entries, pushEntry, pushEntries, clearEntries };
}

/** Shared log store for the Kotlin LSP server. */
export const lspLogStore = createLogStore({ maxEntries: 2000 });
