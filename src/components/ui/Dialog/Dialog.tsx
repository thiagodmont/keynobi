import { createSignal, For, Show, type JSX } from "solid-js";
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

export function showDialog(dialog: Omit<PendingDialog, "resolve">): Promise<string> {
  return new Promise((resolve) => {
    setDialogQueue((q) => [...q, { ...dialog, resolve }]);
  });
}

function resolve(value: string) {
  const q = dialogQueue();
  if (q.length === 0) return;
  const [first, ...rest] = q;
  first.resolve(value);
  setDialogQueue(rest);
}

/** Resolves any queued dialogs and clears the queue. For unit tests only. */
export function resetDialogHostForTests(): void {
  for (const d of dialogQueue()) {
    d.resolve("cancel");
  }
  setDialogQueue([]);
}

export function DialogHost(): JSX.Element {
  return (
    <Show when={dialogQueue()[0]} keyed>
      {(dialog) => (
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
      )}
    </Show>
  );
}
