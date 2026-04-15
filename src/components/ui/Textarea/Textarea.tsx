import { type JSX } from "solid-js";
import styles from "./Textarea.module.css";

export type TextareaState = "default" | "error" | "disabled";

export interface TextareaProps {
  value?: string;
  placeholder?: string;
  rows?: number;
  resize?: "none" | "vertical";
  mono?: boolean;
  state?: TextareaState;
  disabled?: boolean;
  onInput?: (val: string) => void;
  onChange?: (val: string) => void;
  class?: string;
}

export function Textarea(props: TextareaProps): JSX.Element {
  const isDisabled = () => props.disabled || props.state === "disabled";
  const isError = () => props.state === "error";

  const resizeClass = () => {
    if (props.resize === "none") return styles.resizeNone;
    if (props.resize === "vertical") return styles.resizeVertical;
    return "";
  };

  return (
    <textarea
      value={props.value ?? ""}
      placeholder={props.placeholder}
      rows={props.rows}
      disabled={isDisabled()}
      aria-invalid={isError() ? "true" : undefined}
      onInput={(e) => props.onInput?.(e.currentTarget.value)}
      onChange={(e) => props.onChange?.(e.currentTarget.value)}
      class={[
        styles.root,
        isError() ? styles.error : "",
        resizeClass(),
        props.mono ? styles.mono : "",
        props.class,
      ].filter(Boolean).join(" ")}
    />
  );
}
