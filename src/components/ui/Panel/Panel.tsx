import { type JSX, Show } from "solid-js";
import styles from "./Panel.module.css";

export interface PanelProps {
  title?: string;
  headerActions?: JSX.Element;
  footer?: JSX.Element;
  noPadding?: boolean;
  scrollable?: boolean;
  class?: string;
  children: JSX.Element;
}

export function Panel(props: PanelProps): JSX.Element {
  return (
    <div class={[styles.root, props.class].filter(Boolean).join(" ")}>
      <Show when={props.title !== undefined}>
        <header class={styles.header}>
          <span class={styles.title}>{props.title}</span>
          <Show when={props.headerActions}>
            <div class={styles.actions}>{props.headerActions}</div>
          </Show>
        </header>
      </Show>
      <div
        class={[
          styles.body,
          props.noPadding ? styles.noPadding : "",
          props.scrollable ? styles.scrollable : "",
        ].filter(Boolean).join(" ")}
      >
        {props.children}
      </div>
      <Show when={props.footer}>
        <footer class={styles.footer}>{props.footer}</footer>
      </Show>
    </div>
  );
}
