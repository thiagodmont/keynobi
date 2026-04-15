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
}): { text: string; title: string } {
  const { queryActive, visible, ringTotal } = params;
  const v = visible.toLocaleString();

  if (ringTotal === null) {
    return {
      text: `${v} lines`,
      title: queryActive
        ? `${RING_SLASH_TITLE} Ring buffer stats are unavailable.`
        : "Lines in the list. Ring buffer stats are unavailable.",
    };
  }

  const r = ringTotal.toLocaleString();

  if (!queryActive && visible === ringTotal) {
    return {
      text: `${v} lines`,
      title:
        "Lines in the logcat ring buffer (all are shown in the list). Green dot: streaming may add more lines.",
    };
  }

  return {
    text: `${v} / ${r}`,
    title: queryActive
      ? RING_SLASH_TITLE
      : `${RING_SLASH_TITLE} No query: the list may show fewer lines than the ring if the UI buffer cap is lower.`,
  };
}
