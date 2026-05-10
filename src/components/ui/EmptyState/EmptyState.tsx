import { type JSX, Show } from "solid-js";
import { Icon } from "@/components/ui/Icon";
import styles from "./EmptyState.module.css";

export interface EmptyStateProps {
  icon: string;
  title: string;
  description?: string;
  action?: JSX.Element;
  density?: "normal" | "compact";
  class?: string;
}

export function EmptyState(props: EmptyStateProps): JSX.Element {
  const compact = () => props.density === "compact";

  return (
    <div
      class={[styles.root, compact() ? styles.compact : "", props.class].filter(Boolean).join(" ")}
    >
      <div class={styles.icon}>
        <Icon name={props.icon} size={compact() ? 24 : 40} />
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
