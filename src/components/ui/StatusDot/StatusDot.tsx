import { type JSX } from "solid-js";
import styles from "./StatusDot.module.css";

export type DotStatus = "ok" | "warning" | "error" | "active" | "idle";

export interface StatusDotProps {
  status: DotStatus;
  size?: "sm" | "md";
  class?: string;
}

export function StatusDot(props: StatusDotProps): JSX.Element {
  return (
    <span
      class={[
        styles.root,
        styles[props.status],
        styles[props.size ?? "md"],
        props.class,
      ].filter(Boolean).join(" ")}
      role="img"
      aria-label={props.status}
    />
  );
}
