/**
 * Promise-based modal dialog system.
 *
 * Usage:
 *   const result = await showSaveDialog("MainActivity.kt");
 *   // result is "save" | "discard" | "cancel"
 *
 * Mount <DialogHost /> once in App.tsx. Dialogs are queued and shown one
 * at a time; subsequent calls while a dialog is visible will wait.
 */

import { createSignal, For, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

// ── Types ─────────────────────────────────────────────────────────────────────

export type SaveDialogResult = "save" | "discard" | "cancel";

export type CloseDialogResult = "save-all" | "discard-all" | "cancel";

interface PendingDialog {
  title: string;
  message: string;
  buttons: Array<{
    label: string;
    value: string;
    style: "primary" | "danger" | "secondary";
  }>;
  resolve: (value: string) => void;
}

// ── Module-level signal for the pending dialog ────────────────────────────────

const [pendingDialog, setPendingDialog] = createSignal<PendingDialog | null>(null);

/** Internal helper: show a dialog and return the button value the user chose. */
export function showDialog(dialog: Omit<PendingDialog, "resolve">): Promise<string> {
  return new Promise((resolve) => {
    setPendingDialog({ ...dialog, resolve });
  });
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * Show the standard "unsaved changes" dialog for a single file.
 * Returns "save" | "discard" | "cancel".
 */
export function showSaveDialog(filename: string): Promise<SaveDialogResult> {
  return showDialog({
    title: "Unsaved Changes",
    message: `Do you want to save the changes you made to "${filename}"?`,
    buttons: [
      { label: "Save", value: "save", style: "primary" },
      { label: "Don't Save", value: "discard", style: "danger" },
      { label: "Cancel", value: "cancel", style: "secondary" },
    ],
  }) as Promise<SaveDialogResult>;
}

/**
 * Show a "save before close" dialog when multiple dirty files are open.
 * Returns "save-all" | "discard-all" | "cancel".
 */
export function showCloseDialog(dirtyCount: number): Promise<CloseDialogResult> {
  const noun = dirtyCount === 1 ? "file has" : `${dirtyCount} files have`;
  return showDialog({
    title: "Save Changes Before Closing?",
    message: `${noun.charAt(0).toUpperCase()}${noun.slice(1)} unsaved changes. What would you like to do?`,
    buttons: [
      { label: "Save All", value: "save-all", style: "primary" },
      { label: "Discard All", value: "discard-all", style: "danger" },
      { label: "Cancel", value: "cancel", style: "secondary" },
    ],
  }) as Promise<CloseDialogResult>;
}

// ── Renderer ──────────────────────────────────────────────────────────────────

const BUTTON_BASE = {
  padding: "6px 16px",
  "border-radius": "4px",
  "font-size": "13px",
  cursor: "pointer",
  "font-weight": "500",
} as const;

const BUTTON_STYLES = {
  primary: { ...BUTTON_BASE, background: "var(--accent)", color: "#ffffff" },
  danger: {
    ...BUTTON_BASE,
    background: "transparent",
    color: "var(--error)",
    border: "1px solid var(--error)",
  },
  secondary: {
    ...BUTTON_BASE,
    background: "transparent",
    color: "var(--text-secondary)",
    border: "1px solid var(--border)",
  },
} as const;

function resolve(value: string) {
  const dialog = pendingDialog();
  if (!dialog) return;
  setPendingDialog(null);
  dialog.resolve(value);
}

/**
 * Mount this once at the application root (in App.tsx).
 * It renders the active dialog as a portal over all other content.
 */
export function DialogHost(): JSX.Element {
  return (
    <Show when={pendingDialog()}>
      {(dialog) => (
        <Portal>
          {/* Backdrop */}
          <div
            onClick={() => resolve("cancel")}
            style={{
              position: "fixed",
              inset: "0",
              background: "rgba(0,0,0,0.5)",
              "z-index": "9000",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
            }}
          >
            {/* Dialog box — stop click propagation so backdrop doesn't close it */}
            <div
              onClick={(e) => e.stopPropagation()}
              style={{
                background: "var(--bg-tertiary)",
                border: "1px solid var(--border)",
                "border-radius": "8px",
                padding: "24px",
                "min-width": "340px",
                "max-width": "480px",
                "box-shadow": "0 8px 32px rgba(0,0,0,0.6)",
              }}
            >
              <h3
                style={{
                  "font-size": "14px",
                  "font-weight": "600",
                  color: "var(--text-primary)",
                  "margin-bottom": "10px",
                }}
              >
                {dialog().title}
              </h3>
              <p
                style={{
                  "font-size": "13px",
                  color: "var(--text-secondary)",
                  "margin-bottom": "20px",
                  "line-height": "1.5",
                }}
              >
                {dialog().message}
              </p>

              {/* Buttons — primary on the right, cancel on the left */}
              <div
                style={{
                  display: "flex",
                  gap: "8px",
                  "justify-content": "flex-end",
                  "flex-wrap": "wrap",
                }}
              >
                <For each={dialog().buttons.slice().reverse()}>
                  {(btn) => (
                    <button style={BUTTON_STYLES[btn.style]} onClick={() => resolve(btn.value)}>
                      {btn.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>
        </Portal>
      )}
    </Show>
  );
}
