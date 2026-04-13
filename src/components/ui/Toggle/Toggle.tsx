import { type JSX } from "solid-js";
import styles from "./Toggle.module.css";

export interface ToggleProps {
  checked: boolean;
  onChange: (val: boolean) => void;
  disabled?: boolean;
  size?: "sm" | "md";
  class?: string;
}

export function Toggle(props: ToggleProps): JSX.Element {
  const size = () => props.size ?? "md";

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      if (!props.disabled) props.onChange(!props.checked);
    }
  }

  return (
    <button
      role="switch"
      aria-checked={props.checked ? "true" : "false"}
      disabled={props.disabled}
      onClick={() => { if (!props.disabled) props.onChange(!props.checked); }}
      onKeyDown={handleKeyDown}
      class={[
        styles.track,
        size() === "sm" ? styles.sm : styles.md,
        props.checked ? styles.on : styles.off,
        props.class,
      ].filter(Boolean).join(" ")}
    >
      <span
        class={[
          styles.thumb,
          size() === "sm" ? styles.thumbSm : styles.thumbMd,
          props.checked
            ? (size() === "sm" ? styles.thumbOnSm : styles.thumbOnMd)
            : "",
        ].filter(Boolean).join(" ")}
      />
    </button>
  );
}
