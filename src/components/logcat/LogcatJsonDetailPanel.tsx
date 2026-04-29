import { createSignal, type JSX } from "solid-js";
import type { LogcatEntry } from "@/lib/tauri-api";

export function JsonDetailPanel(props: { entry: LogcatEntry; onClose: () => void }): JSX.Element {
  const [copied, setCopied] = createSignal(false);

  const formattedJson = () => {
    try {
      const raw = props.entry.jsonBody;
      if (!raw) return null;
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return props.entry.jsonBody;
    }
  };

  async function copyJson() {
    const json = formattedJson();
    if (!json) return;
    try {
      await navigator.clipboard.writeText(json);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* Clipboard copy is best-effort; the button state remains unchanged. */
    }
  }

  return (
    <div
      style={{
        "flex-shrink": "0",
        "max-height": "220px",
        display: "flex",
        "flex-direction": "column",
        background: "var(--bg-secondary)",
        "border-top": "1px solid var(--border)",
      }}
    >
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "6px",
          padding: "3px 10px",
          background: "var(--bg-tertiary)",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
        }}
      >
        <span style={{ "font-size": "10px", color: "var(--info)", "font-weight": "600" }}>
          JSON
        </span>
        <span
          style={{
            "font-size": "10px",
            color: "var(--text-muted)",
            flex: "1",
            overflow: "hidden",
            "text-overflow": "ellipsis",
            "white-space": "nowrap",
          }}
        >
          {props.entry.tag}: {props.entry.timestamp}
        </span>
        <button
          onClick={copyJson}
          title="Copy JSON"
          style={{
            background: "none",
            border: "1px solid var(--border)",
            color: copied() ? "var(--success)" : "var(--text-muted)",
            "border-radius": "3px",
            cursor: "pointer",
            "font-size": "10px",
            padding: "1px 6px",
          }}
        >
          {copied() ? "Copied!" : "Copy"}
        </button>
        <button
          onClick={() => props.onClose()}
          title="Close JSON viewer"
          style={{
            background: "none",
            border: "none",
            color: "var(--text-muted)",
            cursor: "pointer",
            "font-size": "12px",
            padding: "0 4px",
          }}
        >
          ✕
        </button>
      </div>

      <pre
        style={{
          flex: "1",
          overflow: "auto",
          margin: "0",
          padding: "8px 12px",
          "font-family": "var(--font-mono)",
          "font-size": "11px",
          "line-height": "1.5",
          color: "var(--text-primary)",
          "white-space": "pre",
          background: "transparent",
        }}
      >
        {formattedJson() ?? "(invalid JSON)"}
      </pre>
    </div>
  );
}
