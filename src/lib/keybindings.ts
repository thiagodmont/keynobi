/**
 * Global keyboard shortcut registry.
 *
 * Usage:
 *   registerKeybinding({ key: "s", metaKey: true, action: save, description: "Save" });
 *
 * Shortcuts are matched on every `keydown` event. Matches in input/textarea/
 * contentEditable elements are skipped unless `context: "global"` is set, and
 * none run while a modal dialog or a menu is open.
 */

export interface Keybinding {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  action: () => void;
  description: string;
  /**
   * Set to "global" to fire even when an input element is focused.
   * Leave unset (default) to skip inputs.
   */
  context?: "global";
}

const registry: Keybinding[] = [];

/**
 * Compute a stable string key for a binding based on its key combo.
 * This is the deduplication key — two bindings with the same combo overwrite
 * each other regardless of their description.
 */
function comboKey(binding: Keybinding): string {
  return [
    binding.key.toLowerCase(),
    binding.metaKey ? "meta" : "",
    binding.ctrlKey ? "ctrl" : "",
    binding.shiftKey ? "shift" : "",
    binding.altKey ? "alt" : "",
  ]
    .filter(Boolean)
    .join("+");
}

export function registerKeybinding(binding: Keybinding): void {
  const key = comboKey(binding);
  const existing = registry.findIndex((b) => comboKey(b) === key);
  if (existing !== -1) {
    registry[existing] = binding;
  } else {
    registry.push(binding);
  }
}

function matchesBinding(e: KeyboardEvent, binding: Keybinding): boolean {
  if (e.key.toLowerCase() !== binding.key.toLowerCase()) return false;
  if (!!binding.metaKey !== e.metaKey) return false;
  if (!!binding.ctrlKey !== e.ctrlKey) return false;
  if (!!binding.shiftKey !== e.shiftKey) return false;
  if (!!binding.altKey !== e.altKey) return false;
  return true;
}

/** Open modal dialogs and menus mark themselves with these. */
const OVERLAY_SELECTOR = '[aria-modal="true"], [role="menu"]';

/** A modal dialog or a menu is open, so shortcuts for the window behind it must not run. */
export function isOverlayOpen(): boolean {
  return document.querySelector(OVERLAY_SELECTOR) !== null;
}

export function initKeybindings(): void {
  document.addEventListener("keydown", (e: KeyboardEvent) => {
    for (const binding of registry) {
      if (!matchesBinding(e, binding)) continue;

      if (isOverlayOpen()) {
        // Still keep the webview from acting on an app shortcut (Cmd+R reloads).
        e.preventDefault();
        return;
      }

      const target = e.target as HTMLElement;
      const isInputContext =
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.contentEditable === "true";

      if (isInputContext && binding.context !== "global") continue;

      e.preventDefault();
      e.stopPropagation();
      binding.action();
      return;
    }
  });
}

/** Returns a snapshot of all registered bindings (useful for a keybindings UI). */
export function listKeybindings(): ReadonlyArray<Keybinding> {
  return registry;
}
