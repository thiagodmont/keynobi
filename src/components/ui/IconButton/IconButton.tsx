import { type JSX } from "solid-js";
import styles from "./IconButton.module.css";

export interface IconButtonProps {
  title: string;
  onClick: () => void;
  disabled?: boolean;
  active?: boolean;
  size?: "sm" | "md";
  class?: string;
  children: JSX.Element;
}

export function IconButton(props: IconButtonProps): JSX.Element {
  return (
    <button
      type="button"
      title={props.title}
      disabled={props.disabled}
      aria-pressed={props.active ? "true" : undefined}
      class={[
        styles.root,
        props.size === "sm" ? styles.sm : styles.md,
        props.active ? styles.active : "",
        props.class,
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={() => props.onClick()}
    >
      {props.children}
    </button>
  );
}
