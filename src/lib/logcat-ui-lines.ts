/** Keep in sync with `models::settings` (`LOGCAT_RING_*`). */
export const LOGCAT_RING_MIN = 1_000;
export const LOGCAT_RING_ABS_MAX = 100_000;
export const LOGCAT_RING_DEFAULT = 50_000;

export const LOGCAT_MIN_UI_LINES = 1_000;
export const LOGCAT_DEFAULT_UI_LINES = 20_000;

export function clampLogcatRingMaxEntries(n: number): number {
  const x = Number.isFinite(n) ? Math.floor(n) : LOGCAT_RING_DEFAULT;
  return Math.min(LOGCAT_RING_ABS_MAX, Math.max(LOGCAT_RING_MIN, x));
}

/** UI line cap cannot exceed the configured ring size. */
export function clampLogcatMaxUiLines(n: number, ringCap: number): number {
  const ring = clampLogcatRingMaxEntries(ringCap);
  const x = Number.isFinite(n) ? Math.floor(n) : LOGCAT_DEFAULT_UI_LINES;
  return Math.min(ring, Math.max(LOGCAT_MIN_UI_LINES, x));
}
