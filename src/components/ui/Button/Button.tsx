import { type JSX, Show } from "solid-js";
import styles from "./Button.module.css";
import { Spinner } from "@/components/ui/Spinner";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "outline";
export type ButtonSize = "xs" | "sm" | "md";
export type ButtonTone = "default" | "muted" | "accent" | "success" | "warning" | "danger";

export interface ButtonProps {
  variant?: ButtonVariant;
  size?: ButtonSize;
  tone?: ButtonTone;
  loading?: boolean;
  disabled?: boolean;
  class?: string;
  style?: JSX.CSSProperties;
  title?: string;
  ariaPressed?: boolean;
  onClick?: (e: MouseEvent) => void;
  children: JSX.Element;
  type?: "button" | "submit" | "reset";
}

export function Button(props: ButtonProps): JSX.Element {
  const sizeClass = () => {
    if (props.size === "xs") return styles.xs;
    if (props.size === "sm") return styles.sm;
    return "";
  };

  const toneClass = () => {
    const tone = props.tone ?? "default";
    return tone === "default" ? "" : styles[`tone-${tone}`];
  };

  return (
    <button
      type={props.type ?? "button"}
      class={[
        styles.root,
        styles[props.variant ?? "secondary"],
        sizeClass(),
        toneClass(),
        props.class ?? "",
      ].join(" ")}
      style={props.style}
      title={props.title}
      aria-pressed={props.ariaPressed ? "true" : undefined}
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
