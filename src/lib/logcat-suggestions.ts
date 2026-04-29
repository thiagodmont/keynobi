import type { LogcatEntry } from "@/lib/tauri-api";

export interface LogcatSuggestionIndexOptions {
  maxPackages: number;
  maxTags: number;
  maxVisibleTags: number;
}

export interface LogcatSuggestionIndex {
  ingest(entries: LogcatEntry[]): void;
  packages(): string[];
  tags(): string[];
  clear(): void;
}

function rememberCappedKey<T>(map: Map<T, number>, key: T, value: number, cap: number): void {
  if (map.has(key)) {
    map.set(key, value);
    return;
  }
  map.set(key, value);
  while (map.size > cap) {
    const oldest = map.keys().next().value as T | undefined;
    if (oldest === undefined) break;
    map.delete(oldest);
  }
}

export function createLogcatSuggestionIndex(
  options: LogcatSuggestionIndexOptions
): LogcatSuggestionIndex {
  const packageSeenAt = new Map<string, number>();
  const tagFreqMap = new Map<string, number>();
  let sequence = 0;

  return {
    ingest(entries: LogcatEntry[]) {
      for (const entry of entries) {
        sequence += 1;
        if (entry.package) {
          rememberCappedKey(packageSeenAt, entry.package, sequence, options.maxPackages);
        }
        if (!entry.kind || entry.kind === "normal") {
          const nextCount = (tagFreqMap.get(entry.tag) ?? 0) + 1;
          if (tagFreqMap.has(entry.tag) || tagFreqMap.size < options.maxTags) {
            tagFreqMap.set(entry.tag, nextCount);
          }
        }
      }
    },
    packages() {
      return Array.from(packageSeenAt.entries())
        .map(([pkg]) => pkg)
        .sort();
    },
    tags() {
      return Array.from(tagFreqMap.entries())
        .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
        .slice(0, options.maxVisibleTags)
        .map(([tag]) => tag);
    },
    clear() {
      packageSeenAt.clear();
      tagFreqMap.clear();
      sequence = 0;
    },
  };
}
