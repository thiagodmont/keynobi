import { type JSX } from "solid-js";
import styles from "./FilterChip.module.css";

export type FilterChipTone = "accent" | "muted";
export type FilterChipActiveStyle = "solid" | "soft";

export interface FilterChipProps {
  active?: boolean;
  activeStyle?: FilterChipActiveStyle;
  tone?: FilterChipTone;
  title?: string;
  maxWidth?: string;
  ariaPressed?: boolean;
  class?: string;
  onClick: () => void;
  children: JSX.Element;
}

export function FilterChip(props: FilterChipProps): JSX.Element {
  const pressed = () => props.ariaPressed ?? props.active ?? false;

  return (
    <button
      type="button"
      title={props.title}
      aria-pressed={pressed() ? "true" : "false"}
      onClick={() => props.onClick()}
      class={[
        styles.root,
        props.active ? styles.active : "",
        props.active && props.activeStyle === "soft" ? styles.soft : "",
        props.tone === "muted" ? styles.muted : "",
        props.class,
      ]
        .filter(Boolean)
        .join(" ")}
      style={{ "max-width": props.maxWidth }}
    >
      {props.children}
    </button>
  );
}
