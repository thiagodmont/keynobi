import { createSignal, type JSX } from "solid-js";

interface ResizableProps {
  direction: "horizontal" | "vertical";
  onResize: (delta: number) => void;
  onReset?: () => void;
  class?: string;
}

export function Resizable(props: ResizableProps): JSX.Element {
  const [isDragging, setIsDragging] = createSignal(false);

  let lastPos = 0;
  let pendingDelta = 0;
  let rafId: number | null = null;

  function onMouseDown(e: MouseEvent) {
    e.preventDefault();
    setIsDragging(true);
    lastPos = props.direction === "horizontal" ? e.clientX : e.clientY;
    pendingDelta = 0;

    function onMouseMove(e: MouseEvent) {
      const pos = props.direction === "horizontal" ? e.clientX : e.clientY;
      const delta = pos - lastPos;
      lastPos = pos;
      pendingDelta += delta;

      // Fire at most once per animation frame to prevent layout thrashing
      // on high-frequency displays (120 Hz). Deltas are accumulated so
      // no movement is lost between frames.
      if (rafId === null) {
        rafId = requestAnimationFrame(() => {
          if (pendingDelta !== 0) props.onResize(pendingDelta);
          pendingDelta = 0;
          rafId = null;
        });
      }
    }

    function onMouseUp() {
      setIsDragging(false);
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
        // Flush any remaining accumulated delta before stopping.
        if (pendingDelta !== 0) props.onResize(pendingDelta);
        pendingDelta = 0;
        rafId = null;
      }
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    }

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }

  function onDblClick() {
    props.onReset?.();
  }

  const cursor = props.direction === "horizontal" ? "col-resize" : "row-resize";

  return (
    <div
      class={props.class}
      onMouseDown={onMouseDown}
      onDblClick={onDblClick}
      style={{
        cursor,
        "flex-shrink": "0",
        background: isDragging() ? "var(--accent)" : "transparent",
        transition: "background 0.1s",
        width: props.direction === "horizontal" ? "4px" : "100%",
        height: props.direction === "vertical" ? "4px" : "100%",
        "z-index": "10",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLElement).style.background = "var(--border)";
      }}
      onMouseLeave={(e) => {
        if (!isDragging()) (e.currentTarget as HTMLElement).style.background = "transparent";
      }}
    />
  );
}

export default Resizable;
