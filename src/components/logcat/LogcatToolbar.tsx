import { Show, type JSX } from "solid-js";
import { Button, ControlStrip, Icon, Separator, StatusDot } from "@/components/ui";
import styles from "./LogcatToolbar.module.css";

export function LogcatToolbar(props: {
  streaming: boolean;
  paused: boolean;
  restarting: boolean;
  crashes: number;
  selectedCount: number;
  autoScroll: boolean;
  newEntriesCount: number;
  toolbarCount: { text: string; title: string };
  onStart: () => void;
  onStop: () => void;
  onTogglePaused: () => void;
  onRestart: () => void;
  onClear: () => void;
  onJumpToLastCrash: () => void;
  onJumpToPreviousCrash: () => void;
  onJumpToNextCrash: () => void;
  onCopySelectedRows: () => void;
  onScrollToEnd: () => void;
  onExport: () => void;
}): JSX.Element {
  return (
    <ControlStrip align="start" wrap>
      <Show
        when={props.streaming}
        fallback={
          <Button
            variant="outline"
            size="xs"
            tone="success"
            onClick={() => props.onStart()}
            title="Start Logcat"
          >
            <Icon name="play" size={13} /> Start
          </Button>
        }
      >
        <Button
          variant="outline"
          size="xs"
          tone="danger"
          onClick={() => props.onStop()}
          title="Stop Logcat"
        >
          <Icon name="stop" size={13} /> Stop
        </Button>
      </Show>

      <Button
        variant="outline"
        size="xs"
        tone={props.paused ? "warning" : "muted"}
        onClick={() => props.onTogglePaused()}
        title={props.paused ? "Resume" : "Pause new entries"}
      >
        <Show when={props.paused} fallback={<Icon name="pause" size={12} />}>
          <Icon name="play" size={12} />
        </Show>
      </Button>

      <Button
        variant="outline"
        size="xs"
        tone="muted"
        onClick={() => props.onRestart()}
        title="Stop, clear and restart logcat"
        disabled={props.restarting}
      >
        <Icon name="refresh" size={12} /> Restart
      </Button>

      <Button
        variant="outline"
        size="xs"
        tone="muted"
        onClick={() => props.onClear()}
        title="Clear logcat buffer"
      >
        <Icon name="trash" size={12} />
      </Button>

      <Separator orientation="vertical" style={{ height: "18px", "align-self": "center" }} />

      <Show when={props.crashes > 0}>
        <div class={styles.crashGroup}>
          <Button
            variant="outline"
            size="xs"
            tone="danger"
            onClick={() => props.onJumpToLastCrash()}
            title={`${props.crashes} crash${props.crashes !== 1 ? "es" : ""} — click to jump`}
            class={styles.crashButton}
          >
            <Icon name="bolt" size={12} /> {props.crashes}
          </Button>
          <Button
            variant="outline"
            size="xs"
            tone="muted"
            onClick={() => props.onJumpToPreviousCrash()}
            title="Previous crash"
          >
            <Icon name="arrow-up" size={12} />
          </Button>
          <Button
            variant="outline"
            size="xs"
            tone="muted"
            onClick={() => props.onJumpToNextCrash()}
            title="Next crash"
          >
            <Icon name="arrow-down" size={12} />
          </Button>
        </div>
      </Show>

      <Show when={props.selectedCount > 0}>
        <Button
          variant="outline"
          size="xs"
          tone="accent"
          onClick={() => props.onCopySelectedRows()}
          title={
            props.selectedCount === 1
              ? "Copy selected row"
              : `Copy ${props.selectedCount} selected rows`
          }
        >
          <Icon name="copy" size={12} />{" "}
          {props.selectedCount === 1 ? "1 row" : `${props.selectedCount} rows`}
        </Button>
      </Show>

      <Button
        variant="outline"
        size="xs"
        tone={props.autoScroll ? "muted" : "accent"}
        onClick={() => props.onScrollToEnd()}
        title={
          props.newEntriesCount > 0
            ? `${props.newEntriesCount.toLocaleString()} new ${
                props.newEntriesCount === 1 ? "log" : "logs"
              } available - Jump to end`
            : "Scroll to end"
        }
      >
        <Icon name="arrow-down" size={12} />
        <Show when={props.newEntriesCount > 0}>
          <span>{props.newEntriesCount.toLocaleString()} new</span>
        </Show>
      </Button>

      <Button
        variant="outline"
        size="xs"
        tone="muted"
        onClick={() => props.onExport()}
        title="Export filtered log to file"
      >
        <Icon name="download" size={12} /> Export
      </Button>

      <div class={styles.spacer} />

      <span title={props.toolbarCount.title} class={styles.count}>
        {props.toolbarCount.text}
      </span>

      <Show when={props.streaming}>
        <StatusDot status="ok" size="sm" />
      </Show>
    </ControlStrip>
  );
}
