const RING_SLASH_TITLE =
  "First: lines shown in the list after all filters. Second: total lines in the app logcat ring buffer (includes lines not forwarded to the list because of the stream filter).";

/**
 * Logcat toolbar line count.
 * Denominator is the Rust `LogStore` size from `LogStats.bufferEntryCount` when available.
 */
export function formatLogcatToolbarCount(params: {
  queryActive: boolean;
  visible: number;
  ringTotal: number | null;
  droppedLines?: number;
}): { text: string; title: string } {
  const { queryActive, visible, ringTotal, droppedLines = 0 } = params;
  const v = visible.toLocaleString();

  // Dropped lines mean the view is incomplete — say so rather than showing a
  // silent gap. Appended to whichever count form we return below.
  const dropSuffix = droppedLines > 0 ? `  ⚠ ${droppedLines.toLocaleString()} dropped` : "";
  const dropTitle =
    droppedLines > 0
      ? ` ${droppedLines.toLocaleString()} line(s) were dropped because the backend ingest channel saturated — the list is incomplete.`
      : "";

  if (ringTotal === null) {
    return {
      text: `${v} lines${dropSuffix}`,
      title:
        (queryActive
          ? `${RING_SLASH_TITLE} Ring buffer stats are unavailable.`
          : "Lines in the list. Ring buffer stats are unavailable.") + dropTitle,
    };
  }

  const r = ringTotal.toLocaleString();

  if (!queryActive && visible === ringTotal) {
    return {
      text: `${v} lines${dropSuffix}`,
      title:
        "Lines in the logcat ring buffer (all are shown in the list). Green dot: streaming may add more lines." +
        dropTitle,
    };
  }

  return {
    text: `${v} / ${r}${dropSuffix}`,
    title:
      (queryActive
        ? RING_SLASH_TITLE
        : `${RING_SLASH_TITLE} No query: the list may show fewer lines than the ring if the UI buffer cap is lower.`) +
      dropTitle,
  };
}
