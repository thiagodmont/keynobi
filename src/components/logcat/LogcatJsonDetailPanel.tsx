import { createSignal, type JSX } from "solid-js";
import { Button, DockedPanel, Icon } from "@/components/ui";
import type { LogcatEntry } from "@/lib/tauri-api";
import styles from "./LogcatJsonDetailPanel.module.css";

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
            <Icon name={copied() ? "check" : "copy"} size={12} />
            {copied() ? "Copied!" : "Copy"}
          </Button>
          <Button
            variant="ghost"
            size="xs"
            tone="muted"
            onClick={() => props.onClose()}
            title="Close JSON viewer"
          >
            <Icon name="close" size={12} />
          </Button>
        </>
      }
    >
      <pre class={styles.jsonValue}>{formattedJson() ?? "(invalid JSON)"}</pre>
    </DockedPanel>
  );
}
