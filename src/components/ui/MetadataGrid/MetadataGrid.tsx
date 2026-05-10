import { type JSX, Show } from "solid-js";
import styles from "./MetadataGrid.module.css";

export interface MetadataGridProps {
  columns?: number;
  children: JSX.Element;
}

export interface MetadataCellProps {
  label: string;
  value: JSX.Element;
  title?: string;
  valueStyle?: JSX.CSSProperties;
  onClick?: (target: HTMLElement) => void;
}

export function MetadataGrid(props: MetadataGridProps): JSX.Element {
  return (
    <div
      class={styles.grid}
      style={{ "grid-template-columns": `repeat(${props.columns ?? 3}, minmax(0, 1fr))` }}
    >
      {props.children}
    </div>
  );
}

export function MetadataCell(props: MetadataCellProps): JSX.Element {
  return (
    <div class={styles.cell}>
      <div class={styles.label}>{props.label}</div>
      <Show
        when={props.onClick}
        fallback={
          <div class={styles.value} style={props.valueStyle} title={props.title}>
            {props.value}
          </div>
        }
      >
        <button
          type="button"
          class={styles.valueButton}
          style={props.valueStyle}
          title={props.title ?? `Filter by ${props.label}`}
          onClick={(e) => {
            e.stopPropagation();
            props.onClick?.(e.currentTarget);
          }}
        >
          {props.value}
        </button>
      </Show>
    </div>
  );
}
