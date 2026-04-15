import { createSignal, For, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import styles from "./Dialog.module.css";

export type DialogButtonStyle = "primary" | "danger" | "secondary";

export interface DialogButton {
  label: string;
  value: string;
  style: DialogButtonStyle;
}

interface PendingDialog {
  title: string;
  message: string;
  buttons: DialogButton[];
  resolve: (value: string) => void;
}

const [dialogQueue, setDialogQueue] = createSignal<PendingDialog[]>([]);
/** Shown dialog only; unchanged when another dialog is queued behind the current one (avoids remounting the open UI). */
const [activeDialog, setActiveDialog] = createSignal<PendingDialog | null>(null);

export function showDialog(dialog: Omit<PendingDialog, "resolve">): Promise<string> {
  return new Promise((resolve) => {
    const entry: PendingDialog = { ...dialog, resolve };
    let wasEmpty = false;
    setDialogQueue((q) => {
      wasEmpty = q.length === 0;
      return [...q, entry];
    });
    if (wasEmpty) {
      setActiveDialog(entry);
    }
  });
}

function resolve(value: string) {
  const q = dialogQueue();
  if (q.length === 0) return;
  const [first, ...rest] = q;
  first.resolve(value);
  setDialogQueue(rest);
  setActiveDialog(rest[0] ?? null);
}

/** Resolves any queued dialogs and clears the queue. For unit tests only. */
export function resetDialogHostForTests(): void {
  for (const d of dialogQueue()) {
    d.resolve("cancel");
  }
  setDialogQueue([]);
  setActiveDialog(null);
}

export function DialogHost(): JSX.Element {
  return (
    <>
      {() => {
        const dialog = activeDialog();
        if (!dialog) return null;
        return (
          <Portal>
            <div
              data-testid="dialog-backdrop"
              class={styles.backdrop}
              onClick={() => resolve("cancel")}
            >
              <div class={styles.box} onClick={(e) => e.stopPropagation()}>
                <div class={styles.title}>{dialog.title}</div>
                <div class={styles.message}>{dialog.message}</div>
                <div class={styles.actions}>
                  <For each={dialog.buttons.slice().reverse()}>
                    {(btn) => (
                      <button
                        class={[styles.btn, styles[btn.style]].join(" ")}
                        onClick={() => resolve(btn.value)}
                      >
                        {btn.label}
                      </button>
                    )}
                  </For>
                </div>
              </div>
            </div>
          </Portal>
        );
      }}
    </>
  );
}
