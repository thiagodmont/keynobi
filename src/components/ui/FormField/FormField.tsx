import { type JSX, Show } from "solid-js";
import styles from "./FormField.module.css";

export interface FormFieldProps {
  label: string;
  description?: string;
  error?: string;
  required?: boolean;
  class?: string;
  children: JSX.Element;
}

export function FormField(props: FormFieldProps): JSX.Element {
  return (
    <div class={[styles.root, props.class].filter(Boolean).join(" ")}>
      <div class={styles.header}>
        <span class={styles.label}>{props.label}</span>
        <Show when={props.required}>
          <span class={styles.required} aria-hidden="true">*</span>
        </Show>
      </div>
      <Show when={props.description}>
        <div class={styles.description}>{props.description}</div>
      </Show>
      {props.children}
      <Show when={props.error}>
        <div class={styles.error} role="alert">{props.error}</div>
      </Show>
    </div>
  );
}
