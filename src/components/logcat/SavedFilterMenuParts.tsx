import { type JSX } from "solid-js";
import { Button, Input, MenuEmptyState, MenuListItem, MenuSectionHeader } from "@/components/ui";

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

export function SavedFilterQueryPreview(props: { query: string }): JSX.Element {
  return (
    <span
      style={{
        color: "var(--text-muted)",
        "font-size": "10px",
        "max-width": "130px",
        overflow: "hidden",
        "text-overflow": "ellipsis",
        "white-space": "nowrap",
      }}
    >
      {props.query}
    </span>
  );
}

export function SavedFilterName(props: {
  name: string;
  query: string;
  onApply: () => void;
}): JSX.Element {
  return (
    <span
      onClick={() => props.onApply()}
      style={{
        flex: "1",
        overflow: "hidden",
        "text-overflow": "ellipsis",
        "white-space": "nowrap",
      }}
      title={props.query}
    >
      {props.name}
    </span>
  );
}

export function SavedFilterActionButton(props: {
  title?: string;
  children: JSX.Element;
  onClick: () => void;
}): JSX.Element {
  return (
    <button
      onClick={(e) => {
        e.stopPropagation();
        props.onClick();
      }}
      style={{
        background: "none",
        border: "none",
        color: "var(--text-muted)",
        cursor: "pointer",
        padding: "0 3px",
        "font-size": "10px",
      }}
      title={props.title}
    >
      {props.children}
    </button>
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
        style={{
          flex: "1",
          "border-color": "var(--accent)",
        }}
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
        ✓
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
        ✕
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
