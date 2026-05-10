import { type JSX } from "solid-js";
import styles from "./ControlStrip.module.css";

export interface ControlStripProps {
  children: JSX.Element;
  direction?: "row" | "column";
  align?: "start" | "center";
  wrap?: boolean;
  class?: string;
  style?: JSX.CSSProperties;
  testId?: string;
}

export function ControlStrip(props: ControlStripProps): JSX.Element {
  return (
    <div
      data-testid={props.testId}
      class={[
        styles.root,
        props.direction === "column" ? styles.column : "",
        props.align === "start" ? styles.alignStart : "",
        props.wrap ? styles.wrap : "",
        props.class,
      ]
        .filter(Boolean)
        .join(" ")}
      style={props.style}
    >
      {props.children}
    </div>
  );
}
