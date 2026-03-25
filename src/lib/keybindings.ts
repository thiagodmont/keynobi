interface Keybinding {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  shiftKey?: boolean;
  altKey?: boolean;
  action: () => void;
  description: string;
  context?: string;
}

const registry: Keybinding[] = [];

export function registerKeybinding(binding: Keybinding) {
  registry.push(binding);
}

export function unregisterKeybinding(description: string) {
  const idx = registry.findIndex((b) => b.description === description);
  if (idx !== -1) registry.splice(idx, 1);
}

function matchesBinding(e: KeyboardEvent, binding: Keybinding): boolean {
  const key = binding.key.toLowerCase();
  const eventKey = e.key.toLowerCase();

  if (eventKey !== key) return false;
  if (!!binding.metaKey !== e.metaKey) return false;
  if (!!binding.ctrlKey !== e.ctrlKey) return false;
  if (!!binding.shiftKey !== e.shiftKey) return false;
  if (!!binding.altKey !== e.altKey) return false;
  return true;
}

export function initKeybindings() {
  document.addEventListener("keydown", (e: KeyboardEvent) => {
    for (const binding of registry) {
      if (matchesBinding(e, binding)) {
        // Don't intercept when typing in inputs (except registered editor shortcuts)
        const target = e.target as HTMLElement;
        const isInput =
          target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.contentEditable === "true";

        if (isInput && binding.context !== "global") continue;

        e.preventDefault();
        e.stopPropagation();
        binding.action();
        return;
      }
    }
  });
}
