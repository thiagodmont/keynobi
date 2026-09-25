import { onCleanup, onMount } from "solid-js";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export interface ModalFocusOptions {
  /** Called on Escape. Omit when the dialog handles Escape itself. */
  onEscape?: () => void;
  /** What to focus on open. Default: the first focusable element, else the dialog. */
  initialFocus?: () => HTMLElement | null | undefined;
}

/**
 * Modal focus for a dialog element: focus moves in on open, Tab and Shift+Tab
 * stay inside, and focus returns to where it was on close. Call it from the
 * dialog's `ref` so it lives exactly as long as the dialog is rendered.
 */
export function modalFocus(dialog: HTMLElement, options: ModalFocusOptions = {}): void {
  const previouslyFocused = document.activeElement as HTMLElement | null;
  const focusable = () => Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));

  // A native listener, so Escape handled here never reaches document-level shortcuts.
  function handleKeyDown(e: KeyboardEvent): void {
    if (e.key === "Escape" && options.onEscape) {
      e.preventDefault();
      e.stopPropagation();
      options.onEscape();
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
    const active = document.activeElement as HTMLElement | null;
    const outside = !active || !items.includes(active);
    if (e.shiftKey && (active === first || outside)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || outside)) {
      e.preventDefault();
      first.focus();
    }
  }

  dialog.addEventListener("keydown", handleKeyDown);

  onMount(() => {
    (options.initialFocus?.() ?? focusable()[0] ?? dialog).focus();
  });

  onCleanup(() => {
    dialog.removeEventListener("keydown", handleKeyDown);
    if (previouslyFocused?.isConnected) previouslyFocused.focus();
  });
}
