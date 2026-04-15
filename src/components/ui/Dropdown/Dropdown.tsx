import { createSignal, createEffect, onCleanup, For, Show, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import styles from "./Dropdown.module.css";

export type MenuItem =
  | { separator: true; label?: never; onClick?: never; disabled?: never; destructive?: never }
  | { separator?: false; label: string; onClick: () => void; disabled?: boolean; destructive?: boolean };

export interface DropdownProps {
  trigger: JSX.Element;
  items: MenuItem[];
  placement?: "bottom" | "top" | "right";
  class?: string;
}

export function Dropdown(props: DropdownProps): JSX.Element {
  const [isOpen, setIsOpen] = createSignal(false);
  const [activeIndex, setActiveIndex] = createSignal(-1);
  let triggerRef!: HTMLDivElement;
  let menuRef!: HTMLDivElement;
  let menuPos = { x: 0, y: 0 };

  function open() {
    const rect = triggerRef?.getBoundingClientRect();
    if (rect) {
      if (props.placement === "right") {
        menuPos = { x: rect.right + 4, y: rect.top };
      } else if (props.placement === "top") {
        menuPos = { x: rect.left, y: rect.top - 4 };
      } else {
        menuPos = { x: rect.left, y: rect.bottom + 4 };
      }
    }
    setActiveIndex(-1);
    setIsOpen(true);
  }

  function close() {
    setIsOpen(false);
    setActiveIndex(-1);
  }

  function toggle() {
    if (isOpen()) close();
    else open();
  }

  function activatableItems(): MenuItem[] {
    return props.items.filter((it) => !it.disabled && !it.separator);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!isOpen()) return;
    const items = activatableItems();
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (items.length === 0) return;
      setActiveIndex((i) => (i + 1) % items.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length === 0) return;
      setActiveIndex((i) => (i - 1 + items.length) % items.length);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const idx = activeIndex();
      if (idx >= 0 && idx < items.length) {
        items[idx].onClick?.();
        close();
      }
    }
  }

  function handleOutsideMouseDown(e: MouseEvent) {
    if (!isOpen()) return;
    const target = e.target as globalThis.Node;
    const insideTrigger = triggerRef?.contains(target) ?? false;
    const insideMenu = menuRef?.contains(target) ?? false;
    if (!insideTrigger && !insideMenu) {
      close();
    }
  }

  createEffect(() => {
    if (isOpen()) {
      document.addEventListener("keydown", handleKeyDown);
      document.addEventListener("mousedown", handleOutsideMouseDown);
    } else {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleOutsideMouseDown);
    }
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleKeyDown);
    document.removeEventListener("mousedown", handleOutsideMouseDown);
  });

  // Track activatable index during render
  let activatableIdx = -1;

  return (
    <div ref={triggerRef} class={[styles.wrapper, props.class].filter(Boolean).join(" ")}>
      <div onClick={toggle} style={{ display: "contents" }}>
        {props.trigger}
      </div>
      <Show when={isOpen()}>
        <Portal>
          <div
            ref={menuRef}
            class={styles.menu}
            style={{
              left: `${menuPos.x}px`,
              top: `${menuPos.y}px`,
              transform: props.placement === "top" ? "translateY(-100%)" : undefined,
            }}
          >
            {(() => {
              activatableIdx = -1;
              return (
                <For each={props.items}>
                  {(item) => {
                    if (item.separator) {
                      return <div class={styles.separator} />;
                    }
                    if (!item.disabled) activatableIdx++;
                    const myIdx = item.disabled ? -1 : activatableIdx;
                    return (
                      <button
                        type="button"
                        class={[
                          styles.item,
                          item.disabled ? styles.itemDisabled : "",
                          item.destructive ? styles.itemDestructive : "",
                          !item.disabled &&
                          !item.destructive &&
                          myIdx === activeIndex()
                            ? styles.focused
                            : "",
                        ].filter(Boolean).join(" ")}
                        onClick={() => {
                          if (!item.disabled) {
                            item.onClick();
                            close();
                          }
                        }}
                      >
                        {item.label}
                      </button>
                    );
                  }}
                </For>
              );
            })()}
          </div>
        </Portal>
      </Show>
    </div>
  );
}
