/**
 * Whether the Logcat virtual list should auto-scroll on new entries.
 * User follow-tail preference is suppressed while a row, entry detail, or JSON detail is active.
 */
export function effectiveLogcatFollowTail(params: {
  autoScroll: boolean;
  selectionAnchor: number | null;
  selectedJsonEntry: unknown | null;
  selectedDetailEntry?: unknown | null;
}): boolean {
  return (
    params.autoScroll &&
    params.selectionAnchor === null &&
    (params.selectedDetailEntry === null || params.selectedDetailEntry === undefined) &&
    params.selectedJsonEntry === null
  );
}
