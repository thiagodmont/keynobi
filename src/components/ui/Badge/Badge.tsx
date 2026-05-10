import { type JSX, Show } from "solid-js";
import styles from "./Badge.module.css";

export type BadgeVariant = "default" | "success" | "error" | "warning" | "info" | "accent";
export type BadgeSize = "xs" | "sm";

export interface BadgeProps {
  variant?: BadgeVariant;
  size?: BadgeSize;
  subtle?: boolean;
  dot?: boolean;
  mono?: boolean;
  title?: string;
  ariaLabel?: string;
  class?: string;
  onMouseDown?: (e: MouseEvent) => void;
  onClick?: (e: MouseEvent) => void;
  children: JSX.Element;
}

export function Badge(props: BadgeProps): JSX.Element {
  const clickable = () => props.onClick !== undefined;
  const className = () =>
    [
      styles.root,
      styles[props.variant ?? "default"],
      props.size === "xs" ? styles.xs : "",
      props.subtle ? styles.subtle : "",
      props.mono ? styles.mono : "",
      clickable() ? styles.clickable : "",
      props.class ?? "",
    ].join(" ");

  const content = () => (
    <>
      <Show when={props.dot}>
        <span class={styles.dot} aria-hidden="true" />
      </Show>
      {props.children}
    </>
  );

  return (
    <Show
      when={clickable()}
      fallback={
        <span class={className()} title={props.title}>
          {content()}
        </span>
      }
    >
      <button
        type="button"
        class={className()}
        title={props.title}
        aria-label={props.ariaLabel}
        onMouseDown={(e) => props.onMouseDown?.(e)}
        onClick={(e) => props.onClick?.(e)}
      >
        {content()}
      </button>
    </Show>
  );
}
