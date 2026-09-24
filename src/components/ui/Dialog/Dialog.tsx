import { createSignal, For, onCleanup, onMount, type JSX } from "solid-js";
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

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function DialogBox(props: { dialog: PendingDialog }): JSX.Element {
  let box!: HTMLDivElement;
  const previouslyFocused = document.activeElement as HTMLElement | null;

  const focusable = () => Array.from(box.querySelectorAll<HTMLElement>(FOCUSABLE));

  onMount(() => {
    (focusable()[0] ?? box).focus();
  });

  onCleanup(() => {
    if (previouslyFocused?.isConnected) previouslyFocused.focus();
  });

  // Native listener so Escape never bubbles to document-level shortcut handlers.
  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      resolve("cancel");
      return;
    }
    if (e.key !== "Tab") return;
    const items = focusable();
    if (items.length === 0) {
      e.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || !items.includes(active as HTMLElement))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !items.includes(active as HTMLElement))) {
      e.preventDefault();
      first.focus();
    }
  }

  return (
    <div data-testid="dialog-backdrop" class={styles.backdrop} onClick={() => resolve("cancel")}>
      <div
        ref={box}
        class={styles.box}
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
        tabIndex={-1}
        on:keydown={handleKeyDown}
        onClick={(e) => e.stopPropagation()}
      >
        <div id="dialog-title" class={styles.title}>
          {props.dialog.title}
        </div>
        <div class={styles.message}>{props.dialog.message}</div>
        <div class={styles.actions}>
          <For each={props.dialog.buttons.slice().reverse()}>
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
  );
}

export function DialogHost(): JSX.Element {
  return (
    <>
      {() => {
        const dialog = activeDialog();
        if (!dialog) return null;
        return (
          <Portal>
            <DialogBox dialog={dialog} />
          </Portal>
        );
      }}
    </>
  );
}
