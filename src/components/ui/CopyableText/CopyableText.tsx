import { type JSX, Show } from "solid-js";
import { Icon } from "@/components/ui/Icon";
import styles from "./CopyableText.module.css";

export interface CopyableTextProps {
  text: string;
  truncate?: boolean;
  mono?: boolean;
  iconOnly?: boolean;
  class?: string;
}

export function CopyableText(props: CopyableTextProps): JSX.Element {
  function handleCopy() {
    navigator.clipboard?.writeText(props.text).catch(() => {});
  }

  return (
    <span class={[styles.root, props.class].filter(Boolean).join(" ")}>
      <Show when={!props.iconOnly}>
        <span
          class={[
            styles.text,
            props.mono ? styles.mono : "",
          ].filter(Boolean).join(" ")}
          title={props.truncate ? props.text : undefined}
        >
          {props.text}
        </span>
      </Show>
      <button
        type="button"
        class={styles.copyBtn}
        onClick={handleCopy}
        aria-label="Copy"
      >
        <Icon name="copy" size={12} />
      </button>
    </span>
  );
}
