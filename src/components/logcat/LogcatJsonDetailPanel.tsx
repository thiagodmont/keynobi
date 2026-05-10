import { createSignal, type JSX } from "solid-js";
import { Button, DockedPanel } from "@/components/ui";
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
    <DockedPanel
      title="JSON"
      titleTone="info"
      subtitle={`${props.entry.tag}: ${props.entry.timestamp}`}
      maxHeight="220px"
      actions={
        <>
          <Button
            variant="outline"
            size="xs"
            tone={copied() ? "success" : "muted"}
            onClick={copyJson}
            title="Copy JSON"
          >
            {copied() ? "Copied!" : "Copy"}
          </Button>
          <Button
            variant="ghost"
            size="xs"
            tone="muted"
            onClick={() => props.onClose()}
            title="Close JSON viewer"
          >
            ✕
          </Button>
        </>
      }
    >
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
    </DockedPanel>
  );
}
