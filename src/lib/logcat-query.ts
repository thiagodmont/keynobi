/**
 * logcat-query.ts
 *
 * Parses a freeform query string into structured filter tokens and evaluates
 * them against LogcatEntry objects.  This module is the single source of
 * truth for all logcat filtering logic.
 *
 * Supported syntax:
 *   level:error          — minimum log level (priority >=)
 *   tag:MyTag            — tag substring match
 *   tag~:My.*Tag         — tag regex match
 *   -tag:system          — negate (exclude entries matching "system")
 *   message:crash        — message substring
 *   message~:Null.*Ex    — message regex
 *   package:com.example  — package substring (falls back to tag if no package)
 *   package:mine         — match the current project's applicationId
 *   age:5m               — only entries from the last 5 minutes
 *   age:30s / age:1h / age:1d
 *   is:crash             — only crash entries
 *   is:stacktrace        — only stack-trace lines
 *   bare text            — searches tag + message + package (negatable with -)
 *   error / warn / …     — bare level name shorthand for level:X
 */

import type { LogcatEntry } from "@/lib/tauri-api";

// ── Token types ───────────────────────────────────────────────────────────────

export type QueryToken =
  | { type: "level";    value: string;  negate: boolean }
  | { type: "tag";      value: string;  negate: boolean; regex: boolean }
  | { type: "message";  value: string;  negate: boolean; regex: boolean }
  | { type: "package";  value: string;  negate: boolean }
  | { type: "age";      seconds: number }
  | { type: "is";       value: string }
  | { type: "freetext"; value: string;  negate: boolean };

// ── Constants ─────────────────────────────────────────────────────────────────

export const LEVEL_NAMES = [
  "verbose", "debug", "info", "warn", "error", "fatal",
] as const;

export const QUERY_KEYS = [
  "level:", "tag:", "tag~:", "-tag:", "message:", "message~:", "-message:",
  "package:", "age:", "is:",
];

export const AGE_SUGGESTIONS = ["30s", "1m", "5m", "15m", "1h", "6h", "1d"];
export const IS_SUGGESTIONS = ["crash", "stacktrace"];

const LEVEL_PRIORITY: Record<string, number> = {
  verbose: 0, v: 0,
  debug: 1, d: 1,
  info: 2, i: 2,
  warn: 3, w: 3, warning: 3,
  error: 4, e: 4,
  fatal: 5, f: 5, assert: 5, a: 5,
};

// ── Age parser ────────────────────────────────────────────────────────────────

export function parseAge(value: string): number | null {
  const m = value.match(/^(\d+(?:\.\d+)?)(s|m|h|d)$/i);
  if (!m) return null;
  const n = parseFloat(m[1]);
  switch (m[2].toLowerCase()) {
    case "s": return n;
    case "m": return n * 60;
    case "h": return n * 3600;
    case "d": return n * 86400;
    default: return null;
  }
}

// ── Query parser ──────────────────────────────────────────────────────────────

export function parseQuery(raw: string): QueryToken[] {
  if (!raw.trim()) return [];

  // Split on whitespace, respecting double-quoted strings.
  // Standalone && / & tokens between conditions are explicit AND connectors
  // and are skipped (AND is already the implicit default between tokens).
  const parts: string[] = [];
  let current = "";
  let inQuote = false;
  for (const ch of raw) {
    if (ch === '"') { inQuote = !inQuote; continue; }
    if (ch === " " && !inQuote) {
      if (current) { parts.push(current); current = ""; }
    } else {
      current += ch;
    }
  }
  if (current) parts.push(current);

  const tokens: QueryToken[] = [];

  for (const part of parts) {
    // Skip empty tokens and explicit AND connectors (&&, &)
    if (!part || part === "&&" || part === "&") continue;

    const negate = part.startsWith("-");
    const p = negate ? part.slice(1) : part;

    // Regex key variant: tag~:value or message~:value
    const regexMatch = p.match(/^(tag|message)~:(.+)$/);
    if (regexMatch) {
      tokens.push({
        type: regexMatch[1] as "tag" | "message",
        value: regexMatch[2],
        negate,
        regex: true,
      });
      continue;
    }

    const colonIdx = p.indexOf(":");
    if (colonIdx > 0 && colonIdx < p.length - 1) {
      const key = p.slice(0, colonIdx).toLowerCase();
      const value = p.slice(colonIdx + 1);

      switch (key) {
        case "level":
          tokens.push({ type: "level", value: value.toLowerCase(), negate });
          break;
        case "tag":
          tokens.push({ type: "tag", value, negate, regex: false });
          break;
        case "message":
        case "msg":
          tokens.push({ type: "message", value, negate, regex: false });
          break;
        case "package":
        case "pkg":
          tokens.push({ type: "package", value, negate });
          break;
        case "age": {
          const secs = parseAge(value);
          if (secs !== null && !negate) tokens.push({ type: "age", seconds: secs });
          break;
        }
        case "is":
          if (!negate) tokens.push({ type: "is", value: value.toLowerCase() });
          break;
        default:
          // Unknown key → freetext
          tokens.push({ type: "freetext", value: p, negate });
      }
      continue;
    }

    // Bare level name shorthand (e.g. "error", "warn")
    if (!negate && LEVEL_PRIORITY[p.toLowerCase()] !== undefined) {
      tokens.push({ type: "level", value: p.toLowerCase(), negate: false });
      continue;
    }

    // Default: freetext
    tokens.push({ type: "freetext", value: p, negate });
  }

  return tokens;
}

// ── Timestamp parsing ─────────────────────────────────────────────────────────

// Cache year so we don't call new Date() on every entry comparison.
let _cachedYear = new Date().getFullYear();
let _yearExpiry = 0;

function getYear(): number {
  const now = Date.now();
  if (now > _yearExpiry) {
    _cachedYear = new Date(now).getFullYear();
    _yearExpiry = now + 60_000;
  }
  return _cachedYear;
}

/**
 * Parse a logcat `threadtime` timestamp ("MM-DD HH:MM:SS.mmm") to epoch ms.
 * Returns 0 if parsing fails (treated as "keep the entry").
 */
export function parseLogcatTimestamp(ts: string): number {
  const m = ts.match(/^(\d{2})-(\d{2})\s+(\d{2}):(\d{2}):(\d{2})\.(\d+)$/);
  if (!m) return 0;
  const [, mo, d, hh, mm, ss, ms] = m;
  const year = getYear();
  return new Date(`${year}-${mo}-${d}T${hh}:${mm}:${ss}.${ms.padEnd(3, "0")}`).getTime();
}

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
const STACK_FRAME_RE =
  /^\s+at\s+([\w$.]+)\.([\w$<>]+)\(([\w$]+\.(?:kt|java)):(\d+)\)/;

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
  "com.google.common.",   // Guava
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
  "com.unity3d.",
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

// ── package:mine resolution ───────────────────────────────────────────────────

/** Set by the app on project open. Used to resolve `package:mine`. */
let _minePackage: string | null = null;

export function setMinePackage(pkg: string | null): void {
  _minePackage = pkg;
}

export function getMinePackage(): string | null {
  return _minePackage;
}

// ── Separator entry check ─────────────────────────────────────────────────────

/** Separator entries (process start/die) should only be filtered by age. */
export function isSeparatorEntry(entry: LogcatEntry): boolean {
  return entry.kind === "processDied" || entry.kind === "processStarted";
}

// ── Main matcher ──────────────────────────────────────────────────────────────

/**
 * Test whether a `LogcatEntry` satisfies all tokens in a parsed query.
 *
 * @param entry    The entry to test.
 * @param tokens   Parsed tokens (from `parseQuery`).
 * @param now      `Date.now()` — passed in so the caller controls time.
 */
export function matchesQuery(
  entry: LogcatEntry,
  tokens: QueryToken[],
  now: number
): boolean {
  if (tokens.length === 0) return true;

  // Separator entries bypass all filters except age.
  if (isSeparatorEntry(entry)) {
    const ageToken = tokens.find((t) => t.type === "age");
    if (!ageToken) return true;
    const entryTime = parseLogcatTimestamp(entry.timestamp);
    return entryTime === 0 || now - entryTime <= (ageToken as { type: "age"; seconds: number }).seconds * 1000;
  }

  for (const token of tokens) {
    if (!matchToken(entry, token, now)) return false;
  }
  return true;
}

function matchToken(entry: LogcatEntry, token: QueryToken, now: number): boolean {
  switch (token.type) {
    case "level": {
      const entryPrio = LEVEL_PRIORITY[entry.level] ?? 0;
      const filterPrio = LEVEL_PRIORITY[token.value] ?? 0;
      const matches = entryPrio >= filterPrio;
      return token.negate ? !matches : matches;
    }
    case "tag": {
      const matches = token.regex
        ? safeRegexTest(token.value, entry.tag)
        : entry.tag.toLowerCase().includes(token.value.toLowerCase());
      return token.negate ? !matches : matches;
    }
    case "message": {
      const matches = token.regex
        ? safeRegexTest(token.value, entry.message)
        : entry.message.toLowerCase().includes(token.value.toLowerCase());
      return token.negate ? !matches : matches;
    }
    case "package": {
      const resolvedValue =
        token.value === "mine" ? (_minePackage ?? "mine") : token.value;
      const haystack = (entry.package ?? entry.tag).toLowerCase();
      const matches = haystack.includes(resolvedValue.toLowerCase());
      return token.negate ? !matches : matches;
    }
    case "age": {
      const entryTime = parseLogcatTimestamp(entry.timestamp);
      if (entryTime === 0) return true; // unparseable → keep
      return now - entryTime <= token.seconds * 1000;
    }
    case "is":
      if (token.value === "crash") return entry.isCrash;
      if (token.value === "stacktrace") return isStackTraceLine(entry.message);
      return true;
    case "freetext": {
      const q = token.value.toLowerCase();
      const matches =
        entry.tag.toLowerCase().includes(q) ||
        entry.message.toLowerCase().includes(q) ||
        (entry.package?.toLowerCase().includes(q) ?? false);
      return token.negate ? !matches : matches;
    }
    default:
      return true;
  }
}

// ── Regex cache ───────────────────────────────────────────────────────────────
//
// Without a cache, safeRegexTest creates a new RegExp object on every call.
// With tag~: or message~: filters and 20K entries, that is 20K compilations
// per filter pass. A small insertion-order LRU (8 entries) eliminates this
// cost — there are at most a handful of distinct regex patterns active at once.

const _regexCache = new Map<string, RegExp | null>();
const MAX_REGEX_CACHE = 8;

function getCachedRegex(pattern: string): RegExp | null {
  if (_regexCache.has(pattern)) return _regexCache.get(pattern)!;
  let re: RegExp | null;
  try {
    re = new RegExp(pattern, "i");
  } catch {
    re = null;
  }
  if (_regexCache.size >= MAX_REGEX_CACHE) {
    // Evict the oldest entry (Map preserves insertion order)
    _regexCache.delete(_regexCache.keys().next().value as string);
  }
  _regexCache.set(pattern, re);
  return re;
}

function safeRegexTest(pattern: string, target: string): boolean {
  const re = getCachedRegex(pattern);
  return re ? re.test(target) : target.toLowerCase().includes(pattern.toLowerCase());
}

// ── Query string helpers ──────────────────────────────────────────────────────

/** Replace or insert an `age:` token in a raw query string. */
export function setAgeInQuery(query: string, age: string | null): string {
  const withoutAge = query.replace(/\bage:\S+/g, "").replace(/\s+/g, " ").trim();
  if (!age) return withoutAge;
  return withoutAge ? `${withoutAge} age:${age}` : `age:${age}`;
}

/** Replace or insert a `package:` token in a raw query string. */
export function setPackageInQuery(query: string, pkg: string | null): string {
  const withoutPkg = query.replace(/\bpackage:\S+/g, "").replace(/\s+/g, " ").trim();
  if (!pkg) return withoutPkg;
  return withoutPkg ? `${withoutPkg} package:${pkg}` : `package:${pkg}`;
}

/**
 * Extract the value of the first `package:` token from a raw query string.
 * Returns null when no package token is present.
 */
export function getPackageFromQuery(query: string): string | null {
  const match = query.match(/\bpackage:(\S+)/);
  return match ? match[1] : null;
}

/**
 * Identify which tokens in a group must be evaluated on the frontend.
 *
 * The Rust backend filter spec has exactly one slot per field:
 *   minLevel, tag (single substring), text (shared by message: and freetext),
 *   package, and an onlyCrashes boolean.
 *
 * This function returns every token that the backend either cannot handle at
 * all, or cannot handle because its slot is already occupied by an earlier
 * token of the same type ("overflow").
 *
 * Tokens always handled on the frontend:
 *   • age:N          — time-based, requires live Date.now()
 *   • negated (-X)   — backend has no negation support
 *   • tag~: / msg~:  — backend does substring only, not regex
 *   • is:stacktrace  — backend does not have a stacktrace filter
 *
 * Overflow tokens (second+ occurrence of a backend-handled type):
 *   • 2nd+ level:    — only the first minLevel is sent
 *   • 2nd+ tag:      — only the first tag substring is sent
 *   • 2nd+ message:  — only the first text slot is sent
 *   • 2nd+ freetext  — shares the same text slot as message:
 *   • 2nd+ package:  — only the first package is sent
 *
 * For example, `message:socket message:IPPROTO_TCP` — the backend pre-filters
 * by "socket"; the frontend must additionally verify "IPPROTO_TCP" for correct
 * AND semantics.
 */
export function getFrontendOnlyTokens(tokens: QueryToken[]): QueryToken[] {
  // Track which backend spec slots have been consumed.
  let levelConsumed = false;
  let tagConsumed = false;
  let textConsumed = false; // shared by message: and freetext
  let packageConsumed = false;

  return tokens.filter((token) => {
    // Always frontend-only
    if (token.type === "age") return true;
    if ("negate" in token && token.negate) return true;
    if ((token.type === "tag" || token.type === "message") && token.regex) return true;
    // is:stacktrace — backend has no handler for this
    if (token.type === "is" && token.value === "stacktrace") return true;

    // For backend-handleable tokens: first occurrence goes to backend, rest → frontend
    switch (token.type) {
      case "level":
        if (!levelConsumed) { levelConsumed = true; return false; }
        return true;
      case "tag":
        if (!tagConsumed) { tagConsumed = true; return false; }
        return true;
      case "message":
        if (!textConsumed) { textConsumed = true; return false; }
        return true;
      case "package":
        if (!packageConsumed) { packageConsumed = true; return false; }
        return true;
      case "is":
        // is:crash → onlyCrashes flag (boolean, no overflow); handled above for stacktrace
        return false;
      case "freetext":
        if (!textConsumed) { textConsumed = true; return false; }
        return true;
      default:
        return false;
    }
  });
}
export function getActiveTokenContext(query: string): {
  key: string | null;
  partial: string;
  offset: number;
} {
  // If the query ends with an explicit AND connector (with or without trailing
  // space), treat the context as an empty new token so the autocomplete shows
  // all available key suggestions.
  const trimmedEnd = query.trimEnd();
  if (trimmedEnd.endsWith("&&") || trimmedEnd.endsWith(" &")) {
    return { key: null, partial: "", offset: query.length };
  }

  const lastSpace = query.lastIndexOf(" ");
  const lastToken = query.slice(lastSpace + 1);

  // Skip the token if it is a bare AND connector (e.g. user typed "&&" without
  // trailing space — show full suggestions rather than filtering by "&&").
  if (lastToken === "&&" || lastToken === "&") {
    return { key: null, partial: "", offset: query.length };
  }

  const negated = lastToken.startsWith("-");
  const token = negated ? lastToken.slice(1) : lastToken;
  const colonIdx = token.indexOf(":");
  const offset = lastSpace + 1;

  if (colonIdx > 0) {
    return {
      key: token.slice(0, colonIdx).replace(/~$/, ""),
      partial: token.slice(colonIdx + 1),
      offset,
    };
  }
  return { key: null, partial: token, offset };
}

// ── OR-group support ──────────────────────────────────────────────────────────

/**
 * A FilterGroup is a set of tokens that are AND-ed together.
 * Multiple groups joined by `|` are OR-ed together.
 */
export type FilterGroup = QueryToken[];

/**
 * Parse a query string that may contain `|`-separated OR groups.
 *
 * "level:error tag:App | is:crash"
 *   → [ [level:error, tag:App], [is:crash] ]
 *
 * A query with no `|` returns a single-element array containing the
 * existing `parseQuery()` result — fully backward-compatible.
 *
 * Empty groups produced by trailing/double pipes are filtered out so that
 * a query like `"level:error | "` continues to filter correctly rather than
 * treating the empty trailing group as an always-true match.
 */
export function parseFilterGroups(raw: string): FilterGroup[] {
  if (!raw.trim()) return [[]];
  const groups = raw
    .split(/\s*\|\s*/)
    .map((segment) => parseQuery(segment.trim()))
    .filter((g) => g.length > 0);
  // Return a single empty group when everything was stripped (empty query edge case)
  return groups.length > 0 ? groups : [[]];
}

/**
 * Test whether a `LogcatEntry` satisfies at least one group (OR semantics).
 * Each group is internally AND-ed via the existing `matchesQuery()`.
 *
 * @param entry   The entry to test.
 * @param groups  Parsed OR groups (from `parseFilterGroups`).
 * @param now     `Date.now()` — passed in so the caller controls time.
 */
export function matchesFilterGroups(
  entry: LogcatEntry,
  groups: FilterGroup[],
  now: number,
): boolean {
  if (groups.length === 0) return true;
  if (groups.length === 1 && groups[0].length === 0) return true;
  return groups.some((group) => matchesQuery(entry, group, now));
}

/**
 * Return the number of OR groups in a raw query string.
 * A query with no `|` has 1 group. Empty string → 0.
 */
export function getGroupCount(raw: string): number {
  if (!raw.trim()) return 0;
  return raw.split("|").length;
}

/**
 * Append a new OR group separator to the query, returning the new string.
 * Ensures exactly one space before and after the `|`.
 * Does nothing if the query already ends with `|` or is empty.
 */
export function addOrGroup(query: string): string {
  const trimmed = query.trimEnd();
  if (!trimmed || trimmed.endsWith("|")) return query;
  return `${trimmed} | `;
}

/**
 * Append an explicit AND connector (`&&`) to the current group in the query.
 * Does nothing if the query is empty or already ends with `&&`, `&`, or `|`.
 * The `&&` connector is cosmetic — it is skipped by `parseQuery` — but it
 * makes the AND relationship visually explicit alongside the `|` OR separator.
 */
export function addAndConnector(query: string): string {
  const trimmed = query.trimEnd();
  if (!trimmed) return query;
  if (trimmed.endsWith("&&") || trimmed.endsWith("&") || trimmed.endsWith("|")) return query;
  return `${trimmed} && `;
}

/**
 * Return the segment of the query string that the cursor is currently inside
 * (i.e. the group the user is actively editing). Operates on text to the
 * left of `cursorPos` (defaults to full string).
 */
export function getActiveGroupSegment(
  query: string,
  cursorPos?: number,
): string {
  const pos = cursorPos ?? query.length;
  const text = query.slice(0, pos);
  const lastPipeInText = text.lastIndexOf("|");

  // Start of the group the cursor is in
  const groupStart = lastPipeInText === -1 ? 0 : lastPipeInText + 1;

  // End of the group: next pipe at or after cursor position, or end of string
  const nextPipe = query.indexOf("|", pos);
  const groupEnd = nextPipe === -1 ? query.length : nextPipe;

  return query.slice(groupStart, groupEnd).trim();
}

/**
 * Compute the character offset within `query` where the active group begins
 * (the character right after the last `|` separator, or 0 if no `|`).
 */
export function getActiveGroupOffset(
  query: string,
  cursorPos?: number,
): number {
  const text = cursorPos !== undefined ? query.slice(0, cursorPos) : query;
  const pipeIdx = text.lastIndexOf("|");
  if (pipeIdx === -1) return 0;
  // Advance past any whitespace that follows the pipe
  let offset = pipeIdx + 1;
  while (offset < query.length && query[offset] === " ") offset++;
  return offset;
}
