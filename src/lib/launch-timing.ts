import type { BuildRecord, LaunchTiming } from "@/bindings";

/** "812 ms (cold)", or "812 ms" when the device did not report the launch state. */
export function formatLaunchTime(timing: LaunchTiming): string {
  const state = timing.launchState ? ` (${timing.launchState})` : "";
  return `${timing.totalMs} ms${state}`;
}

/** "790 ms" under a second, else "1.4 s". */
export function formatDisplayDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}

/**
 * The display times the launch's logcat lines reported, when the logcat
 * stream was reading the device: ["displayed 790 ms", "fully drawn 1.4 s"].
 */
export function describeDisplayTimes(timing: LaunchTiming): string[] {
  const parts: string[] = [];
  if (timing.displayedMs !== null) {
    parts.push(`displayed ${formatDisplayDuration(timing.displayedMs)}`);
  }
  if (timing.fullyDrawnMs !== null) {
    parts.push(`fully drawn ${formatDisplayDuration(timing.fullyDrawnMs)}`);
  }
  return parts;
}

/**
 * Whether two launches ran on the same device. An emulator is identified by
 * its AVD name, since emulator serials are reused by other AVDs; a device
 * without one by its serial.
 */
export function sameLaunchDevice(a: LaunchTiming, b: LaunchTiming): boolean {
  if (a.avdName || b.avdName) return a.avdName === b.avdName;
  return a.serial === b.serial;
}

export interface LaunchComparison {
  /** The earlier build compared with. */
  previousId: number;
  /** This launch minus the earlier one; positive is slower. */
  deltaMs: number;
}

/**
 * Compare `record`'s launch with the most recent earlier build of the same
 * project and task that launched in the same state on the same device.
 * `null` when `record` has no launch time or no earlier build qualifies.
 */
export function compareLaunch(
  history: readonly BuildRecord[],
  record: BuildRecord
): LaunchComparison | null {
  const launch = record.launch;
  if (!launch) return null;
  let previous: BuildRecord | null = null;
  for (const other of history) {
    const earlier = other.launch;
    if (
      earlier &&
      other.id < record.id &&
      (previous === null || other.id > previous.id) &&
      other.projectRoot === record.projectRoot &&
      other.task === record.task &&
      earlier.launchState === launch.launchState &&
      sameLaunchDevice(earlier, launch)
    ) {
      previous = other;
    }
  }
  if (!previous?.launch) return null;
  return { previousId: previous.id, deltaMs: launch.totalMs - previous.launch.totalMs };
}

/** "+54 ms vs #41", "−12 ms vs #41", or "±0 ms vs #41". */
export function formatLaunchDelta(comparison: LaunchComparison): string {
  const { deltaMs, previousId } = comparison;
  const sign = deltaMs > 0 ? "+" : deltaMs < 0 ? "−" : "±";
  return `${sign}${Math.abs(deltaMs)} ms vs #${previousId}`;
}

/** The comparison in words, for assistive technology and tooltips. */
export function describeLaunchDelta(comparison: LaunchComparison): string {
  const { deltaMs, previousId } = comparison;
  if (deltaMs === 0) return `Same launch time as build #${previousId}`;
  const direction = deltaMs > 0 ? "slower" : "faster";
  return `${Math.abs(deltaMs)} ms ${direction} than build #${previousId}`;
}
