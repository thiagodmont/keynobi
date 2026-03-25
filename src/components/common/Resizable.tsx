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

  function onMouseDown(e: MouseEvent) {
    e.preventDefault();
    setIsDragging(true);
    lastPos = props.direction === "horizontal" ? e.clientX : e.clientY;

    function onMouseMove(e: MouseEvent) {
      const pos = props.direction === "horizontal" ? e.clientX : e.clientY;
      const delta = pos - lastPos;
      lastPos = pos;
      if (delta !== 0) props.onResize(delta);
    }

    function onMouseUp() {
      setIsDragging(false);
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    }

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }

  function onDblClick() {
    props.onReset?.();
  }

  const cursor =
    props.direction === "horizontal" ? "col-resize" : "row-resize";

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
        (e.currentTarget as HTMLElement).style.background =
          "var(--border)";
      }}
      onMouseLeave={(e) => {
        if (!isDragging())
          (e.currentTarget as HTMLElement).style.background = "transparent";
      }}
    />
  );
}

export default Resizable;
