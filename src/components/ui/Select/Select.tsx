import { type JSX, For, Show } from "solid-js";
import styles from "./Select.module.css";

export type SelectOption = string | { label: string; value: string };

export interface SelectProps {
  value: string;
  options: SelectOption[];
  onChange: (val: string) => void;
  placeholder?: string;
  disabled?: boolean;
  class?: string;
  /** Pairs the select with a `FormField` label (`for`). */
  id?: string;
  /** Accessible name when no visible label names the select. */
  ariaLabel?: string;
  title?: string;
}

function getLabel(opt: SelectOption): string {
  return typeof opt === "string" ? opt : opt.label;
}

function getValue(opt: SelectOption): string {
  return typeof opt === "string" ? opt : opt.value;
}

export function Select(props: SelectProps): JSX.Element {
  return (
    <select
      id={props.id}
      aria-label={props.ariaLabel}
      title={props.title}
      value={props.value}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.currentTarget.value)}
      class={[styles.root, props.class].filter(Boolean).join(" ")}
    >
      <Show when={props.placeholder}>
        <option value="" disabled>
          {props.placeholder}
        </option>
      </Show>
      <For each={props.options}>
        {(opt) => (
          // Marked here too: a re-created option would otherwise lose the selection.
          <option value={getValue(opt)} selected={getValue(opt) === props.value}>
            {getLabel(opt)}
          </option>
        )}
      </For>
    </select>
  );
}
