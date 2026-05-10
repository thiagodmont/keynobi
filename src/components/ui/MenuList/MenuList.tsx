import { type JSX, Show } from "solid-js";
import styles from "./MenuList.module.css";

export interface MenuListProps {
  children: JSX.Element;
  class?: string;
  style?: JSX.CSSProperties;
  role?: JSX.HTMLAttributes<HTMLDivElement>["role"];
  listRef?: (el: HTMLDivElement) => void;
}

export interface MenuListItemProps {
  children: JSX.Element;
  onClick?: () => void;
  active?: boolean;
  destructive?: boolean;
  mono?: boolean;
  gap?: "xs" | "sm" | "md";
  class?: string;
  title?: string;
  role?: JSX.HTMLAttributes<HTMLDivElement>["role"];
  style?: JSX.CSSProperties;
  onMouseDown?: (e: MouseEvent) => void;
  onMouseEnter?: (e: MouseEvent) => void;
}

export interface MenuSectionHeaderProps {
  label: string;
  end?: JSX.Element;
}

export function MenuList(props: MenuListProps): JSX.Element {
  return (
    <div
      ref={props.listRef}
      role={props.role}
      class={[styles.root, props.class].filter(Boolean).join(" ")}
      style={props.style}
    >
      {props.children}
    </div>
  );
}

export function MenuListItem(props: MenuListItemProps): JSX.Element {
  return (
    <div
      title={props.title}
      role={props.role}
      tabIndex={props.onClick ? 0 : undefined}
      style={props.style}
      onMouseDown={(e) => props.onMouseDown?.(e)}
      onMouseEnter={(e) => props.onMouseEnter?.(e)}
      onClick={() => props.onClick?.()}
      onKeyDown={(e) => {
        if (!props.onClick) return;
        if (e.key !== "Enter" && e.key !== " ") return;
        e.preventDefault();
        props.onClick();
      }}
      class={[
        styles.item,
        props.active ? styles.active : "",
        props.destructive ? styles.destructive : "",
        props.mono ? styles.mono : "",
        props.gap === "xs" ? styles.gapXs : "",
        props.gap === "md" ? styles.gapMd : "",
        props.class,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {props.children}
    </div>
  );
}

export function MenuSectionHeader(props: MenuSectionHeaderProps): JSX.Element {
  return (
    <div class={styles.sectionHeader}>
      <span>{props.label}</span>
      <Show when={props.end}>
        <span class={styles.sectionHeaderEnd}>{props.end}</span>
      </Show>
    </div>
  );
}

export function MenuEmptyState(props: { children: JSX.Element }): JSX.Element {
  return <div class={styles.empty}>{props.children}</div>;
}
