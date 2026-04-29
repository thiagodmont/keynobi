export interface QueryBarTokenStyle {
  color: string;
  bg: string;
  border: string;
}

export function getQueryBarTokenStyle(tokenText: string): QueryBarTokenStyle {
  const token = tokenText.startsWith("-") ? tokenText.slice(1) : tokenText;
  if (token.startsWith("level:")) {
    return {
      color: "var(--warning)",
      bg: "color-mix(in srgb, var(--warning) 13%, transparent)",
      border: "color-mix(in srgb, var(--warning) 35%, transparent)",
    };
  }
  if (token.startsWith("tag:") || token.startsWith("tag~:")) {
    return {
      color: "var(--info)",
      bg: "color-mix(in srgb, var(--info) 13%, transparent)",
      border: "color-mix(in srgb, var(--info) 35%, transparent)",
    };
  }
  if (token.startsWith("message:") || token.startsWith("message~:") || token.startsWith("msg:")) {
    return {
      color: "var(--success)",
      bg: "color-mix(in srgb, var(--success) 11%, transparent)",
      border: "color-mix(in srgb, var(--success) 35%, transparent)",
    };
  }
  if (token.startsWith("package:") || token.startsWith("pkg:")) {
    return {
      color: "var(--accent)",
      bg: "color-mix(in srgb, var(--accent) 13%, transparent)",
      border: "color-mix(in srgb, var(--accent) 40%, transparent)",
    };
  }
  if (token.startsWith("age:")) {
    return {
      color: "var(--accent)",
      bg: "color-mix(in srgb, var(--accent) 13%, transparent)",
      border: "color-mix(in srgb, var(--accent) 35%, transparent)",
    };
  }
  if (token.startsWith("is:")) {
    return {
      color: "var(--error)",
      bg: "color-mix(in srgb, var(--error) 13%, transparent)",
      border: "color-mix(in srgb, var(--error) 35%, transparent)",
    };
  }
  return {
    color: "var(--text-secondary)",
    bg: "rgba(255,255,255,0.07)",
    border: "var(--border)",
  };
}

export function queryBarRootStyle(): Record<string, string> {
  return { position: "relative", flex: "1", "min-width": "280px" };
}

export function queryBarContainerStyle(active: boolean): Record<string, string> {
  return {
    display: "flex",
    "flex-wrap": "wrap",
    "align-items": "center",
    gap: "3px",
    "min-height": "26px",
    padding: "3px 6px",
    background: "var(--bg-primary)",
    border: `1px solid ${active ? "var(--accent)" : "var(--border)"}`,
    "border-radius": "4px",
    cursor: "text",
    transition: "border-color 0.1s",
  };
}

export function searchIconStyle(): Record<string, string> {
  return {
    "flex-shrink": "0",
    color: "var(--text-muted)",
    "font-size": "11px",
    "pointer-events": "none",
    opacity: "0.6",
    "line-height": "1",
  };
}

export function queryBarGroupBoxStyle(): Record<string, string> {
  return {
    display: "inline-flex",
    "align-items": "center",
    gap: "3px",
    padding: "3px 6px",
    border: "1px solid rgba(255,255,255,0.10)",
    "border-radius": "8px",
    background: "rgba(255,255,255,0.04)",
    "flex-shrink": "0",
    "max-width": "100%",
  };
}

export function queryBarOrBadgeStyle(): Record<string, string> {
  return {
    "font-size": "9px",
    "font-weight": "700",
    "letter-spacing": "0.05em",
    color: "var(--accent)",
    background: "rgba(var(--accent-rgb,59,130,246),0.13)",
    border: "1px solid rgba(var(--accent-rgb,59,130,246),0.35)",
    "border-radius": "10px",
    padding: "1px 5px",
    "flex-shrink": "0",
    "user-select": "none",
  };
}

export function queryBarAndBadgeStyle(): Record<string, string> {
  return {
    "font-size": "9px",
    "font-weight": "600",
    "letter-spacing": "0.04em",
    color: "var(--text-muted)",
    border: "1px dashed var(--border)",
    "border-radius": "10px",
    padding: "1px 5px",
    "flex-shrink": "0",
    "user-select": "none",
  };
}

export function queryBarPillStyle(
  tokenStyle: QueryBarTokenStyle,
  negated: boolean
): Record<string, string> {
  return {
    display: "inline-flex",
    "align-items": "center",
    gap: "2px",
    "font-size": "10px",
    "font-family": "var(--font-mono)",
    color: tokenStyle.color,
    background: tokenStyle.bg,
    border: `1px solid ${tokenStyle.border}`,
    "border-radius": "10px",
    padding: "1px 4px 1px 6px",
    "flex-shrink": "0",
    "white-space": "nowrap",
    opacity: negated ? "0.65" : "1",
  };
}

export function queryBarPillLabelStyle(): Record<string, string> {
  return { cursor: "text", "user-select": "none" };
}

export function queryBarPillRemoveButtonStyle(color: string): Record<string, string> {
  return {
    background: "none",
    border: "none",
    color,
    cursor: "pointer",
    padding: "0 1px",
    "font-size": "9px",
    "line-height": "1",
    opacity: "0.55",
    display: "flex",
    "align-items": "center",
  };
}

export function queryBarInlineEditStyle(
  overrides: Record<string, string> = {}
): Record<string, string> {
  return {
    flex: "1",
    "min-width": "120px",
    "max-width": "420px",
    background: "var(--bg-primary)",
    border: "1px solid var(--accent)",
    color: "var(--text-primary)",
    "font-size": "11px",
    "font-family": "var(--font-mono)",
    "border-radius": "10px",
    padding: "1px 6px",
    outline: "none",
    ...overrides,
  };
}

export function queryBarOrphanInlineEditStyle(): Record<string, string> {
  return { display: "inline-flex", "align-items": "center", gap: "3px" };
}

export function queryBarInputRowStyle(): Record<string, string> {
  return {
    flex: "1",
    display: "flex",
    "align-items": "center",
    gap: "3px",
    "min-width": "180px",
  };
}

export function queryBarDraftInputStyle(): Record<string, string> {
  return {
    flex: "1",
    "min-width": "0",
    background: "transparent",
    border: "none",
    color: "var(--text-primary)",
    "font-size": "11px",
    "font-family": "var(--font-mono)",
    outline: "none",
    padding: "1px 0",
  };
}

export function queryBarConnectorGroupStyle(): Record<string, string> {
  return { display: "flex", gap: "3px", "flex-shrink": "0" };
}

export function queryBarConnectorButtonStyle(): Record<string, string> {
  return {
    background: "none",
    border: "1px solid var(--border)",
    color: "var(--text-muted)",
    "border-radius": "4px",
    cursor: "pointer",
    "font-size": "10px",
    "font-family": "var(--font-mono)",
    padding: "2px 6px",
    "white-space": "nowrap",
    transition: "color 0.1s, border-color 0.1s",
  };
}

export function queryBarClearButtonStyle(): Record<string, string> {
  return {
    "flex-shrink": "0",
    background: "none",
    border: "none",
    color: "var(--text-muted)",
    cursor: "pointer",
    "font-size": "11px",
    padding: "0 2px",
    "line-height": "1",
    display: "flex",
    "align-items": "center",
    opacity: "0.6",
  };
}

export function suggestionMenuStyle(): Record<string, string> {
  return {
    position: "absolute",
    top: "calc(100% + 3px)",
    left: "0",
    "min-width": "220px",
    "max-width": "360px",
    "max-height": "260px",
    "overflow-y": "auto",
    background: "var(--bg-secondary)",
    border: "1px solid var(--border)",
    "border-radius": "4px",
    "box-shadow": "0 6px 20px rgba(0,0,0,0.45)",
    "z-index": "600",
  };
}

export function suggestionItemStyle(selected: boolean): Record<string, string> {
  return {
    padding: "5px 10px",
    "font-size": "11px",
    "font-family": "var(--font-mono)",
    color: selected ? "#fff" : "var(--text-primary)",
    background: selected ? "var(--accent)" : "transparent",
    cursor: "pointer",
    "white-space": "nowrap",
    overflow: "hidden",
    "text-overflow": "ellipsis",
  };
}

export function suggestionFooterStyle(): Record<string, string> {
  return {
    padding: "4px 10px",
    "font-size": "10px",
    color: "var(--text-disabled, #4b5563)",
    "border-top": "1px solid var(--border)",
    "user-select": "none",
  };
}
