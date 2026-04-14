import {
  type JSX,
  Show,
  For,
  createSignal,
  createEffect,
  createMemo,
} from "solid-js";
import { Portal } from "solid-js/web";
import { searchActions } from "@/lib/action-registry";
import { Icon } from "@/components/ui/Icon";
import styles from "./CommandPalette.module.css";

export type PaletteMode = "commands";

const [paletteState, setPaletteState] = createSignal<{ open: boolean; mode: PaletteMode }>({
  open: false,
  mode: "commands",
});

export function openPalette(_mode: string = "commands"): void {
  setPaletteState({ open: true, mode: "commands" });
}

export function closePalette(): void {
  setPaletteState({ open: false, mode: "commands" });
}

export function isPaletteOpen(): boolean {
  return paletteState().open;
}

interface PaletteItem {
  id: string;
  label: string;
  detail?: string;
  icon?: string;
  action: () => void;
}

export function CommandPalette(): JSX.Element {
  let inputRef!: HTMLInputElement;
  const [query, setQuery] = createSignal("");
  const [selectedIndex, setSelectedIndex] = createSignal(0);

  createEffect(() => {
    if (paletteState().open) {
      setQuery("");
      setSelectedIndex(0);
      setTimeout(() => inputRef?.focus(), 10);
    }
  });

  const results = createMemo<PaletteItem[]>(() => {
    const q = query().startsWith(">") ? query().slice(1).trim() : query();
    return searchActions(q).map((a) => ({
      id: a.id,
      label: a.label,
      detail: a.shortcut ?? (a.category ? a.category : ""),
      icon: a.icon,
      action: () => {
        closePalette();
        a.action();
      },
    }));
  });

  function handleKeyDown(e: KeyboardEvent): void {
    const items = results();
    if (e.key === "Escape") { e.preventDefault(); closePalette(); return; }
    if (e.key === "ArrowDown") { e.preventDefault(); setSelectedIndex((i) => Math.min(i + 1, items.length - 1)); return; }
    if (e.key === "ArrowUp") { e.preventDefault(); setSelectedIndex((i) => Math.max(i - 1, 0)); return; }
    if (e.key === "Enter") { e.preventDefault(); const item = items[selectedIndex()]; if (item) item.action(); return; }
  }

  return (
    <Show when={paletteState().open}>
      <Portal>
        <div data-testid="palette-backdrop" class={styles.backdrop} onClick={() => closePalette()}>
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Command Palette"
            class={styles.palette}
            onClick={(e) => e.stopPropagation()}
          >
            <div class={styles.inputWrapper}>
              <input
                ref={inputRef}
                type="text"
                placeholder="Type a command..."
                aria-label="Command Palette"
                role="combobox"
                aria-expanded="true"
                aria-autocomplete="list"
                value={query()}
                onInput={(e) => { setQuery(e.currentTarget.value); setSelectedIndex(0); }}
                onKeyDown={handleKeyDown}
                class={styles.input}
              />
            </div>
            <div role="listbox" class={styles.list}>
              <For each={results()}>
                {(item, index) => (
                  <div
                    role="option"
                    aria-selected={index() === selectedIndex()}
                    onClick={() => item.action()}
                    onMouseEnter={() => setSelectedIndex(index())}
                    class={[styles.item, index() === selectedIndex() ? styles.active : ""].filter(Boolean).join(" ")}
                  >
                    <Show when={item.icon}><Icon name={item.icon!} size={14} /></Show>
                    <span class={styles.label}>{item.label}</span>
                    <Show when={item.detail}><span class={styles.detail}>{item.detail}</span></Show>
                  </div>
                )}
              </For>
              <Show when={results().length === 0}>
                <div class={styles.empty}>
                  {query().trim() ? "No commands found" : "Type to search commands"}
                </div>
              </Show>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
