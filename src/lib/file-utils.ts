/**
 * Shared file-type utilities. Path utilities assume POSIX-style '/' separators (macOS target).
 */

export type Language = "kotlin" | "gradle" | "xml" | "json" | "text";

// ── Language detection ────────────────────────────────────────────────────────

export function detectLanguage(path: string): Language {
  if (path.endsWith(".kt")) return "kotlin";
  if (path.endsWith(".gradle.kts") || path.endsWith(".gradle")) return "gradle";
  if (path.endsWith(".xml")) return "xml";
  if (path.endsWith(".json")) return "json";
  return "text";
}

// ── File-type display info ─────────────────────────────────────────────────────

export interface FileTypeInfo {
  label: string;
  color: string;
}

const FILE_TYPE_MAP: Record<Language, FileTypeInfo> = {
  kotlin:  { label: "K", color: "#a97bff" },
  gradle:  { label: "G", color: "#02b10a" },
  xml:     { label: "X", color: "#f0883e" },
  json:    { label: "J", color: "#e8c07d" },
  text:    { label: "T", color: "#858585" },
};

export function getFileTypeInfo(language: Language): FileTypeInfo {
  return FILE_TYPE_MAP[language] ?? FILE_TYPE_MAP.text;
}

export function getFileTypeInfoByPath(path: string): FileTypeInfo {
  return getFileTypeInfo(detectLanguage(path));
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/** Extract the filename from a POSIX path (last segment after the final `/`). */
export function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

/** Extract the parent directory from a POSIX path. */
export function dirname(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx > 0 ? path.slice(0, idx) : "/";
}

/** Join two POSIX path segments, avoiding double slashes. */
export function joinPath(parent: string, name: string): string {
  return parent.endsWith("/") ? `${parent}${name}` : `${parent}/${name}`;
}
