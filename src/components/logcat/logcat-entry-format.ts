import type { LogcatEntry } from "@/lib/tauri-api";

export function formatLogcatEntry(entry: LogcatEntry): string {
  const pkg = entry.package ? `[${entry.package}] ` : "";
  return `${entry.timestamp}  ${entry.level.toUpperCase()}  ${pkg}${entry.tag}: ${entry.message}`;
}

export function formatLogcatEntries(entries: LogcatEntry[]): string {
  return entries.map(formatLogcatEntry).join("\n");
}
