import { type JSX, For, Show } from "solid-js";
import styles from "./Tabs.module.css";

export interface TabDef {
  id: string;
  label: string;
  icon?: string;
  badge?: string | number;
}

export interface TabsProps {
  tabs: TabDef[];
  activeTab: string;
  onChange: (id: string) => void;
  class?: string;
}

export function Tabs(props: TabsProps): JSX.Element {
  return (
    <div role="tablist" class={[styles.root, props.class].filter(Boolean).join(" ")}>
      <For each={props.tabs}>
        {(tab) => (
          <button
            role="tab"
            aria-selected={tab.id === props.activeTab}
            class={[styles.tab, tab.id === props.activeTab ? styles.active : ""].filter(Boolean).join(" ")}
            onClick={() => {
              if (tab.id !== props.activeTab) props.onChange(tab.id);
            }}
          >
            {tab.label}
            <Show when={tab.badge !== undefined}>
              <span class={styles.badge}>{tab.badge}</span>
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}
