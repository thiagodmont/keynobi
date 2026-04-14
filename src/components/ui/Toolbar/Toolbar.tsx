import { type JSX, For, Show } from "solid-js";
import { Separator } from "@/components/ui/Separator";
import styles from "./Toolbar.module.css";

export interface ToolbarItemDef {
  id: string;
  label: string;
  icon?: string;
  onClick: () => void;
  disabled?: boolean;
  active?: boolean;
  separator?: boolean;
}

export interface ToolbarProps {
  items: ToolbarItemDef[];
  compact?: boolean;
  class?: string;
}

export function Toolbar(props: ToolbarProps): JSX.Element {
  return (
    <div
      role="toolbar"
      class={[
        styles.root,
        props.compact ? styles.compact : "",
        props.class,
      ].filter(Boolean).join(" ")}
    >
      <For each={props.items}>
        {(item) => (
          <>
            <Show when={item.separator}>
              <Separator orientation="vertical" spacing="sm" />
            </Show>
            <button
              type="button"
              disabled={item.disabled}
              aria-pressed={item.active ? "true" : undefined}
              class={[
                styles.item,
                item.active ? styles.active : "",
              ].filter(Boolean).join(" ")}
              onClick={item.onClick}
            >
              {item.label}
            </button>
          </>
        )}
      </For>
    </div>
  );
}
