import { type JSX, Show } from "solid-js";
import styles from "./Input.module.css";

export type InputType = "text" | "search" | "number" | "password";
export type InputState = "default" | "error" | "disabled";

export interface InputProps {
  type?: InputType;
  value?: string | number;
  placeholder?: string;
  state?: InputState;
  disabled?: boolean;
  clearable?: boolean;
  prefix?: JSX.Element;
  suffix?: JSX.Element;
  onInput?: (val: string) => void;
  onChange?: (val: string) => void;
  onClear?: () => void;
  class?: string;
}

export function Input(props: InputProps): JSX.Element {
  const isDisabled = () => props.disabled || props.state === "disabled";
  const isError = () => props.state === "error";

  return (
    <div
      class={[
        styles.wrapper,
        isError() ? styles.error : "",
        isDisabled() ? styles.disabled : "",
        props.class,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <Show when={props.prefix}>
        <div class={styles.prefix}>{props.prefix}</div>
      </Show>
      <input
        type={props.type ?? "text"}
        value={props.value ?? ""}
        placeholder={props.placeholder}
        disabled={isDisabled()}
        aria-invalid={isError() ? "true" : undefined}
        onInput={(e) => props.onInput?.(e.currentTarget.value)}
        onChange={(e) => props.onChange?.(e.currentTarget.value)}
        class={styles.input}
      />
      <Show when={props.suffix}>
        <div class={styles.suffix}>{props.suffix}</div>
      </Show>
      <Show when={props.clearable && props.value}>
        <button
          type="button"
          class={styles.clearBtn}
          onClick={() => props.onClear?.()}
          aria-label="Clear"
        >
          ×
        </button>
      </Show>
    </div>
  );
}
