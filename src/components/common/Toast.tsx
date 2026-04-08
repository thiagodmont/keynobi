import { For } from "solid-js";
import { toasts, dismissToast, showToast, type Toast } from "../../stores/ui.store";

export { showToast };

export function ToastContainer() {
  return (
    <div class="toast-container" aria-live="polite" aria-atomic="false">
      <For each={toasts()}>
        {(toast) => <ToastItem toast={toast} />}
      </For>
    </div>
  );
}

function ToastItem(props: { toast: Toast }) {
  return (
    <div
      class={`toast toast--${props.toast.kind}`}
      role={props.toast.kind === "error" ? "alert" : "status"}
    >
      <span class="toast__message">{props.toast.message}</span>
      <button
        class="toast__close"
        aria-label="Dismiss"
        onClick={() => dismissToast(props.toast.id)}
      >
        ×
      </button>
    </div>
  );
}
