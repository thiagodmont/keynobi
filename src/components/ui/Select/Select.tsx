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
      value={props.value}
      disabled={props.disabled}
      onChange={(e) => props.onChange(e.currentTarget.value)}
      class={[styles.root, props.class].filter(Boolean).join(" ")}
    >
      <Show when={props.placeholder}>
        <option value="" disabled>{props.placeholder}</option>
      </Show>
      <For each={props.options}>
        {(opt) => <option value={getValue(opt)}>{getLabel(opt)}</option>}
      </For>
    </select>
  );
}
