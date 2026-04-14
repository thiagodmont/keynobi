import { type JSX, Show } from "solid-js";
import styles from "./Alert.module.css";

export type AlertVariant = "info" | "warning" | "error" | "success";

export interface AlertProps {
  variant: AlertVariant;
  title?: string;
  dismissible?: boolean;
  onDismiss?: () => void;
  action?: JSX.Element;
  class?: string;
  children?: JSX.Element;
}

export function Alert(props: AlertProps): JSX.Element {
  return (
    <div
      role="alert"
      class={[styles.root, styles[props.variant], props.class].filter(Boolean).join(" ")}
    >
      <div class={styles.content}>
        <Show when={props.title}>
          <div class={styles.title}>{props.title}</div>
        </Show>
        {props.children}
        <Show when={props.action}>
          <div class={styles.actions}>{props.action}</div>
        </Show>
      </div>
      <Show when={props.dismissible}>
        <button
          type="button"
          class={styles.closeBtn}
          aria-label="Dismiss"
          onClick={props.onDismiss}
        >
          ×
        </button>
      </Show>
    </div>
  );
}
