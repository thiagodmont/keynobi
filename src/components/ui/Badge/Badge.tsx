import { type JSX, Show } from "solid-js";
import styles from "./Badge.module.css";

export type BadgeVariant = "default" | "success" | "error" | "warning" | "info" | "accent";

export interface BadgeProps {
  variant?: BadgeVariant;
  subtle?: boolean;
  dot?: boolean;
  class?: string;
  children: JSX.Element;
}

export function Badge(props: BadgeProps): JSX.Element {
  return (
    <span
      class={[
        styles.root,
        styles[props.variant ?? "default"],
        props.subtle ? styles.subtle : "",
        props.class ?? "",
      ].join(" ")}
    >
      <Show when={props.dot}>
        <span class={styles.dot} aria-hidden="true" />
      </Show>
      {props.children}
    </span>
  );
}
