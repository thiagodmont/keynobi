import { createSignal, type JSX, For } from "solid-js";
import styles from "./TagInput.module.css";

export interface TagInputProps {
  tags: string[];
  onChange: (tags: string[]) => void;
  placeholder?: string;
  max?: number;
  class?: string;
}

export function TagInput(props: TagInputProps): JSX.Element {
  const [inputValue, setInputValue] = createSignal("");

  function addTag() {
    const val = inputValue().trim();
    if (!val) return;
    if (props.tags.includes(val)) return;
    if (props.max !== undefined && props.tags.length >= props.max) return;
    props.onChange([...props.tags, val]);
    setInputValue("");
  }

  function removeTag(tag: string) {
    props.onChange(props.tags.filter((t) => t !== tag));
  }

  return (
    <div class={[styles.root, props.class].filter(Boolean).join(" ")}>
      <div class={styles.tagList}>
        <For each={props.tags}>
          {(tag) => (
            <span class={styles.tag}>
              {tag}
              <button
                class={styles.removeBtn}
                onClick={() => removeTag(tag)}
                aria-label={`Remove ${tag}`}
                type="button"
              >
                ×
              </button>
            </span>
          )}
        </For>
      </div>
      <div class={styles.addRow}>
        <input
          type="text"
          value={inputValue()}
          placeholder={props.placeholder ?? "Add..."}
          onInput={(e) => setInputValue(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addTag();
            }
          }}
          class={styles.input}
        />
        <button type="button" onClick={addTag} class={styles.addBtn}>
          Add
        </button>
      </div>
    </div>
  );
}
