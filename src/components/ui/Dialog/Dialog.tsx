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

const [pendingDialog, setPendingDialog] = createSignal<PendingDialog | null>(null);

export function showDialog(dialog: Omit<PendingDialog, "resolve">): Promise<string> {
  return new Promise((resolve) => {
    setPendingDialog({ ...dialog, resolve });
  });
}

function resolve(value: string) {
  const dialog = pendingDialog();
  if (!dialog) return;
  setPendingDialog(null);
  dialog.resolve(value);
}

export function DialogHost(): JSX.Element {
  return (
    <Show when={pendingDialog()}>
      {(dialog) => (
        <Portal>
          <div
            data-testid="dialog-backdrop"
            class={styles.backdrop}
            onClick={() => resolve("cancel")}
          >
            <div class={styles.box} onClick={(e) => e.stopPropagation()}>
              <div class={styles.title}>{dialog().title}</div>
              <div class={styles.message}>{dialog().message}</div>
              <div class={styles.actions}>
                <For each={dialog().buttons.slice().reverse()}>
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
