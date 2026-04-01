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

  // Split on whitespace, respecting double-quoted strings
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
    if (!part) continue;

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

function safeRegexTest(pattern: string, target: string): boolean {
  try {
    return new RegExp(pattern, "i").test(target);
  } catch {
    return target.toLowerCase().includes(pattern.toLowerCase());
  }
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

/** Detect which key the user is currently completing (for autocomplete). */
export function getActiveTokenContext(query: string): {
  key: string | null;
  partial: string;
  offset: number;
} {
  const lastSpace = query.lastIndexOf(" ");
  const lastToken = query.slice(lastSpace + 1);
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
