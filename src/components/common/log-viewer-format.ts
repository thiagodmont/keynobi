import type { LogEntry } from "@/bindings";

export function formatLogViewerTime(iso: string): string {
  try {
    const d = new Date(iso);
    const hh = String(d.getHours()).padStart(2, "0");
    const mm = String(d.getMinutes()).padStart(2, "0");
    const ss = String(d.getSeconds()).padStart(2, "0");
    const ms = String(d.getMilliseconds()).padStart(3, "0");
    return `${hh}:${mm}:${ss}.${ms}`;
  } catch {
    return iso;
  }
}

export function formatLogViewerEntry(
  entry: LogEntry,
  options: { showTimestamp: boolean; showSource: boolean }
): string {
  const parts: string[] = [];
  if (options.showTimestamp) parts.push(formatLogViewerTime(entry.timestamp));
  parts.push(`[${entry.level.toUpperCase()}]`);
  if (options.showSource) parts.push(`[${entry.source}]`);
  parts.push(entry.message);
  return parts.join(" ");
}
