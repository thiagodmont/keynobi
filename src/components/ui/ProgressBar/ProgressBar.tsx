import { type JSX } from "solid-js";
import styles from "./ProgressBar.module.css";

export type ProgressVariant = "default" | "success" | "warning";

export interface ProgressBarProps {
  value?: number;
  variant?: ProgressVariant;
  size?: "sm" | "md";
  class?: string;
}

export function ProgressBar(props: ProgressBarProps): JSX.Element {
  const isIndeterminate = () => props.value === undefined;
  const clamped = () => Math.min(100, Math.max(0, props.value ?? 0));
  const variant = () => props.variant ?? "default";

  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={isIndeterminate() ? undefined : clamped()}
      class={[
        styles.root,
        props.size === "md" ? styles.md : "",
        props.class,
      ].filter(Boolean).join(" ")}
    >
      <div
        data-testid="fill"
        class={[
          styles.fill,
          styles[variant()],
          isIndeterminate() ? styles.indeterminate : "",
        ].filter(Boolean).join(" ")}
        style={isIndeterminate() ? {} : { width: `${clamped()}%` }}
      />
    </div>
  );
}
