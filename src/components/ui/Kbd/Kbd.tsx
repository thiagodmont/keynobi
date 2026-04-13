import { type JSX } from "solid-js";
import styles from "./Kbd.module.css";

export interface KbdProps {
  class?: string;
  children: JSX.Element;
}

export function Kbd(props: KbdProps): JSX.Element {
  return (
    <kbd class={[styles.root, props.class].filter(Boolean).join(" ")}>
      {props.children}
    </kbd>
  );
}
