import type { LogEntry, LogLevel } from "@/bindings";

export type LogViewerLevelFilter = "all" | LogLevel;

const LEVEL_RANK: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

export function uniqueLogViewerSources(entries: LogEntry[]): string[] {
  const seen = new Set<string>();
  for (const entry of entries) {
    if (entry.source) seen.add(entry.source);
  }
  return Array.from(seen).sort();
}

export function matchesLogViewerFilter(
  entry: LogEntry,
  level: LogViewerLevelFilter,
  source: string,
  search: string
): boolean {
  if (level !== "all" && LEVEL_RANK[entry.level] < LEVEL_RANK[level]) return false;
  if (source !== "all" && entry.source !== source) return false;
  if (search && !entry.message.toLowerCase().includes(search.toLowerCase())) return false;
  return true;
}
