import { createSignal, createUniqueId, onCleanup, Show, type JSX } from "solid-js";
import styles from "./Tooltip.module.css";

export interface TooltipProps {
  content: string | JSX.Element;
  position?: "top" | "bottom" | "left" | "right";
  delay?: number;
  disabled?: boolean;
  class?: string;
  children: JSX.Element;
}

export function Tooltip(props: TooltipProps): JSX.Element {
  const [visible, setVisible] = createSignal(false);
  const tooltipId = createUniqueId();
  let timer: ReturnType<typeof setTimeout> | undefined;

  function show() {
    if (props.disabled) return;
    timer = setTimeout(() => setVisible(true), props.delay ?? 400);
  }

  function hide() {
    clearTimeout(timer);
    setVisible(false);
  }

  onCleanup(() => clearTimeout(timer));

  return (
    <div
      class={[styles.wrapper, props.class].filter(Boolean).join(" ")}
      aria-describedby={visible() ? tooltipId : undefined}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
    >
      {props.children}
      <Show when={visible()}>
        <div
          id={tooltipId}
          class={[styles.tooltip, styles[props.position ?? "top"]].join(" ")}
          role="tooltip"
        >
          {props.content}
        </div>
      </Show>
    </div>
  );
}
