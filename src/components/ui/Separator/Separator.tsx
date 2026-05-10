import { type JSX } from "solid-js";
import styles from "./Separator.module.css";

export interface SeparatorProps {
  orientation?: "horizontal" | "vertical";
  spacing?: "sm" | "md";
  class?: string;
  style?: JSX.CSSProperties;
}

export function Separator(props: SeparatorProps): JSX.Element {
  const orientation = () => props.orientation ?? "horizontal";
  const spacingClass = () => {
    if (!props.spacing) return "";
    return props.spacing === "sm" ? styles.spacingSm : styles.spacingMd;
  };

  return (
    <div
      role="separator"
      aria-orientation={orientation()}
      class={[styles.root, styles[orientation()], spacingClass(), props.class]
        .filter(Boolean)
        .join(" ")}
      style={props.style}
    />
  );
}
