export function logcatDropdownRootStyle(): Record<string, string> {
  return { position: "relative", "flex-shrink": "0" };
}

export function logcatDropdownPanelStyle(options: {
  align: "left" | "right";
  minWidth: string;
  maxWidth: string;
}): Record<string, string> {
  return {
    position: "absolute",
    top: "calc(100% + 4px)",
    [options.align]: "0",
    "min-width": options.minWidth,
    "max-width": options.maxWidth,
    background: "var(--bg-secondary)",
    border: "1px solid var(--border)",
    "border-radius": "4px",
    "box-shadow": "0 6px 20px rgba(0,0,0,0.45)",
    "z-index": "700",
    "font-size": "11px",
  };
}

export function logcatDropdownOverlayStyle(): Record<string, string> {
  return { position: "fixed", inset: "0", "z-index": "699" };
}

export function logcatDropdownSeparatorStyle(margin = "4px 0"): Record<string, string> {
  return { height: "1px", background: "var(--border)", margin };
}

export function logcatDropdownSectionHeaderStyle(): Record<string, string> {
  return {
    padding: "2px 10px 4px",
    "font-size": "10px",
    color: "var(--text-muted)",
    "text-transform": "uppercase",
    "letter-spacing": "0.05em",
  };
}
