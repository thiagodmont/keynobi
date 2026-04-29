import type { LogcatFilterSpec } from "@/lib/tauri-api";
import {
  getFrontendOnlyTokens,
  getMinePackage,
  type FilterGroup,
  type QueryToken,
} from "@/lib/logcat-query";

const LEVEL_PRIORITY_MAP: Record<string, number> = {
  verbose: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
  fatal: 5,
};

const EMPTY_FILTER_SPEC: LogcatFilterSpec = {
  minLevel: null,
  tag: null,
  text: null,
  package: null,
  onlyCrashes: false,
};

export function emptyLogcatFilterSpec(): LogcatFilterSpec {
  return { ...EMPTY_FILTER_SPEC };
}

/**
 * Extract the subset of query tokens that can be applied by the Rust backend.
 * Complex tokens stay frontend-only so the backend never over-filters.
 */
export function tokensToFilterSpec(tokens: QueryToken[]): LogcatFilterSpec {
  const spec = emptyLogcatFilterSpec();

  for (const token of tokens) {
    if ("negate" in token && token.negate) continue;

    switch (token.type) {
      case "level":
        if (!spec.minLevel) spec.minLevel = token.value;
        break;
      case "tag":
        if (!token.regex && !spec.tag) spec.tag = token.value;
        break;
      case "message":
        if (!token.regex && !spec.text) spec.text = token.value;
        break;
      case "package": {
        if (!spec.package) {
          const pkg = token.value === "mine" ? (getMinePackage() ?? null) : token.value;
          if (pkg) spec.package = pkg;
        }
        break;
      }
      case "is":
        if (token.value === "crash") spec.onlyCrashes = true;
        break;
      case "freetext":
        if (!spec.text) spec.text = token.value;
        break;
    }
  }

  return spec;
}

/**
 * Compute the most permissive backend filter that is safe for all OR groups.
 * Precise OR semantics remain in the frontend.
 */
export function groupsToFilterSpec(groups: FilterGroup[]): LogcatFilterSpec {
  if (groups.length === 0) return emptyLogcatFilterSpec();
  if (groups.length === 1) return tokensToFilterSpec(groups[0]);

  const specs = groups.map((g) => tokensToFilterSpec(g));

  const levels = specs.map((s) => (s.minLevel ? (LEVEL_PRIORITY_MAP[s.minLevel] ?? 0) : 0));
  const minLevelPriority = Math.min(...levels);
  const minLevel =
    minLevelPriority === 0
      ? null
      : (Object.entries(LEVEL_PRIORITY_MAP).find(([, v]) => v === minLevelPriority)?.[0] ?? null);

  const tags = specs.map((s) => s.tag);
  const tag = tags.every((t) => t !== null && t === tags[0]) ? tags[0] : null;

  const texts = specs.map((s) => s.text);
  const text = texts.every((t) => t !== null && t === texts[0]) ? texts[0] : null;

  const pkgs = specs.map((s) => s.package);
  const packageFilter = pkgs.every((p) => p !== null && p === pkgs[0]) ? pkgs[0] : null;

  const onlyCrashes = specs.every((s) => s.onlyCrashes);

  return { minLevel, tag, text, package: packageFilter, onlyCrashes };
}

export function hasAnyFrontendOnlyLogic(groups: FilterGroup[]): boolean {
  if (groups.length > 1) return true;
  return getFrontendOnlyTokens(groups[0] ?? []).length > 0;
}
