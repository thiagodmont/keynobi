import { type JSX, Show } from "solid-js";
import styles from "./Button.module.css";
import { Spinner } from "@/components/ui/Spinner";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md";

export interface ButtonProps {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  disabled?: boolean;
  class?: string;
  onClick?: (e: MouseEvent) => void;
  children: JSX.Element;
  type?: "button" | "submit" | "reset";
}

export function Button(props: ButtonProps): JSX.Element {
  return (
    <button
      type={props.type ?? "button"}
      class={[
        styles.root,
        styles[props.variant ?? "secondary"],
        props.size === "sm" ? styles.sm : "",
        props.class ?? "",
      ].join(" ")}
      disabled={props.disabled || props.loading}
      onClick={(e) => props.onClick?.(e)}
    >
      <Show when={props.loading}>
        <Spinner size="sm" />
      </Show>
      {props.children}
    </button>
  );
}
