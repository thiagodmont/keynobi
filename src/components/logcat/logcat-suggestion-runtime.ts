import { createSignal, onCleanup, type Accessor } from "solid-js";
import type { LogcatEntry } from "@/lib/tauri-api";
import { createLogcatSuggestionIndex } from "@/lib/logcat-suggestions";

export interface LogcatSuggestionRuntime {
  knownPackages: Accessor<string[]>;
  knownTags: Accessor<string[]>;
  ingest(entries: LogcatEntry[]): void;
  flush(immediate?: boolean): void;
  clear(): void;
}

export function createLogcatSuggestionRuntime(): LogcatSuggestionRuntime {
  const [knownPackages, setKnownPackages] = createSignal<string[]>([]);
  const [knownTags, setKnownTags] = createSignal<string[]>([]);
  let suggestTimer: ReturnType<typeof setTimeout> | null = null;
  const suggestionIndex = createLogcatSuggestionIndex({
    maxPackages: 500,
    maxTags: 500,
    maxVisibleTags: 50,
  });

  function clearTimer(): void {
    if (suggestTimer !== null) {
      clearTimeout(suggestTimer);
      suggestTimer = null;
    }
  }

  function publishSuggestions(): void {
    suggestTimer = null;
    setKnownPackages(suggestionIndex.packages());
    setKnownTags(suggestionIndex.tags());
  }

  function clear(): void {
    suggestionIndex.clear();
    setKnownPackages([]);
    setKnownTags([]);
    clearTimer();
  }

  onCleanup(clearTimer);

  return {
    knownPackages,
    knownTags,
    ingest(entries) {
      suggestionIndex.ingest(entries);
    },
    flush(immediate = false) {
      if (suggestTimer !== null && !immediate) return;
      clearTimer();
      suggestTimer = setTimeout(publishSuggestions, immediate ? 0 : 3_000);
    },
    clear,
  };
}
