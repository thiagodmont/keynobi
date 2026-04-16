/**
 * Whether the Logcat virtual list should auto-scroll on new entries.
 * User follow-tail preference is suppressed while a row or JSON detail is active.
 */
export function effectiveLogcatFollowTail(params: {
  autoScroll: boolean;
  selectionAnchor: number | null;
  selectedJsonEntry: unknown | null;
}): boolean {
  return (
    params.autoScroll &&
    params.selectionAnchor === null &&
    params.selectedJsonEntry === null
  );
}
