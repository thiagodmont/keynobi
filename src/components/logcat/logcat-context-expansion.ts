import type { LogcatEntry } from "@/lib/tauri-api";

export const LOGCAT_CONTEXT_EXPAND_COUNT = 10;
export const LOGCAT_MAX_EXPANDED_CONTEXT_ENTRIES = 500;

function compareByLogcatOrder(a: LogcatEntry, b: LogcatEntry): number {
  if (a.id < b.id) return -1;
  if (a.id > b.id) return 1;
  return 0;
}

export function mergeLogcatEntriesChronologically(
  primaryEntries: LogcatEntry[],
  contextEntries: LogcatEntry[]
): LogcatEntry[] {
  const byId = new Map<LogcatEntry["id"], LogcatEntry>();
  for (const entry of contextEntries) {
    byId.set(entry.id, entry);
  }
  for (const entry of primaryEntries) {
    byId.set(entry.id, entry);
  }
  return Array.from(byId.values()).sort(compareByLogcatOrder);
}

export function mergeExpandedContextEntries(
  currentEntries: LogcatEntry[],
  incomingEntries: LogcatEntry[],
  maxEntries = LOGCAT_MAX_EXPANDED_CONTEXT_ENTRIES
): LogcatEntry[] {
  const byId = new Map<LogcatEntry["id"], LogcatEntry>();
  for (const entry of currentEntries) {
    byId.set(entry.id, entry);
  }
  for (const entry of incomingEntries) {
    if (byId.has(entry.id)) continue;
    if (byId.size >= maxEntries) break;
    byId.set(entry.id, entry);
  }
  return Array.from(byId.values()).sort(compareByLogcatOrder);
}

export function isExpandedContextRow(
  entryId: LogcatEntry["id"],
  expandedContextIds: ReadonlySet<LogcatEntry["id"]>,
  filteredEntryIds: ReadonlySet<LogcatEntry["id"]>
): boolean {
  return expandedContextIds.has(entryId) && !filteredEntryIds.has(entryId);
}
