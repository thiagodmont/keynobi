import { type JSX, Show } from "solid-js";
import type { BuildRecord } from "@/bindings";
import {
  compareLaunch,
  describeLaunchDelta,
  formatLaunchDelta,
  formatLaunchTime,
} from "@/lib/launch-timing";
import styles from "./LaunchTimingSummary.module.css";

/**
 * "Launch 812 ms (cold) · +54 ms vs #41" for a build Run App launched. The
 * comparison is with the last earlier launch of the same task in the same
 * state on the same device; its sign says slower or faster, not only its color.
 */
export function LaunchTimingSummary(props: {
  record: BuildRecord;
  history: readonly BuildRecord[];
}): JSX.Element {
  const comparison = () => compareLaunch(props.history, props.record);
  const deltaClass = (deltaMs: number) =>
    deltaMs > 0 ? styles.slower : deltaMs < 0 ? styles.faster : undefined;

  return (
    <Show when={props.record.launch}>
      {(launch) => (
        <span class={styles.summary} data-testid="launch-timing">
          <span>Launch {formatLaunchTime(launch())}</span>
          <Show when={comparison()}>
            {(c) => (
              <>
                <span aria-hidden="true"> · </span>
                <span class={deltaClass(c().deltaMs)} title={describeLaunchDelta(c())}>
                  {formatLaunchDelta(c())}
                </span>
              </>
            )}
          </Show>
        </span>
      )}
    </Show>
  );
}
