import { type JSX, For } from "solid-js";
import { Badge, Input, MenuList, MenuListItem } from "@/components/ui";
import type { QueryBarSuggestion } from "@/lib/logcat-query";
import {
  getQueryBarTokenStyle,
  queryBarConnectorButtonStyle,
  queryBarInlineEditStyle,
  queryBarPillLabelStyle,
  queryBarPillRemoveButtonStyle,
  queryBarPillStyle,
  queryBarPillToggleButtonStyle,
  suggestionFooterStyle,
  suggestionItemStyle,
  suggestionMenuStyle,
} from "./querybar-styles";

export function QueryBarOrBadge(props: { onToggle?: () => void } = {}): JSX.Element {
  return (
    <>
      {props.onToggle ? (
        <Badge
          size="xs"
          variant="accent"
          ariaLabel="Change OR to AND"
          title="Change OR to AND"
          onMouseDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          onClick={(e) => {
            e.stopPropagation();
            props.onToggle?.();
          }}
        >
          OR
        </Badge>
      ) : (
        <Badge size="xs" variant="accent">
          OR
        </Badge>
      )}
    </>
  );
}

export function QueryBarAndBadge(props: { onToggle?: () => void } = {}): JSX.Element {
  return (
    <>
      {props.onToggle ? (
        <Badge
          size="xs"
          ariaLabel="Change AND to OR"
          title="Change AND to OR"
          onMouseDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          onClick={(e) => {
            e.stopPropagation();
            props.onToggle?.();
          }}
        >
          AND
        </Badge>
      ) : (
        <Badge size="xs">AND</Badge>
      )}
    </>
  );
}

export function QueryBarInlineEditInput(props: {
  value: string;
  inputRef: (el: HTMLInputElement) => void;
  onInput: (value: string) => void;
  onKeyDown: (e: KeyboardEvent) => void;
  onBlur: () => void;
  style?: Record<string, string>;
}): JSX.Element {
  return (
    <Input
      inputRef={props.inputRef}
      type="text"
      spellcheck={false}
      value={props.value}
      size="xs"
      mono
      onInput={props.onInput}
      onKeyDown={(e) => props.onKeyDown(e)}
      onBlur={() => props.onBlur()}
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      placeholder="Edit filter..."
      style={queryBarInlineEditStyle(props.style)}
    />
  );
}

export function QueryBarPill(props: {
  token: string;
  disabled?: boolean;
  onEdit: () => void;
  onRemove: () => void;
  onToggleDisabled?: () => void;
}): JSX.Element {
  const style = () => getQueryBarTokenStyle(props.token);
  const disabled = () => props.disabled === true;

  return (
    <span
      onClick={(e) => e.stopPropagation()}
      style={queryBarPillStyle(style(), props.token.startsWith("-"), disabled())}
    >
      {props.onToggleDisabled ? (
        <button
          onMouseDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          onClick={(e) => {
            e.stopPropagation();
            props.onToggleDisabled?.();
          }}
          aria-label={`${disabled() ? "Re-enable" : "Disable"} filter ${props.token}`}
          title={disabled() ? "Re-enable filter" : "Disable filter temporarily"}
          style={queryBarPillToggleButtonStyle(style().color, disabled())}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLElement).style.opacity = "1";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLElement).style.opacity = disabled() ? "0.9" : "0.45";
          }}
        >
          ⏻
        </button>
      ) : null}
      <span
        onMouseDown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          props.onEdit();
        }}
        title="Edit filter"
        style={queryBarPillLabelStyle(disabled())}
      >
        {props.token}
      </span>
      <button
        onMouseDown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          props.onRemove();
        }}
        title="Remove filter"
        style={queryBarPillRemoveButtonStyle(style().color)}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLElement).style.opacity = "1";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLElement).style.opacity = "0.55";
        }}
      >
        ✕
      </button>
    </span>
  );
}

export function QueryBarConnectorButton(props: {
  title: string;
  hoverColor: string;
  onMouseDown: () => void;
  children: JSX.Element;
}): JSX.Element {
  return (
    <button
      onMouseDown={(e) => {
        e.preventDefault();
        props.onMouseDown();
      }}
      title={props.title}
      style={queryBarConnectorButtonStyle()}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.color = props.hoverColor;
        (e.currentTarget as HTMLElement).style.borderColor = props.hoverColor;
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.color = "var(--text-muted)";
        (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
      }}
    >
      {props.children}
    </button>
  );
}

export function QueryBarSuggestions(props: {
  suggestions: readonly QueryBarSuggestion[];
  selectedIdx: number;
  onSelect: (insert: string) => void;
  onHover: (idx: number) => void;
}): JSX.Element {
  return (
    <MenuList style={suggestionMenuStyle()}>
      <For each={props.suggestions}>
        {(suggestion, i) => {
          const isSelected = () => i() === props.selectedIdx;
          return (
            <MenuListItem
              onMouseDown={(e) => {
                e.preventDefault();
                props.onSelect(suggestion.insert);
              }}
              onMouseEnter={() => props.onHover(i())}
              active={isSelected()}
              mono
              style={suggestionItemStyle(isSelected())}
            >
              {suggestion.display}
            </MenuListItem>
          );
        }}
      </For>
      <div style={suggestionFooterStyle()}>
        ↑↓ navigate · Tab/Enter select · Esc close · && = AND · | = OR group
      </div>
    </MenuList>
  );
}
