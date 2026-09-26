/**
 * A session's timeline, oldest first, one fixed-height row per event, so a
 * session with thousands of events stays responsive. The list is one tab
 * stop: Up/Down, Home, and End move the selection; Enter opens a crash's log
 * lines.
 */
import { type JSX, createSignal } from "solid-js";
import type { DebugSessionEvent } from "@/bindings";
import { Badge, VirtualList, type VirtualListHandle } from "@/components/ui";
import { describeEvent, formatSessionTime } from "./session-format";
import styles from "./SessionsDialog.module.css";

export const TIMELINE_ROW_HEIGHT = 24;

export function timelineRowId(seq: number): string {
  return `session-event-${seq}`;
}

export function SessionTimeline(props: {
  events: DebugSessionEvent[];
  selectedSeq: number | null;
  onSelect: (event: DebugSessionEvent) => void;
  /** Enter on the selected row. */
  onActivate: (event: DebugSessionEvent) => void;
}): JSX.Element {
  // Follows new events until the user scrolls up or moves the selection.
  const [follow, setFollow] = createSignal(true);
  let list: VirtualListHandle | undefined;
  let root!: HTMLDivElement;

  const selectedIndex = () => props.events.findIndex((e) => e.seq === props.selectedSeq);

  function select(index: number): void {
    const event = props.events[index];
    if (!event) return;
    setFollow(false);
    props.onSelect(event);
    const row = root.querySelector<HTMLElement>(`#${timelineRowId(event.seq)}`);
    if (row) row.scrollIntoView?.({ block: "nearest" });
    else list?.scrollToIndex(index);
  }

  function handleKeyDown(e: KeyboardEvent): void {
    const count = props.events.length;
    if (count === 0) return;
    const current = selectedIndex();
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        select(current < 0 ? count - 1 : Math.min(count - 1, current + 1));
        return;
      case "ArrowUp":
        e.preventDefault();
        select(current < 0 ? count - 1 : Math.max(0, current - 1));
        return;
      case "Home":
        e.preventDefault();
        select(0);
        return;
      case "End":
        e.preventDefault();
        select(count - 1);
        return;
      case "Enter": {
        const event = props.events[current];
        if (event) {
          e.preventDefault();
          props.onActivate(event);
        }
        return;
      }
    }
  }

  return (
    <div
      ref={root}
      class={styles.timeline}
      role="listbox"
      aria-label="Timeline, oldest first"
      tabIndex={0}
      aria-activedescendant={
        props.selectedSeq !== null ? timelineRowId(props.selectedSeq) : undefined
      }
      onKeyDown={handleKeyDown}
    >
      <VirtualList
        items={props.events}
        rowHeight={TIMELINE_ROW_HEIGHT}
        autoScroll={follow()}
        onScrolledUp={() => setFollow(false)}
        overscan={20}
        class={styles.timelineScroll}
        handle={(api) => (list = api)}
        renderRow={(event) => {
          const view = describeEvent(event);
          return (
            <div
              id={timelineRowId(event.seq)}
              role="option"
              aria-selected={props.selectedSeq === event.seq ? "true" : "false"}
              class={styles.timelineRow}
              onClick={() => {
                setFollow(false);
                props.onSelect(event);
              }}
            >
              <span class={styles.timelineTime}>{formatSessionTime(event.at)}</span>
              <Badge size="xs" variant={view.variant} subtle class={styles.timelineKind}>
                {view.label}
              </Badge>
              <span class={styles.timelineText} title={view.text}>
                {view.text}
              </span>
            </div>
          );
        }}
      />
    </div>
  );
}
