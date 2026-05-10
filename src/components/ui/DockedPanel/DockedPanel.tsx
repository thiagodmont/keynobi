import { type JSX, Show } from "solid-js";
import styles from "./DockedPanel.module.css";

export interface DockedPanelProps {
  title: JSX.Element;
  subtitle?: JSX.Element;
  titleTone?: "default" | "info";
  actions?: JSX.Element;
  maxHeight?: string;
  bodyClass?: string;
  bodyStyle?: JSX.CSSProperties;
  class?: string;
  children: JSX.Element;
}

export function DockedPanel(props: DockedPanelProps): JSX.Element {
  return (
    <section
      class={[styles.root, props.class].filter(Boolean).join(" ")}
      style={{ "max-height": props.maxHeight }}
    >
      <header class={styles.header}>
        <div class={styles.titleGroup}>
          <span
            class={[styles.title, props.titleTone === "info" ? styles.titleInfo : ""]
              .filter(Boolean)
              .join(" ")}
          >
            {props.title}
          </span>
          <Show when={props.subtitle}>
            <span class={styles.subtitle}>{props.subtitle}</span>
          </Show>
        </div>
        <Show when={props.actions}>
          <div class={styles.actions}>{props.actions}</div>
        </Show>
      </header>
      <div class={[styles.body, props.bodyClass].filter(Boolean).join(" ")} style={props.bodyStyle}>
        {props.children}
      </div>
    </section>
  );
}
