import { createSignal, type JSX } from "solid-js";
import styles from "./Resizable.module.css";

export interface ResizableProps {
  direction: "horizontal" | "vertical";
  onResize: (delta: number) => void;
  onReset?: () => void;
  class?: string;
}

export function Resizable(props: ResizableProps): JSX.Element {
  const [dragging, setDragging] = createSignal(false);
  let lastPos = 0;
  let rafId: number | undefined;
  let pendingDelta = 0;

  function onMouseDown(e: MouseEvent) {
    e.preventDefault();
    lastPos = props.direction === "horizontal" ? e.clientX : e.clientY;
    setDragging(true);
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }

  function onMouseMove(e: MouseEvent) {
    const pos = props.direction === "horizontal" ? e.clientX : e.clientY;
    pendingDelta += pos - lastPos;
    lastPos = pos;
    if (rafId !== undefined) return;
    rafId = requestAnimationFrame(() => {
      rafId = undefined;
      if (pendingDelta !== 0) {
        props.onResize(pendingDelta);
        pendingDelta = 0;
      }
    });
  }

  function onMouseUp() {
    if (pendingDelta !== 0) {
      props.onResize(pendingDelta);
      pendingDelta = 0;
    }
    setDragging(false);
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
  }

  function onDblClick() {
    props.onReset?.();
  }

  return (
    <div
      class={[
        styles.handle,
        styles[props.direction],
        dragging() ? styles.dragging : "",
        props.class ?? "",
      ].join(" ")}
      onMouseDown={onMouseDown}
      onDblClick={onDblClick}
    />
  );
}
