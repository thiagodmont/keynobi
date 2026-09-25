import { type JSX, createSignal, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import { MenuList } from "@/components/ui/MenuList";
import styles from "./ContextMenu.module.css";

export interface ContextMenuProps {
  /** Viewport point to open at; the menu is moved to stay inside the window. */
  x: number;
  y: number;
  /** Accessible name, such as "Actions for MockProject". */
  label: string;
  onClose: () => void;
  width?: number;
  /** `MenuListItem`s with `onClick`. */
  children: JSX.Element;
}

const EDGE = 8;
const ITEM = '[role="menuitem"]';

/**
 * Actions for one thing, opened by right-click or from the keyboard. Focus
 * moves to the first item; Up/Down, Home, and End move between items; Escape,
 * Tab, or a click outside closes it and returns focus to where it was.
 * Render it only while open.
 */
export function ContextMenu(props: ContextMenuProps): JSX.Element {
  let menu!: HTMLDivElement;
  const returnTo = document.activeElement as HTMLElement | null;
  // eslint-disable-next-line solid/reactivity
  const [position, setPosition] = createSignal({ x: props.x, y: props.y });

  const items = () => Array.from(menu.querySelectorAll<HTMLElement>(ITEM));

  function closeOnOutsidePointer(e: MouseEvent): void {
    if (!menu.contains(e.target as globalThis.Node)) props.onClose();
  }

  onMount(() => {
    const rect = menu.getBoundingClientRect();
    setPosition({
      x: Math.max(EDGE, Math.min(props.x, window.innerWidth - rect.width - EDGE)),
      y: Math.max(EDGE, Math.min(props.y, window.innerHeight - rect.height - EDGE)),
    });
    items()[0]?.focus();
    document.addEventListener("mousedown", closeOnOutsidePointer, true);
  });

  onCleanup(() => {
    document.removeEventListener("mousedown", closeOnOutsidePointer, true);
    const active = document.activeElement;
    const focusWasInMenu = !active || active === document.body || menu.contains(active);
    if (focusWasInMenu && returnTo?.isConnected) returnTo.focus();
  });

  function handleKeyDown(e: KeyboardEvent): void {
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      props.onClose();
      return;
    }
    const all = items();
    if (all.length === 0) return;
    const index = all.indexOf(document.activeElement as HTMLElement);
    let next: HTMLElement | undefined;
    if (e.key === "ArrowDown") next = all[(index + 1) % all.length];
    else if (e.key === "ArrowUp") next = all[(index - 1 + all.length) % all.length];
    else if (e.key === "Home") next = all[0];
    else if (e.key === "End") next = all[all.length - 1];
    if (!next) return;
    e.preventDefault();
    next.focus();
  }

  return (
    <Portal>
      <div ref={menu} class={styles.root} on:keydown={handleKeyDown}>
        <MenuList
          role="menu"
          surface="floating"
          aria-label={props.label}
          style={{
            left: `${position().x}px`,
            top: `${position().y}px`,
            width: props.width ? `${props.width}px` : undefined,
          }}
          class={styles.menu}
        >
          {props.children}
        </MenuList>
      </div>
    </Portal>
  );
}
