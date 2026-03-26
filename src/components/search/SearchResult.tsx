import { type JSX, For, Show } from "solid-js";
import type { SearchResult } from "@/lib/tauri-api";
import Icon from "@/components/common/Icon";

interface SearchResultItemProps {
  result: SearchResult;
  collapsed: boolean;
  onToggle: () => void;
  relativePath: string;
}

export function SearchResultItem(props: SearchResultItemProps): JSX.Element {
  return (
    <div>
      {/* File header */}
      <div
        onClick={props.onToggle}
        style={{
          display: "flex",
          "align-items": "center",
          gap: "4px",
          padding: "2px 8px",
          cursor: "pointer",
          "user-select": "none",
          "font-size": "12px",
          color: "var(--text-primary)",
          "background": "var(--bg-tertiary)",
          "border-bottom": "1px solid var(--border)",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.background = "var(--bg-hover)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = "var(--bg-tertiary)";
        }}
      >
        <Icon
          name={props.collapsed ? "chevron-right" : "chevron-down"}
          size={12}
        />
        <Icon name="file" size={14} color="var(--text-muted)" />
        <span style={{ flex: "1", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
          {props.relativePath}
        </span>
        <span
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
            "background": "var(--bg-secondary)",
            padding: "0 4px",
            "border-radius": "8px",
            "flex-shrink": "0",
          }}
        >
          {props.result.matches.length}
        </span>
      </div>

      {/* Match lines */}
      <Show when={!props.collapsed}>
        <For each={props.result.matches}>
          {(match) => (
            <MatchLine
              path={props.result.path}
              line={match.line}
              col={match.col}
              endCol={match.endCol}
              content={match.lineContent}
            />
          )}
        </For>
      </Show>
    </div>
  );
}

function MatchLine(props: {
  path: string;
  line: number;
  col: number;
  endCol: number;
  content: string;
}): JSX.Element {
  async function handleClick() {
    const { openFileAtLocation } = await import("@/services/project.service");
    openFileAtLocation(props.path, props.line, props.col);
  }

  const before = () => props.content.slice(0, props.col);
  const matched = () => props.content.slice(props.col, props.endCol);
  const after = () => props.content.slice(props.endCol);

  return (
    <div
      role="button"
      tabindex="0"
      onClick={handleClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          handleClick();
        }
      }}
      style={{
        display: "flex",
        gap: "6px",
        padding: "1px 8px 1px 24px",
        cursor: "pointer",
        "font-family": "var(--font-mono)",
        "font-size": "12px",
        "line-height": "18px",
        color: "var(--text-secondary)",
        "white-space": "nowrap",
        overflow: "hidden",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "var(--bg-hover)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
      }}
    >
      <span style={{ color: "var(--text-muted)", "min-width": "28px", "text-align": "right", "flex-shrink": "0" }}>
        {props.line}
      </span>
      <span style={{ overflow: "hidden", "text-overflow": "ellipsis" }}>
        <span>{before()}</span>
        <span
          style={{
            background: "rgba(234, 179, 8, 0.3)",
            color: "var(--text-primary)",
            "border-radius": "2px",
          }}
        >
          {matched()}
        </span>
        <span>{after()}</span>
      </span>
    </div>
  );
}

export default SearchResultItem;
