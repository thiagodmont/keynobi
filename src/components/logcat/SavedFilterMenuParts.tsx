import { type JSX } from "solid-js";
import {
  Button,
  Icon,
  Input,
  MenuEmptyState,
  MenuListItem,
  MenuSectionHeader,
} from "@/components/ui";
import styles from "./SavedFilterMenuParts.module.css";

export function SavedFilterMenuRow(props: {
  children: JSX.Element;
  onClick?: () => void;
  mono?: boolean;
  gap?: string;
}): JSX.Element {
  return (
    <MenuListItem
      onClick={() => props.onClick?.()}
      mono={props.mono}
      gap={props.gap === "4px" ? "xs" : undefined}
    >
      {props.children}
    </MenuListItem>
  );
}

export function SavedFilterItemLayout(props: { children: JSX.Element }): JSX.Element {
  return <div class={styles.itemLayout}>{props.children}</div>;
}

export function SavedFilterApplyItem(props: {
  name: string;
  query: string;
  onApply: () => void;
}): JSX.Element {
  return (
    <MenuListItem onClick={props.onApply} class={styles.applyItem}>
      <SavedFilterName name={props.name} query={props.query} />
    </MenuListItem>
  );
}

export function SavedFilterQueryPreview(props: { query: string }): JSX.Element {
  return <span class={styles.queryPreview}>{props.query}</span>;
}

export function SavedFilterName(props: { name: string; query: string }): JSX.Element {
  return (
    <span class={styles.name} title={props.query}>
      {props.name}
    </span>
  );
}

export function SavedFilterActionButton(props: {
  title?: string;
  icon: "pencil" | "trash";
  onClick: () => void;
}): JSX.Element {
  return (
    <Button
      variant="ghost"
      size="xs"
      tone="muted"
      title={props.title}
      class={styles.actionButton}
      onClick={(e) => {
        e.stopPropagation();
        props.onClick();
      }}
    >
      <Icon name={props.icon} size={11} />
    </Button>
  );
}

export function SavedFilterRenameInput(props: {
  value: string;
  onInput: (value: string) => void;
  onCommit: () => void;
  onCancel: () => void;
}): JSX.Element {
  return (
    <>
      <Input
        type="text"
        value={props.value}
        size="sm"
        onInput={props.onInput}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.stopPropagation();
            props.onCommit();
          }
          if (e.key === "Escape") {
            e.stopPropagation();
            props.onCancel();
          }
        }}
        autofocus
        class={styles.renameInput}
      />
      <Button
        variant="outline"
        size="xs"
        tone="accent"
        onClick={(e) => {
          e.stopPropagation();
          props.onCommit();
        }}
      >
        <Icon name="check" size={12} />
      </Button>
      <Button
        variant="outline"
        size="xs"
        tone="muted"
        onClick={(e) => {
          e.stopPropagation();
          props.onCancel();
        }}
      >
        <Icon name="close" size={12} />
      </Button>
    </>
  );
}

export function SavedFilterEmptyState(): JSX.Element {
  return <MenuEmptyState>No saved filters yet</MenuEmptyState>;
}

export function SavedFilterCountHeader(props: { count: number; max: number }): JSX.Element {
  return <MenuSectionHeader label="Saved" end={`${props.count} / ${props.max}`} />;
}
