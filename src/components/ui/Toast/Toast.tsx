import { createSignal, For, type JSX } from "solid-js";
import styles from "./Toast.module.css";

export type ToastKind = "error" | "info" | "success" | "warning";

export interface Toast {
  id: string;
  message: string;
  kind: ToastKind;
}

// Module-level signals — same self-contained pattern as Dialog
const [_toasts, setToasts] = createSignal<Toast[]>([]);
const _timers = new Map<string, ReturnType<typeof setTimeout>>();

export const toasts = _toasts;

export function showToast(message: string, kind: ToastKind = "info"): void {
  const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
  setToasts((prev) => [...prev, { id, message, kind }]);
  if (kind !== "error") {
    const timer = setTimeout(() => {
      _timers.delete(id);
      dismissToast(id);
    }, 4000);
    _timers.set(id, timer);
  }
}

export function dismissToast(id: string): void {
  const timer = _timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    _timers.delete(id);
  }
  setToasts((prev) => prev.filter((t) => t.id !== id));
}

export function ToastContainer(): JSX.Element {
  return (
    <div class={styles.container} aria-live="polite" aria-atomic="false">
      <For each={toasts()}>
        {(toast) => (
          <div
            class={[styles.toast, styles[toast.kind]].join(" ")}
            role={toast.kind === "error" ? "alert" : "status"}
          >
            <span class={styles.message}>{toast.message}</span>
            <button
              class={styles.closeBtn}
              aria-label="Dismiss"
              onClick={() => dismissToast(toast.id)}
            >
              ×
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
