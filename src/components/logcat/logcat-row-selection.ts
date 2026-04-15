import type { LogcatEntry } from "@/lib/tauri-api";

/** Whether `index` lies in the current copy selection range (inclusive). */
export function rowInSelectionRange(
  index: number,
  range: [number, number] | null
): boolean {
  if (range === null) return false;
  return index >= range[0] && index <= range[1];
}

/**
 * Which row gets the primary focus stripe: detail entry, else anchor-only / anchor in a range.
 */
export function rowFocusMarked(
  index: number,
  anchor: number | null,
  end: number | null,
  detail: LogcatEntry | null,
  entryId: bigint
): boolean {
  if (detail !== null) return detail.id === entryId;
  if (anchor === null) return false;
  if (end === null) return anchor === index;
  return anchor === index;
}
