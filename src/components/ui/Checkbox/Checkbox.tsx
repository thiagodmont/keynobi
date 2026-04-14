import { createEffect, type JSX } from "solid-js";
import styles from "./Checkbox.module.css";

export interface CheckboxProps {
  checked: boolean;
  onChange: (val: boolean) => void;
  indeterminate?: boolean;
  disabled?: boolean;
  class?: string;
  children?: JSX.Element;
}

export function Checkbox(props: CheckboxProps): JSX.Element {
  let inputRef!: HTMLInputElement;

  createEffect(() => {
    if (inputRef) inputRef.indeterminate = props.indeterminate ?? false;
  });

  return (
    <label
      class={[
        styles.root,
        props.disabled ? styles.disabled : "",
        props.class,
      ].filter(Boolean).join(" ")}
    >
      <input
        ref={inputRef}
        type="checkbox"
        checked={props.checked}
        disabled={props.disabled}
        aria-checked={props.indeterminate ? "mixed" : props.checked}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
        class={styles.input}
      />
      {props.children}
    </label>
  );
}
