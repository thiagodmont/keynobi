/** Log rows that participate in keyboard / primary selection (not process separators). */
export function isLogcatEntrySelectable(entry: { kind: string }): boolean {
  return entry.kind !== "processDied" && entry.kind !== "processStarted";
}

/**
 * Next selectable index in `entries` from `anchor`, or first/last when `anchor` is null.
 * @param direction `1` = ArrowDown, `-1` = ArrowUp
 */
export function nextSelectableIndex(
  entries: { kind: string }[],
  anchor: number | null,
  direction: 1 | -1
): number | null {
  if (entries.length === 0) return null;
  if (anchor === null) {
    if (direction === 1) {
      for (let i = 0; i < entries.length; i++) {
        if (isLogcatEntrySelectable(entries[i])) return i;
      }
      return null;
    }
    for (let i = entries.length - 1; i >= 0; i--) {
      if (isLogcatEntrySelectable(entries[i])) return i;
    }
    return null;
  }
  if (direction === 1) {
    for (let i = anchor + 1; i < entries.length; i++) {
      if (isLogcatEntrySelectable(entries[i])) return i;
    }
    return null;
  }
  for (let i = anchor - 1; i >= 0; i--) {
    if (isLogcatEntrySelectable(entries[i])) return i;
  }
  return null;
}

/** Clamp selection endpoints to valid row indices; clears both when `anchor` is null. */
export function clampSelectionIndices(
  anchor: number | null,
  end: number | null,
  entryCount: number
): { anchor: number | null; end: number | null } {
  if (entryCount === 0) return { anchor: null, end: null };
  if (anchor === null) return { anchor: null, end: null };
  const max = entryCount - 1;
  const na = Math.min(Math.max(0, anchor), max);
  const nb = end === null ? null : Math.min(Math.max(0, end), max);
  return { anchor: na, end: nb };
}

/** Shift row selection after `dropped` rows were removed from the front. */
export function shiftSelectionAfterFrontDrop(
  anchor: number | null,
  end: number | null,
  dropped: number
): { anchor: number | null; end: number | null } {
  if (anchor === null || dropped <= 0) return { anchor, end: anchor === null ? null : end };

  const shift = (index: number | null): number | null => {
    if (index === null) return null;
    const shifted = index - dropped;
    return shifted >= 0 ? shifted : null;
  };

  if (end !== null) {
    const low = Math.min(anchor, end);
    const high = Math.max(anchor, end);
    if (high < dropped) return { anchor: null, end: null };

    const shiftedLow = Math.max(0, low - dropped);
    const shiftedHigh = high - dropped;
    return anchor <= end
      ? { anchor: shiftedLow, end: shiftedHigh }
      : { anchor: shiftedHigh, end: shiftedLow };
  }

  return {
    anchor: shift(anchor),
    end: shift(end),
  };
}
