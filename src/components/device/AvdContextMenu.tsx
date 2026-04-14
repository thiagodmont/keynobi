import { type JSX } from "solid-js";

export interface AvdContextMenuProps {
  onClose: () => void;
  onWipe: () => void;
  onDelete: () => void;
}

export function AvdContextMenu(props: AvdContextMenuProps): JSX.Element {
  return (
    <>
      {/* Click-away backdrop */}
      <div
        style={{ position: "fixed", inset: "0", "z-index": "1999" }}
        onClick={() => props.onClose()}
      />
      <div
        style={{
          position: "absolute",
          right: "0",
          bottom: "calc(100% + 4px)",
          "z-index": "2000",
          background: "var(--bg-tertiary)",
          border: "1px solid var(--border)",
          "border-radius": "6px",
          "box-shadow": "0 4px 16px rgba(0,0,0,0.4)",
          "min-width": "160px",
          padding: "4px",
          "white-space": "nowrap",
        }}
      >
        <ContextMenuItem label="Wipe Data…" onClick={props.onWipe} destructive={false} />
        <div style={{ height: "1px", background: "var(--border)", margin: "4px 0" }} />
        <ContextMenuItem label="Delete…" onClick={props.onDelete} destructive={true} />
      </div>
    </>
  );
}

function ContextMenuItem(props: {
  label: string;
  onClick: () => void;
  destructive: boolean;
}): JSX.Element {
  return (
    <button
      onClick={() => props.onClick()}
      style={{
        display: "block",
        width: "100%",
        padding: "6px 10px",
        background: "none",
        border: "none",
        cursor: "pointer",
        "text-align": "left",
        "font-size": "12px",
        "border-radius": "4px",
        color: props.destructive ? "var(--error)" : "var(--text-secondary)",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = props.destructive
          ? "rgba(0,0,0,0.15)"
          : "var(--bg-hover)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLElement).style.background = "none";
      }}
    >
      {props.label}
    </button>
  );
}
