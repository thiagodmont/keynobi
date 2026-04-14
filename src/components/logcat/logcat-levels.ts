export const LEVEL_CONFIG = {
  verbose: { label: "V", color: "var(--text-muted)", bg: "transparent" },
  debug:   { label: "D", color: "var(--info)", bg: "transparent" },
  info:    { label: "I", color: "var(--success)", bg: "transparent" },
  warn:    {
    label: "W",
    color: "var(--warning)",
    bg: "color-mix(in srgb, var(--warning) 8%, transparent)",
  },
  error:   {
    label: "E",
    color: "var(--error)",
    bg: "color-mix(in srgb, var(--error) 10%, transparent)",
  },
  fatal:   {
    label: "F",
    color: "var(--error)",
    bg: "color-mix(in srgb, var(--error) 14%, transparent)",
  },
  unknown: { label: "?", color: "var(--text-muted)", bg: "transparent" },
} as const;

export function getLevelConfig(level: string) {
  return LEVEL_CONFIG[level.toLowerCase() as keyof typeof LEVEL_CONFIG] ?? LEVEL_CONFIG.unknown;
}
