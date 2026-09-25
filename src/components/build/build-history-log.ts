import { type Accessor, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { lineToLogEntry } from "@/stores/build.store";
import type { LogEntry } from "@/stores/log.store";
import { formatError, getBuildLogEntries, isAppErrorKind } from "@/lib/tauri-api";

/** What the Build panel can show for the saved log of a past build. */
export type HistoricalLogState =
  | { status: "none" }
  | { status: "loading" }
  | { status: "loaded"; entries: LogEntry[] }
  /** Log rotation removed the file (by age or folder size). */
  | { status: "expired" }
  | { status: "failed"; message: string };

/**
 * Load the saved log of the build `id()` names, again on `retry()`. A response
 * for a build that is no longer selected is dropped.
 */
export function createHistoricalLog(id: Accessor<number | null>): {
  state: Accessor<HistoricalLogState>;
  retry: () => void;
} {
  const [state, setState] = createSignal<HistoricalLogState>({ status: "none" });
  const [attempt, setAttempt] = createSignal(0);
  // Reload only when the ID itself changes, not whenever its inputs recompute.
  const currentId = createMemo(id);

  createEffect(() => {
    const buildId = currentId();
    void attempt();
    let stale = false;
    onCleanup(() => {
      stale = true;
    });
    if (buildId === null) {
      setState({ status: "none" });
      return;
    }
    setState({ status: "loading" });
    getBuildLogEntries(buildId)
      .then((lines) => {
        if (!stale) setState({ status: "loaded", entries: lines.map(lineToLogEntry) });
      })
      .catch((e) => {
        if (stale) return;
        setState(
          isAppErrorKind(e, "notFound")
            ? { status: "expired" }
            : { status: "failed", message: formatError(e) }
        );
      });
  });

  return { state, retry: () => setAttempt((n) => n + 1) };
}
