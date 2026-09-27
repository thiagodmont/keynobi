/**
 * Files attached to a debug session: a strip of screenshot thumbnails and UI
 * hierarchy chips that select their timeline event, and a screenshot's image,
 * read from the backend as base64.
 */
import { type JSX, For, Show, createResource } from "solid-js";
import type { DebugSessionEvent } from "@/bindings";
import { formatError, getSessionAttachment } from "@/lib/tauri-api";
import { Button, Icon, Spinner } from "@/components/ui";
import { formatSessionTime } from "./session-format";
import styles from "./SessionsDialog.module.css";

/** A screenshot of attachment event `seq`, loaded when shown. */
export function AttachmentImage(props: {
  sessionId: string;
  seq: number;
  alt: string;
  class?: string;
}): JSX.Element {
  const [data] = createResource(
    () => ({ id: props.sessionId, seq: props.seq }),
    ({ id, seq }) => getSessionAttachment(id, seq)
  );
  return (
    <Show
      when={!data.error}
      fallback={<span class={styles.fieldError}>{formatError(data.error)}</span>}
    >
      <Show when={data()} fallback={<Spinner size="sm" />}>
        {(loaded) => (
          <img
            class={props.class}
            src={`data:${loaded().mediaType};base64,${loaded().base64}`}
            alt={props.alt}
          />
        )}
      </Show>
    </Show>
  );
}

export function SessionAttachments(props: {
  sessionId: string;
  attachments: DebugSessionEvent[];
  selectedSeq: number | null;
  onSelect: (seq: number) => void;
}): JSX.Element {
  return (
    <div class={styles.attachments} role="group" aria-label="Attachments">
      <For each={props.attachments}>
        {(event) => {
          const hierarchy = event.kind === "attachment" && event.data.kind === "hierarchy";
          const label = `${hierarchy ? "UI hierarchy" : "Screenshot"} from ${formatSessionTime(
            event.at
          )}`;
          return (
            <Button
              variant="ghost"
              class={styles.thumb}
              ariaLabel={label}
              ariaPressed={props.selectedSeq === event.seq}
              title={label}
              onClick={() => props.onSelect(event.seq)}
            >
              <Show
                when={!hierarchy}
                fallback={
                  <span class={styles.thumbHierarchy} aria-hidden="true">
                    <Icon name="list" size={16} />
                    Hierarchy
                  </span>
                }
              >
                <AttachmentImage
                  sessionId={props.sessionId}
                  seq={event.seq}
                  alt=""
                  class={styles.thumbImage}
                />
              </Show>
            </Button>
          );
        }}
      </For>
    </div>
  );
}
