import { type JSX } from "solid-js";
import styles from "./Spinner.module.css";

export interface SpinnerProps {
  size?: "sm" | "md" | "lg";
  class?: string;
}

export function Spinner(props: SpinnerProps): JSX.Element {
  return (
    <span
      class={[styles.root, styles[props.size ?? "md"], props.class ?? ""].join(" ")}
      role="status"
      aria-label="Loading"
    />
  );
}
