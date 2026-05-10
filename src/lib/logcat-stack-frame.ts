// ── Stack-trace detector ──────────────────────────────────────────────────────

const STACKTRACE_RE = /^(\s+at |\s*Caused by:|\s*\.\.\. \d+ more)/;

export function isStackTraceLine(message: string): boolean {
  return STACKTRACE_RE.test(message);
}

/** Parsed info from an `at com.example.Foo.bar(Foo.kt:42)` frame line. */
export interface StackFrameInfo {
  /** Fully-qualified class, e.g. `com.example.app.MainActivity` */
  classPath: string;
  /** Package portion only, e.g. `com.example.app` */
  packagePath: string;
  /** Source filename, e.g. `MainActivity.kt` */
  filename: string;
  /** 1-based line number */
  line: number;
}

/**
 * Matches: `\tat com.example.app.Foo.bar(Foo.kt:42)`
 *            group1=full qualified class+method  group2=filename  group3=line
 */
const STACK_FRAME_RE = /^\s+at\s+([\w$.]+)\.([\w$<>]+)\(([\w$]+\.(?:kt|java)):(\d+)\)/;

/**
 * Parse a logcat stack frame line into its constituent parts.
 * Returns `null` for non-frame lines (e.g. `Caused by:`, `... N more`).
 */
export function parseStackFrame(message: string): StackFrameInfo | null {
  const m = STACK_FRAME_RE.exec(message);
  if (!m) return null;
  const [, fqClass, , filename, lineStr] = m;
  const line = parseInt(lineStr, 10);
  if (!Number.isFinite(line) || line < 1) return null;
  // Strip the simple class name from the end to get just the package.
  const lastDot = fqClass.lastIndexOf(".");
  const packagePath = lastDot >= 0 ? fqClass.slice(0, lastDot) : fqClass;
  return { classPath: fqClass, packagePath, filename, line };
}

/**
 * Known Android / Java / Kotlin / Google framework package prefixes.
 * Stack frames whose class path starts with any of these are NOT part of
 * the user's project source code and should not get a "jump to Studio" button.
 */
const FRAMEWORK_PREFIXES = [
  "android.",
  "androidx.",
  "com.android.",
  "com.google.android.",
  "com.google.firebase.",
  "com.google.gson.",
  "com.google.common.", // Guava
  "java.",
  "javax.",
  "kotlin.",
  "kotlinx.",
  "dalvik.",
  "sun.",
  "libcore.",
  "org.apache.",
  "org.chromium.",
  "io.flutter.",
];

/**
 * Returns `true` when the class path belongs to the user's project code
 * (i.e. is NOT a known Android / Java / Kotlin / Google framework package).
 *
 * Use this to decide whether to show the "Open in Studio" jump button.
 */
export function isProjectFrame(classPath: string): boolean {
  return !FRAMEWORK_PREFIXES.some((prefix) => classPath.startsWith(prefix));
}
