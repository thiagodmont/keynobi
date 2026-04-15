import { type JSX, Show } from "solid-js";
import { Icon } from "@/components/ui/Icon";
import styles from "./EmptyState.module.css";

export interface EmptyStateProps {
  icon: string;
  title: string;
  description?: string;
  action?: JSX.Element;
  class?: string;
}

export function EmptyState(props: EmptyStateProps): JSX.Element {
  return (
    <div class={[styles.root, props.class].filter(Boolean).join(" ")}>
      <div class={styles.icon}>
        <Icon name={props.icon} size={40} />
      </div>
      <div class={styles.title}>{props.title}</div>
      <Show when={props.description}>
        <div class={styles.description}>{props.description}</div>
      </Show>
      <Show when={props.action}>
        <div class={styles.action}>{props.action}</div>
      </Show>
    </div>
  );
}
