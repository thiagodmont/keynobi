import type { LogcatEntry } from "@/lib/tauri-api";

export function isLifecycleLogcatEntry(entry: LogcatEntry): boolean {
  return entry.category === "lifecycle" || entry.kind !== "normal";
}
