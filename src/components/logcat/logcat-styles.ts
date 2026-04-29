export function btnStyle(color: string): Record<string, string> {
  return {
    display: "flex",
    "align-items": "center",
    gap: "4px",
    padding: "3px 8px",
    background: "none",
    border: "1px solid var(--border)",
    color,
    "border-radius": "4px",
    cursor: "pointer",
    "font-size": "11px",
    "white-space": "nowrap",
  };
}
