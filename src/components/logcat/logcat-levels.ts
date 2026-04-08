export const LEVEL_CONFIG = {
  verbose: { label: "V", color: "#9ca3af", bg: "transparent" },
  debug:   { label: "D", color: "#60a5fa", bg: "transparent" },
  info:    { label: "I", color: "#4ade80", bg: "transparent" },
  warn:    { label: "W", color: "#fbbf24", bg: "rgba(251,191,36,0.08)" },
  error:   { label: "E", color: "#f87171", bg: "rgba(248,113,113,0.10)" },
  fatal:   { label: "F", color: "#e879f9", bg: "rgba(232,121,249,0.12)" },
  unknown: { label: "?", color: "#9ca3af", bg: "transparent" },
} as const;

export function getLevelConfig(level: string) {
  return LEVEL_CONFIG[level.toLowerCase() as keyof typeof LEVEL_CONFIG] ?? LEVEL_CONFIG.unknown;
}
