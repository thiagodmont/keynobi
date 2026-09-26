/**
 * SessionsDialog — debug sessions (one per install of the app on a device):
 * the list on the left, the selected session's timeline and actions on the
 * right. It reads the session index again every few seconds while open, so
 * sessions an AI client or a standalone MCP server records show up.
 */
import {
  type JSX,
  Match,
  Show,
  Switch,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  untrack,
} from "solid-js";
import { Portal } from "solid-js/web";
import type { DebugSessionSummary } from "@/bindings";
import { formatError, listDebugSessions } from "@/lib/tauri-api";
import { Alert, Badge, Button, EmptyState, Listbox, Spinner, modalFocus } from "@/components/ui";
import {
  STATE_LABELS,
  formatSessionTime,
  isUnattributed,
  sessionBuildLabel,
  sessionCountsLabel,
  sessionDeviceLabel,
  sessionState,
  stateVariant,
} from "./session-format";
import { SessionDetail } from "./SessionDetail";
import styles from "./SessionsDialog.module.css";

/** How often the list is read again while the dialog is open. */
export const SESSIONS_POLL_MS = 3000;

const [open, setOpen] = createSignal(false);
const [requestedId, setRequestedId] = createSignal<string | null>(null);

/** Open the dialog, on `sessionId` when given, else on the newest session. */
export function openSessionsDialog(sessionId?: string): void {
  setRequestedId(sessionId ?? null);
  setOpen(true);
}

export function closeSessionsDialog(): void {
  setOpen(false);
}

export function SessionsDialog(): JSX.Element {
  return (
    <Show when={open()}>
      <Portal>
        <SessionsDialogBody />
      </Portal>
    </Show>
  );
}

function SessionOption(props: { session: DebugSessionSummary }): JSX.Element {
  const state = () => sessionState(props.session);
  return (
    <div class={styles.option}>
      <div class={styles.optionHead}>
        <span class={styles.optionBuild}>{sessionBuildLabel(props.session)}</span>
        <Badge size="xs" variant={stateVariant(state())}>
          {STATE_LABELS[state()]}
        </Badge>
      </div>
      <div class={styles.optionMeta}>
        {sessionDeviceLabel(props.session.device)} · {props.session.package}
      </div>
      <div class={styles.optionMeta}>{sessionCountsLabel(props.session.counts)}</div>
      <Show
        when={
          props.session.kept ||
          props.session.recordedBy === "standalone" ||
          isUnattributed(props.session)
        }
      >
        <div class={styles.badges}>
          <Show when={props.session.kept}>
            <Badge size="xs" variant="accent">
              Kept
            </Badge>
          </Show>
          <Show when={props.session.recordedBy === "standalone"}>
            <Badge size="xs" variant="info" title="Recorded by a standalone MCP server">
              Standalone
            </Badge>
          </Show>
          <Show when={isUnattributed(props.session)}>
            <Badge size="xs" variant="warning" title="Crashes of an app Keynobi did not install">
              Unattributed
            </Badge>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function SessionsDialogBody(): JSX.Element {
  const [sessions, setSessions] = createSignal<DebugSessionSummary[] | null>(null);
  const [listError, setListError] = createSignal<string | null>(null);
  const [selectedId, setSelectedId] = createSignal<string | null>(untrack(requestedId));
  // Drops a list that arrives after a newer one.
  let request = 0;
  let box!: HTMLDivElement;

  async function loadList(): Promise<void> {
    const id = ++request;
    try {
      const list = await listDebugSessions();
      if (id !== request) return;
      const first = sessions() === null;
      setSessions(list);
      setListError(null);
      const selected = selectedId();
      if (selected === null || !list.some((s) => s.id === selected)) {
        setSelectedId(list[0]?.id ?? null);
      }
      // Focus starts on the dialog while the list loads; then it moves to the list.
      if (first && document.activeElement === box) {
        queueMicrotask(() =>
          box.querySelector<HTMLElement>('[role="option"][tabindex="0"]')?.focus()
        );
      }
    } catch (e) {
      if (id === request) setListError(formatError(e));
    }
  }

  onMount(() => {
    void loadList();
    const timer = setInterval(() => void loadList(), SESSIONS_POLL_MS);
    onCleanup(() => {
      clearInterval(timer);
      request++;
    });
  });

  const selected = createMemo(() => sessions()?.find((s) => s.id === selectedId()) ?? null);

  return (
    <div class={styles.backdrop} onClick={closeSessionsDialog}>
      <div
        ref={(el) => {
          box = el;
          modalFocus(el, { onEscape: closeSessionsDialog, initialFocus: () => el });
        }}
        class={styles.box}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="debug-sessions-title"
        onClick={(e) => e.stopPropagation()}
      >
        <div class={styles.header}>
          <h2 id="debug-sessions-title" class={styles.title}>
            Debug Sessions
          </h2>
          <span class={styles.subtitle}>One per install of the app on a device, newest first</span>
        </div>

        <div class={styles.split}>
          <div class={styles.listPane}>
            <Show when={listError() !== null && sessions() !== null}>
              <div class={styles.fieldError} role="alert">
                Could not refresh the list: {listError()}
              </div>
            </Show>
            <Switch>
              <Match when={listError() !== null && sessions() === null}>
                <Alert
                  variant="error"
                  title="Could not read debug sessions"
                  action={
                    <Button variant="outline" size="xs" onClick={() => void loadList()}>
                      Retry
                    </Button>
                  }
                >
                  {listError()}
                </Alert>
              </Match>
              <Match when={sessions() === null}>
                <div class={styles.loading}>
                  <Spinner size="sm" />
                  Reading debug sessions…
                </div>
              </Match>
              <Match when={sessions()?.length === 0}>
                <EmptyState
                  icon="list"
                  title="No debug sessions yet"
                  description="Run App, or an AI client's install_apk, opens one for each install on a device."
                  density="compact"
                />
              </Match>
              <Match when={sessions()}>
                {(list) => (
                  <Listbox
                    items={list()}
                    label="Debug sessions"
                    getKey={(s) => s.id}
                    isSelected={(s) => s.id === selectedId()}
                    getOptionTitle={(s) => `Opened ${formatSessionTime(s.openedAt)}`}
                    onSelect={(s) => setSelectedId(s.id)}
                    class={styles.listbox}
                    optionClass={styles.listOption}
                  >
                    {(session) => <SessionOption session={session()} />}
                  </Listbox>
                )}
              </Match>
            </Switch>
          </div>

          <div class={styles.detailPane}>
            <Show
              when={selected()?.id}
              keyed
              fallback={
                <Show when={sessions()?.length}>
                  <EmptyState icon="list" title="Select a debug session" density="compact" />
                </Show>
              }
            >
              {(id) => <SessionDetail id={id} summary={selected} onChanged={loadList} />}
            </Show>
          </div>
        </div>

        <div class={styles.footer}>
          <Button variant="secondary" size="sm" onClick={closeSessionsDialog}>
            Close
          </Button>
        </div>
      </div>
    </div>
  );
}
