import { createSignal, For, type JSX } from "solid-js";

export type ToastType = "success" | "error" | "warning" | "info";

interface ToastItem {
  id: number;
  message: string;
  type: ToastType;
}

const [toasts, setToasts] = createSignal<ToastItem[]>([]);
let nextId = 0;

export function showToast(message: string, type: ToastType = "info") {
  const id = nextId++;
  setToasts((prev) => [...prev, { id, message, type }]);
  setTimeout(() => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, 4000);
}

const typeColors: Record<ToastType, string> = {
  success: "#4caf50",
  error: "#f14c4c",
  warning: "#cca700",
  info: "#007acc",
};

export function ToastContainer(): JSX.Element {
  return (
    <div
      style={{
        position: "fixed",
        bottom: "32px",
        right: "16px",
        display: "flex",
        "flex-direction": "column",
        gap: "8px",
        "z-index": "9999",
        "pointer-events": "none",
      }}
    >
      <For each={toasts()}>
        {(toast) => (
          <div
            style={{
              background: "var(--bg-tertiary)",
              border: `1px solid ${typeColors[toast.type]}`,
              "border-left": `3px solid ${typeColors[toast.type]}`,
              "border-radius": "4px",
              padding: "8px 12px",
              "font-size": "12px",
              color: "var(--text-primary)",
              "max-width": "320px",
              "word-break": "break-word",
              "pointer-events": "all",
              "box-shadow": "0 2px 8px rgba(0,0,0,0.4)",
            }}
          >
            {toast.message}
          </div>
        )}
      </For>
    </div>
  );
}
