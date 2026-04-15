import { type JSX } from "solid-js";
import styles from "./ScrollArea.module.css";

export interface ScrollAreaProps {
  overflow?: "auto" | "scroll" | "hidden";
  horizontal?: boolean;
  class?: string;
  children: JSX.Element;
}

export function ScrollArea(props: ScrollAreaProps): JSX.Element {
  const ov = () => props.overflow ?? "auto";

  return (
    <div
      class={[styles.root, props.class].filter(Boolean).join(" ")}
      style={{
        "overflow-y": ov(),
        "overflow-x": props.horizontal ? "auto" : "hidden",
      }}
    >
      {props.children}
    </div>
  );
}
