/**
 * The selected timeline event in full. A crash or ANR shows how it was
 * attributed and offers the log lines kept with it, newest first loaded,
 * older ones a page at a time.
 */
import { type JSX, Show, createEffect, createSignal, on } from "solid-js";
import type { DebugSessionCapture, DebugSessionCrash, DebugSessionEvent } from "@/bindings";
import { formatError, getSessionCapture } from "@/lib/tauri-api";
import {
  Alert,
  Badge,
  Button,
  MetadataCell,
  MetadataGrid,
  Spinner,
  VirtualList,
} from "@/components/ui";
import { getLevelConfig } from "@/components/logcat/logcat-levels";
import {
  actorLabel,
  attributionLabel,
  crashOf,
  describeEvent,
  formatSessionTime,
} from "./session-format";
import styles from "./SessionsDialog.module.css";

/** Log lines read per page of a capture. */
export const CAPTURE_PAGE_LINES = 200;
export const CAPTURE_ROW_HEIGHT = 18;

type CaptureState =
  | { kind: "loading" }
  | { kind: "loaded"; capture: DebugSessionCapture }
  | { kind: "error"; message: string };

function CaptureLines(props: { sessionId: string; seq: number; total: number }): JSX.Element {
  const [limit, setLimit] = createSignal(CAPTURE_PAGE_LINES);
  const [state, setState] = createSignal<CaptureState>({ kind: "loading" });
  const [jumpTo, setJumpTo] = createSignal<number | null>(null);
  let request = 0;

  createEffect(
    on(limit, async (wanted, previous) => {
      const id = ++request;
      try {
        const capture = await getSessionCapture(props.sessionId, props.seq, wanted);
        if (id !== request) return;
        const shown = loaded()?.entries.length ?? 0;
        setState({ kind: "loaded", capture });
        // Keep the line that was first in view when older lines are added.
        if (previous !== undefined) setJumpTo(Math.max(0, capture.entries.length - shown));
      } catch (e) {
        if (id === request) setState({ kind: "error", message: formatError(e) });
      }
    })
  );

  const loaded = () => {
    const s = state();
    return s.kind === "loaded" ? s.capture : null;
  };
  const errorMessage = () => {
    const s = state();
    return s.kind === "error" ? s.message : null;
  };

  return (
    <div class={styles.capture}>
      <Show when={errorMessage()}>
        {(message) => (
          <Alert variant="error" title="Could not read the kept log lines">
            {message()}
          </Alert>
        )}
      </Show>
      <Show when={state().kind === "loading" && !loaded()}>
        <div class={styles.loading}>
          <Spinner size="sm" />
          Reading the kept log lines…
        </div>
      </Show>
      <Show when={loaded()}>
        {(capture) => (
          <>
            <div class={styles.captureHead}>
              <span class={styles.note} role="status">
                Showing the last {capture().entries.length} of {props.total} lines kept with this
                crash
              </span>
              <Button
                variant="outline"
                size="xs"
                disabled={!capture().truncated}
                onClick={() => setLimit((n) => Math.min(props.total, n + CAPTURE_PAGE_LINES))}
              >
                Load older lines
              </Button>
            </div>
            <VirtualList
              items={capture().entries}
              rowHeight={CAPTURE_ROW_HEIGHT}
              autoScroll={jumpTo() === null}
              jumpTo={jumpTo()}
              class={styles.captureLines}
              data-testid="session-capture-lines"
              renderRow={(entry) => {
                const level = getLevelConfig(entry.level);
                return (
                  <div
                    class={styles.captureLine}
                    classList={{ [styles.captureCrash]: entry.isCrash }}
                  >
                    <span class={styles.captureTime}>{entry.timestamp}</span>
                    <span class={styles.captureLevel} style={{ color: level.color }}>
                      {level.label}
                    </span>
                    <span class={styles.captureTag}>{entry.tag}</span>
                    <span class={styles.captureMessage} title={entry.message}>
                      {entry.message}
                    </span>
                  </div>
                );
              }}
            />
          </>
        )}
      </Show>
    </div>
  );
}

function CrashDetail(props: {
  sessionId: string;
  event: DebugSessionEvent;
  crash: DebugSessionCrash;
  captureOpen: boolean;
  onOpenCapture: () => void;
}): JSX.Element {
  return (
    <>
      <MetadataGrid columns={3}>
        <MetadataCell label="Received" value={formatSessionTime(props.crash.receivedAt)} />
        <MetadataCell label="Device time" value={props.crash.deviceTime} />
        <MetadataCell
          label="Process"
          value={props.crash.pid !== null ? `pid ${props.crash.pid}` : "Unknown pid"}
        />
        <MetadataCell label="Attribution" value={attributionLabel(props.crash)} />
        <MetadataCell label="Device" value={props.crash.serial} />
        <MetadataCell
          label="Dropped lines"
          value={String(props.crash.droppedLines)}
          title="Logcat lines the stream had dropped when this was captured"
        />
      </MetadataGrid>
      <Show when={props.crash.attribution.reason}>
        {(reason) => <div class={styles.note}>{reason()}</div>}
      </Show>
      <Show
        when={props.crash.capture}
        fallback={
          <div class={styles.note}>
            No log lines were kept with this crash: a session keeps them for its first 10 crashes.
          </div>
        }
      >
        {(capture) => (
          <Show
            when={props.captureOpen}
            fallback={
              <div>
                <Button variant="outline" size="xs" onClick={() => props.onOpenCapture()}>
                  Show log lines ({capture().entries})
                </Button>
              </div>
            }
          >
            <CaptureLines
              sessionId={props.sessionId}
              seq={props.event.seq}
              total={capture().entries}
            />
          </Show>
        )}
      </Show>
    </>
  );
}

export function SessionEventDetail(props: {
  sessionId: string;
  event: DebugSessionEvent;
  captureOpen: boolean;
  onOpenCapture: () => void;
}): JSX.Element {
  const view = () => describeEvent(props.event);
  const by = () => actorLabel(props.event.actor);
  return (
    <section class={styles.eventDetail} aria-label="Selected event">
      <div class={styles.eventHead}>
        <Badge size="xs" variant={view().variant}>
          {view().label}
        </Badge>
        <span class={styles.timelineTime}>{formatSessionTime(props.event.at)}</span>
        <Show when={by()}>{(label) => <span class={styles.note}>by {label()}</span>}</Show>
      </div>
      <div class={styles.eventText}>{view().text}</div>
      <Show when={props.event.kind === "exit" && props.event.data.record.description}>
        {(description) => <div class={styles.eventText}>{description()}</div>}
      </Show>
      <Show when={crashOf(props.event)}>
        {(crash) => (
          <CrashDetail
            sessionId={props.sessionId}
            event={props.event}
            crash={crash()}
            captureOpen={props.captureOpen}
            onOpenCapture={props.onOpenCapture}
          />
        )}
      </Show>
    </section>
  );
}
