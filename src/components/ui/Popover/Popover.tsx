import { createEffect, onCleanup, type JSX } from "solid-js";
import styles from "./Popover.module.css";

export interface PopoverApi {
  open: boolean;
  close: () => void;
  toggle: () => void;
}

export interface PopoverProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  trigger: (api: PopoverApi) => JSX.Element;
  children: JSX.Element;
  align?: "left" | "right";
  minWidth?: string;
  maxWidth?: string;
  class?: string;
  panelClass?: string;
  panelStyle?: JSX.CSSProperties;
  closeOnWindowBlur?: boolean;
  closeOnVisibilityHidden?: boolean;
}

export function Popover(props: PopoverProps): JSX.Element {
  const close = () => props.onOpenChange(false);
  const toggle = () => props.onOpenChange(!props.open);

  function handleWindowBlur(): void {
    if (props.open && props.closeOnWindowBlur !== false) close();
  }

  function handleVisibilityChange(): void {
    if (
      props.open &&
      props.closeOnVisibilityHidden !== false &&
      document.visibilityState === "hidden"
    ) {
      close();
    }
  }

  createEffect(() => {
    if (!props.open) return;
    window.addEventListener("blur", handleWindowBlur);
    document.addEventListener("visibilitychange", handleVisibilityChange);
    onCleanup(() => {
      window.removeEventListener("blur", handleWindowBlur);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    });
  });

  return (
    <div class={[styles.root, props.class].filter(Boolean).join(" ")}>
      {props.trigger({ open: props.open, close, toggle })}
      {props.open ? (
        <>
          <div
            class={[
              styles.panel,
              props.align === "right" ? styles.alignRight : styles.alignLeft,
              props.panelClass,
            ]
              .filter(Boolean)
              .join(" ")}
            style={{
              "min-width": props.minWidth,
              "max-width": props.maxWidth,
              ...props.panelStyle,
            }}
          >
            {props.children}
          </div>
          <div class={styles.overlay} onClick={close} />
        </>
      ) : null}
    </div>
  );
}
