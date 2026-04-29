import { type JSX, For } from "solid-js";
import type { QueryBarSuggestion } from "@/lib/logcat-query";
import {
  getQueryBarTokenStyle,
  queryBarAndBadgeStyle,
  queryBarConnectorButtonStyle,
  queryBarInlineEditStyle,
  queryBarOrBadgeStyle,
  queryBarPillLabelStyle,
  queryBarPillRemoveButtonStyle,
  queryBarPillStyle,
  suggestionFooterStyle,
  suggestionItemStyle,
  suggestionMenuStyle,
} from "./querybar-styles";

export function QueryBarOrBadge(): JSX.Element {
  return <span style={queryBarOrBadgeStyle()}>OR</span>;
}

export function QueryBarAndBadge(): JSX.Element {
  return <span style={queryBarAndBadgeStyle()}>AND</span>;
}

export function QueryBarInlineEditInput(props: {
  value: string;
  inputRef: (el: HTMLInputElement) => void;
  onInput: (e: InputEvent) => void;
  onKeyDown: (e: KeyboardEvent) => void;
  onBlur: () => void;
  style?: Record<string, string>;
}): JSX.Element {
  return (
    <input
      ref={props.inputRef}
      type="text"
      spellcheck={false}
      value={props.value}
      onInput={(e) => props.onInput(e)}
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
  onEdit: () => void;
  onRemove: () => void;
}): JSX.Element {
  const style = () => getQueryBarTokenStyle(props.token);

  return (
    <span
      onClick={(e) => e.stopPropagation()}
      style={queryBarPillStyle(style(), props.token.startsWith("-"))}
    >
      <span
        onMouseDown={(e) => {
          e.preventDefault();
          e.stopPropagation();
          props.onEdit();
        }}
        title="Edit filter"
        style={queryBarPillLabelStyle()}
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
    <div style={suggestionMenuStyle()}>
      <For each={props.suggestions}>
        {(suggestion, i) => {
          const isSelected = () => i() === props.selectedIdx;
          return (
            <div
              onMouseDown={(e) => {
                e.preventDefault();
                props.onSelect(suggestion.insert);
              }}
              onMouseEnter={() => props.onHover(i())}
              style={suggestionItemStyle(isSelected())}
            >
              {suggestion.display}
            </div>
          );
        }}
      </For>
      <div style={suggestionFooterStyle()}>
        ↑↓ navigate · Tab/Enter select · Esc close · && = AND · | = OR group
      </div>
    </div>
  );
}
