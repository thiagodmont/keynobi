import { type Accessor, type JSX, For, createMemo, createSignal } from "solid-js";
import styles from "./Listbox.module.css";

export interface ListboxContextMenuRequest<T> {
  item: T;
  /** Viewport point to open a menu at: the pointer, or the option's corner from the keyboard. */
  x: number;
  y: number;
}

export interface ListboxProps<T> {
  items: readonly T[];
  /** Accessible name of the list. */
  label: string;
  getKey: (item: T) => string;
  isSelected: (item: T) => boolean;
  /** Focusable but not selectable, such as an offline device. */
  isDisabled?: (item: T) => boolean;
  /** Accessible name of an option whose content has no readable text, such as an icon rail. */
  getOptionLabel?: (item: T) => string | undefined;
  getOptionTitle?: (item: T) => string | undefined;
  onSelect: (item: T) => void;
  /** Right-click, the context-menu key, or Shift+F10 on an option. */
  onContextMenu?: (request: ListboxContextMenuRequest<T>) => void;
  /** The option's content. `item` follows the latest value for its key. */
  children: (item: Accessor<T>) => JSX.Element;
  class?: string;
  optionClass?: string;
  testId?: string;
}

const OPTION = '[role="option"]';

/**
 * Single-select list with one tab stop (roving tabindex). Up/Down, Home, and
 * End move focus without selecting; Enter or Space selects; the context-menu
 * key or Shift+F10 asks for the option's actions.
 */
export function Listbox<T>(props: ListboxProps<T>): JSX.Element {
  let root!: HTMLDivElement;
  const [focusedKey, setFocusedKey] = createSignal<string | null>(null);

  const keys = createMemo(() => props.items.map((item) => props.getKey(item)));
  const byKey = createMemo(() => new Map(props.items.map((item) => [props.getKey(item), item])));

  const tabStop = createMemo(() => {
    const focused = focusedKey();
    if (focused !== null && byKey().has(focused)) return focused;
    const selected = props.items.find((item) => props.isSelected(item));
    return selected !== undefined ? props.getKey(selected) : (keys()[0] ?? null);
  });

  function options(): HTMLElement[] {
    return Array.from(root.querySelectorAll<HTMLElement>(OPTION));
  }

  function moveFocus(from: HTMLElement, key: string): void {
    const all = options();
    const index = all.indexOf(from);
    const target =
      key === "Home"
        ? all[0]
        : key === "End"
          ? all[all.length - 1]
          : all[Math.min(all.length - 1, Math.max(0, index + (key === "ArrowDown" ? 1 : -1)))];
    target?.focus();
  }

  function requestMenu(item: T, x: number, y: number): void {
    props.onContextMenu?.({ item, x, y });
  }

  function handleKeyDown(e: KeyboardEvent, item: T): void {
    const option = e.currentTarget as HTMLElement;
    // Keys typed into a control inside an option (an inline rename) are its own.
    if (e.target !== option) return;
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp":
      case "Home":
      case "End":
        e.preventDefault();
        moveFocus(option, e.key);
        return;
      case "Enter":
      case " ":
        e.preventDefault();
        if (!props.isDisabled?.(item)) props.onSelect(item);
        return;
    }
    if (props.onContextMenu && (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey))) {
      e.preventDefault();
      const rect = option.getBoundingClientRect();
      requestMenu(item, rect.left + 8, rect.bottom);
    }
  }

  return (
    <div
      role="listbox"
      aria-label={props.label}
      ref={root}
      class={[styles.root, props.class].filter(Boolean).join(" ")}
      data-testid={props.testId}
      onFocusOut={(e) => {
        const next = e.relatedTarget as globalThis.Node | null;
        if (!next || !root.contains(next)) setFocusedKey(null);
      }}
    >
      {/* Keyed by string so a refreshed list keeps its rows, and the focused row keeps focus. */}
      <For each={keys()}>
        {(key) => {
          // Keeps the last value while the row is being removed with its key.
          let latest: T | undefined;
          const item = createMemo(() => {
            latest = byKey().get(key) ?? latest;
            return latest as T;
          });
          const disabled = () => props.isDisabled?.(item()) ?? false;
          return (
            <div
              role="option"
              aria-selected={props.isSelected(item()) ? "true" : "false"}
              aria-disabled={disabled() ? "true" : undefined}
              aria-label={props.getOptionLabel?.(item())}
              title={props.getOptionTitle?.(item())}
              tabIndex={tabStop() === key ? 0 : -1}
              data-key={key}
              class={[styles.option, props.optionClass].filter(Boolean).join(" ")}
              onFocus={() => setFocusedKey(key)}
              onKeyDown={(e) => handleKeyDown(e, item())}
              onClick={() => {
                if (!disabled()) props.onSelect(item());
              }}
              onContextMenu={(e) => {
                if (!props.onContextMenu) return;
                e.preventDefault();
                requestMenu(item(), e.clientX, e.clientY);
              }}
            >
              {props.children(item)}
            </div>
          );
        }}
      </For>
    </div>
  );
}
