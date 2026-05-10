import { type JSX, Show } from "solid-js";
import styles from "./Input.module.css";

export type InputType = "text" | "search" | "number" | "password";
export type InputState = "default" | "error" | "disabled";
export type InputSize = "xs" | "sm" | "md";

export interface InputProps {
  type?: InputType;
  value?: string | number;
  placeholder?: string;
  state?: InputState;
  size?: InputSize;
  mono?: boolean;
  disabled?: boolean;
  clearable?: boolean;
  autofocus?: boolean;
  spellcheck?: boolean;
  prefix?: JSX.Element;
  suffix?: JSX.Element;
  inputRef?: (el: HTMLInputElement) => void;
  onInput?: (val: string) => void;
  onChange?: (val: string) => void;
  onKeyDown?: (e: KeyboardEvent) => void;
  onFocus?: (e: FocusEvent) => void;
  onBlur?: (e: FocusEvent) => void;
  onClick?: (e: MouseEvent) => void;
  onMouseDown?: (e: MouseEvent) => void;
  onClear?: () => void;
  class?: string;
  inputClass?: string;
  style?: JSX.CSSProperties;
  inputStyle?: JSX.CSSProperties;
  title?: string;
  ariaLabel?: string;
}

export function Input(props: InputProps): JSX.Element {
  const isDisabled = () => props.disabled || props.state === "disabled";
  const isError = () => props.state === "error";

  return (
    <div
      class={[
        styles.wrapper,
        props.size === "xs" ? styles.xs : "",
        props.size === "sm" ? styles.sm : "",
        props.mono ? styles.mono : "",
        isError() ? styles.error : "",
        isDisabled() ? styles.disabled : "",
        props.class,
      ]
        .filter(Boolean)
        .join(" ")}
      style={props.style}
    >
      <Show when={props.prefix}>
        <div class={styles.prefix}>{props.prefix}</div>
      </Show>
      <input
        ref={props.inputRef}
        type={props.type ?? "text"}
        value={props.value ?? ""}
        placeholder={props.placeholder}
        title={props.title}
        aria-label={props.ariaLabel}
        disabled={isDisabled()}
        autofocus={props.autofocus}
        spellcheck={props.spellcheck}
        aria-invalid={isError() ? "true" : undefined}
        onInput={(e) => props.onInput?.(e.currentTarget.value)}
        onChange={(e) => props.onChange?.(e.currentTarget.value)}
        onKeyDown={(e) => props.onKeyDown?.(e)}
        onFocus={(e) => props.onFocus?.(e)}
        onBlur={(e) => props.onBlur?.(e)}
        onClick={(e) => props.onClick?.(e)}
        onMouseDown={(e) => props.onMouseDown?.(e)}
        class={[styles.input, props.inputClass].filter(Boolean).join(" ")}
        style={props.inputStyle}
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
