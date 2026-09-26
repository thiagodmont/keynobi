/**
 * Options for exporting a debug session: which redaction rules apply and
 * whether the crash log lines go in. The backend shows the save dialog and
 * writes the file.
 */
import { type JSX, For, Show, createSignal } from "solid-js";
import { Portal } from "solid-js/web";
import type { RedactionRule, SessionExportOptions, SessionExportResult } from "@/bindings";
import { exportDebugSession, formatError } from "@/lib/tauri-api";
import { Alert, Button, Checkbox, Spinner, modalFocus } from "@/components/ui";
import { REDACTION_RULE_LABELS } from "./session-format";
import styles from "./SessionExportDialog.module.css";

const RULES: RedactionRule[] = ["emails", "secrets", "ipAddresses", "paths", "deviceSerials"];

export function SessionExportDialog(props: {
  sessionId: string;
  onClose: () => void;
  onExported: (result: SessionExportResult) => void;
}): JSX.Element {
  const [options, setOptions] = createSignal<SessionExportOptions>({
    redaction: {
      emails: true,
      secrets: true,
      ipAddresses: true,
      paths: true,
      deviceSerials: true,
    },
    includeCrashLogs: true,
  });
  const [saving, setSaving] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  function setRule(rule: RedactionRule, on: boolean): void {
    setOptions((o) => ({ ...o, redaction: { ...o.redaction, [rule]: on } }));
  }

  function close(): void {
    if (!saving()) props.onClose();
  }

  async function save(): Promise<void> {
    if (saving()) return;
    setSaving(true);
    setError(null);
    try {
      const result = await exportDebugSession(props.sessionId, options());
      // A cancelled save dialog leaves the options open.
      if (result) props.onExported(result);
    } catch (e) {
      setError(formatError(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Portal>
      <div class={styles.backdrop} onClick={close}>
        <div
          ref={(el) => modalFocus(el, { onEscape: close })}
          class={styles.box}
          role="dialog"
          aria-modal="true"
          aria-labelledby="session-export-title"
          aria-describedby="session-export-description"
          tabIndex={-1}
          onClick={(e) => e.stopPropagation()}
        >
          <h2 id="session-export-title" class={styles.title}>
            Export Debug Session
          </h2>
          <p id="session-export-description" class={styles.description}>
            Saves a .zip with the session, its timeline, and what you select below. Redaction is
            best effort: check the file before you share it. R8 mappings are never included.
          </p>
          <Checkbox
            checked={options().includeCrashLogs}
            onChange={(on) => setOptions((o) => ({ ...o, includeCrashLogs: on }))}
          >
            Log lines kept with crashes and ANRs
          </Checkbox>
          <fieldset class={styles.rules}>
            <legend class={styles.legend}>Redact</legend>
            <For each={RULES}>
              {(rule) => (
                <Checkbox checked={options().redaction[rule]} onChange={(on) => setRule(rule, on)}>
                  {REDACTION_RULE_LABELS[rule]}
                </Checkbox>
              )}
            </For>
          </fieldset>
          <Show when={error()}>
            {(message) => (
              <Alert variant="error" title="Could not export the session">
                {message()}
              </Alert>
            )}
          </Show>
          <div class={styles.status} role="status">
            <Show when={saving()}>
              <Spinner size="sm" />
              Waiting for the save dialog…
            </Show>
          </div>
          <div class={styles.actions}>
            <Button variant="secondary" size="sm" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={() => void save()}>
              Save…
            </Button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
