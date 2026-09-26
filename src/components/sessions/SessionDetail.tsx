/**
 * One debug session: what was installed where, its timeline, the selected
 * event's details (a crash's kept log lines), and the session's actions.
 */
import {
  type JSX,
  Match,
  Show,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  on,
} from "solid-js";
import type { DebugSessionDetail, DebugSessionEvent, DebugSessionSummary } from "@/bindings";
import {
  addSessionBookmark,
  endDebugSession,
  formatError,
  getDebugSession,
  refreshSessionExitReasons,
  setDebugSessionKept,
} from "@/lib/tauri-api";
import { Alert, Badge, Button, Input, MetadataCell, MetadataGrid, Spinner } from "@/components/ui";
import {
  STATE_LABELS,
  actorLabel,
  crashOf,
  formatSessionTime,
  isUnattributed,
  sessionBuildLabel,
  sessionCountsLabel,
  sessionDeviceLabel,
  sessionState,
  stateVariant,
} from "./session-format";
import { SessionTimeline } from "./SessionTimeline";
import { SessionEventDetail } from "./SessionEventDetail";
import styles from "./SessionsDialog.module.css";

/** As `MAX_BOOKMARK_NOTE_CHARS` in the backend. */
export const MAX_BOOKMARK_NOTE_CHARS = 500;

type DetailState =
  | { kind: "loading" }
  | { kind: "loaded"; detail: DebugSessionDetail }
  | { kind: "error"; message: string };

type Action = "keep" | "end" | "bookmark" | "exits";

const BUSY_LABELS: Record<Action, string> = {
  keep: "Saving…",
  end: "Ending the session…",
  bookmark: "Adding the bookmark…",
  exits: "Reading exit reasons from the device…",
};

/** The timeline's events and every crash, even one older than the events returned. */
export function timelineEvents(detail: DebugSessionDetail): DebugSessionEvent[] {
  const seen = new Set(detail.events.map((e) => e.seq));
  const older = detail.crashes.filter((c) => !seen.has(c.seq));
  if (older.length === 0) return detail.events;
  return [...older, ...detail.events].sort((a, b) => a.seq - b.seq);
}

function exitRefreshNote(added: number, message: string | null): string {
  if (message) return message;
  if (added === 0) return "No new process exits for this session.";
  return added === 1 ? "Added 1 process exit." : `Added ${added} process exits.`;
}

export function SessionDetail(props: {
  id: string;
  /** The session as the list last read it; `null` once it left the list. */
  summary: () => DebugSessionSummary | null;
  onChanged: () => Promise<void>;
}): JSX.Element {
  const [state, setState] = createSignal<DetailState>({ kind: "loading" });
  const [selectedSeq, setSelectedSeq] = createSignal<number | null>(null);
  const [captureSeq, setCaptureSeq] = createSignal<number | null>(null);
  const [busy, setBusy] = createSignal<Action | null>(null);
  const [actionError, setActionError] = createSignal<string | null>(null);
  const [actionNote, setActionNote] = createSignal<string | null>(null);
  const [note, setNote] = createSignal("");
  let request = 0;

  async function load(): Promise<void> {
    const id = ++request;
    try {
      const detail = await getDebugSession(props.id);
      if (id === request) setState({ kind: "loaded", detail });
    } catch (e) {
      if (id === request) setState({ kind: "error", message: formatError(e) });
    }
  }

  // Read again when the list shows the session changed (new events, kept, closed).
  const version = () => {
    const s = props.summary();
    return s ? `${s.eventCount}|${s.lastEventAt}|${s.closedAt}|${s.kept}|${s.droppedEvents}` : "";
  };
  createEffect(on(version, () => void load()));

  const detail = () => {
    const s = state();
    return s.kind === "loaded" ? s.detail : null;
  };
  const loadError = () => {
    const s = state();
    return s.kind === "error" ? s.message : null;
  };
  const events = createMemo(() => {
    const d = detail();
    return d ? timelineEvents(d) : [];
  });
  const selectedEvent = () => events().find((e) => e.seq === selectedSeq()) ?? null;

  const summary = () => props.summary();
  const open = () => summary()?.closedAt === null;
  const kept = () => summary()?.kept ?? false;

  const noteLength = () => [...note().trim()].length;
  const noteError = () =>
    noteLength() > MAX_BOOKMARK_NOTE_CHARS
      ? `A bookmark note is at most ${MAX_BOOKMARK_NOTE_CHARS} characters.`
      : null;

  let actionsEl!: HTMLDivElement;
  let noteInput: HTMLInputElement | undefined;

  // Buttons stay enabled while an action runs, so focus never falls out of the
  // dialog; a second action waits for the first.
  async function run(action: Action, work: () => Promise<string | null>): Promise<void> {
    if (busy() !== null) return;
    setBusy(action);
    setActionError(null);
    setActionNote(null);
    try {
      setActionNote(await work());
      await props.onChanged();
    } catch (e) {
      setActionError(formatError(e));
    } finally {
      setBusy(null);
      // A control that became disabled (End session once closed) dropped focus.
      if (!document.activeElement || document.activeElement === document.body) {
        actionsEl?.querySelector<HTMLElement>("button:not([disabled])")?.focus();
      }
    }
  }

  function toggleKept(): Promise<void> {
    const id = props.id;
    const keep = !kept();
    return run("keep", async () => {
      await setDebugSessionKept(id, keep);
      return null;
    });
  }

  function endSession(): Promise<void> {
    const id = props.id;
    return run("end", async () => {
      await endDebugSession(id);
      return null;
    });
  }

  function refreshExits(): Promise<void> {
    const id = props.id;
    return run("exits", async () => {
      const result = await refreshSessionExitReasons(id);
      return exitRefreshNote(result.added, result.message);
    });
  }

  function addBookmark(): Promise<void> | undefined {
    const text = note().trim();
    if (text === "" || noteError()) return;
    const id = props.id;
    return run("bookmark", async () => {
      const event = await addSessionBookmark(text, { sessionId: id });
      setNote("");
      setSelectedSeq(event.seq);
      noteInput?.focus();
      return null;
    });
  }

  function activate(event: DebugSessionEvent): void {
    setSelectedSeq(event.seq);
    if (crashOf(event)?.capture) setCaptureSeq(event.seq);
  }

  return (
    <div class={styles.detail}>
      <Show when={summary()}>
        {(s) => (
          <div class={styles.detailHeader}>
            <div class={styles.detailTitle}>
              <h3 class={styles.package}>{s().package}</h3>
              <Badge size="xs" variant={stateVariant(sessionState(s()))}>
                {STATE_LABELS[sessionState(s())]}
              </Badge>
              <Show when={s().kept}>
                <Badge size="xs" variant="accent">
                  Kept
                </Badge>
              </Show>
              <Show when={s().recordedBy === "standalone"}>
                <Badge size="xs" variant="info" title="Recorded by a standalone MCP server">
                  Standalone
                </Badge>
              </Show>
              <Show when={isUnattributed(s())}>
                <Badge
                  size="xs"
                  variant="warning"
                  title="Crashes of an app Keynobi did not install"
                >
                  Unattributed
                </Badge>
              </Show>
            </div>
            <MetadataGrid columns={3}>
              <MetadataCell
                label="Device"
                value={sessionDeviceLabel(s().device)}
                title={s().device.serial}
              />
              <MetadataCell
                label="Build"
                value={
                  detail()?.session.build
                    ? `${sessionBuildLabel(s())} · ${detail()?.session.build?.task}`
                    : sessionBuildLabel(s())
                }
              />
              <MetadataCell
                label="APK"
                value={
                  s().apkSha256
                    ? `${s().apkSha256?.slice(0, 12)}…${
                        s().versionCode !== null ? ` · version code ${s().versionCode}` : ""
                      }`
                    : "None"
                }
                title={s().apkSha256 ?? undefined}
              />
              <MetadataCell
                label="Installed"
                value={
                  detail()?.session.install
                    ? `${formatSessionTime(detail()?.session.install?.installedAt ?? "")} by ${
                        actorLabel(detail()?.session.install?.by ?? null) ?? "Keynobi"
                      }`
                    : "Not installed by Keynobi"
                }
              />
              <MetadataCell label="Opened" value={formatSessionTime(s().openedAt)} />
              <MetadataCell
                label={s().closedAt ? STATE_LABELS[sessionState(s())] : "Last event"}
                value={formatSessionTime(s().closedAt ?? s().lastEventAt)}
              />
            </MetadataGrid>
            <div class={styles.counts}>
              {sessionCountsLabel(s().counts)} · {s().counts.exits} exits · {s().counts.bookmarks}{" "}
              bookmarks
              <Show when={s().droppedEvents > 0}>
                {" "}
                · {s().droppedEvents} events not recorded (session full)
              </Show>
            </div>
          </div>
        )}
      </Show>

      <div class={styles.actions} ref={actionsEl}>
        <Button
          variant={kept() ? "primary" : "outline"}
          size="xs"
          ariaPressed={kept()}
          title="A kept session is not removed by age and keeps its R8 mapping (at most 5)"
          onClick={() => void toggleKept()}
        >
          Keep
        </Button>
        <Button
          variant="outline"
          size="xs"
          disabled={!open()}
          title={open() ? "Close this session; the next install opens a new one" : "Closed"}
          onClick={() => void endSession()}
        >
          End session
        </Button>
        <Button
          variant="outline"
          size="xs"
          title="Read why the app's processes exited from the device (Android 11+)"
          onClick={() => void refreshExits()}
        >
          Refresh exit reasons
        </Button>
      </div>

      <div class={styles.bookmark}>
        <Input
          class={styles.bookmarkInput}
          size="xs"
          value={note()}
          placeholder={open() ? "Note what you just did or saw" : "This session is closed"}
          ariaLabel="Bookmark note"
          inputRef={(el) => (noteInput = el)}
          state={noteError() ? "error" : undefined}
          disabled={!open()}
          onInput={setNote}
          onKeyDown={(e) => {
            if (e.key === "Enter") void addBookmark();
          }}
        />
        <span class={styles.noteCount} aria-hidden="true">
          {noteLength()}/{MAX_BOOKMARK_NOTE_CHARS}
        </span>
        <Button
          variant="outline"
          size="xs"
          disabled={!open() || noteLength() === 0 || noteError() !== null}
          onClick={() => void addBookmark()}
        >
          Add bookmark
        </Button>
      </div>
      <Show when={noteError()}>
        {(message) => (
          <div class={styles.fieldError} role="alert">
            {message()}
          </div>
        )}
      </Show>
      <Show when={actionError()}>
        {(message) => (
          <Alert variant="error" dismissible onDismiss={() => setActionError(null)}>
            {message()}
          </Alert>
        )}
      </Show>
      <div class={styles.status} role="status">
        <Show when={busy()} fallback={actionNote()}>
          {(action) => (
            <>
              <Spinner size="sm" />
              {BUSY_LABELS[action()]}
            </>
          )}
        </Show>
      </div>

      <Switch>
        <Match when={state().kind === "loading"}>
          <div class={styles.loading}>
            <Spinner size="sm" />
            Reading the timeline…
          </div>
        </Match>
        <Match when={loadError()}>
          {(message) => (
            <Alert variant="error" title="Could not read this session">
              {message()}
            </Alert>
          )}
        </Match>
        <Match when={detail()}>
          {(d) => (
            <>
              <Show when={d().eventsTruncated}>
                <div class={styles.note}>
                  Showing the newest {d().events.length} of {d().session.eventCount} events, and
                  every crash.
                </div>
              </Show>
              <SessionTimeline
                events={events()}
                selectedSeq={selectedSeq()}
                onSelect={(event) => setSelectedSeq(event.seq)}
                onActivate={activate}
              />
              {/* Keyed by seq, so a timeline read again keeps the open capture's page. */}
              <Show when={selectedEvent()?.seq} keyed>
                {(seq) => (
                  <Show when={selectedEvent()}>
                    {(event) => (
                      <SessionEventDetail
                        sessionId={props.id}
                        event={event()}
                        captureOpen={captureSeq() === seq}
                        onOpenCapture={() => setCaptureSeq(seq)}
                      />
                    )}
                  </Show>
                )}
              </Show>
            </>
          )}
        </Match>
      </Switch>
    </div>
  );
}
