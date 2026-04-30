#!/usr/bin/env node
/**
 * changelog.mjs — Pure changelog generation utilities.
 *
 * All functions are exported for testing. This module is only imported
 * by release.mjs — it has no CLI entry point.
 */
import { readFileSync, writeFileSync } from "fs";
import { execSync } from "child_process";

// ── getCommitsSinceTag ────────────────────────────────────────────────────────

/**
 * Return raw `git log --oneline` lines since the most recent tag.
 * If no tag exists, returns all commits.
 *
 * @param {string} [fromRef] - override the auto-detected tag (for testing)
 * @returns {string[]}
 */
export function getCommitsSinceTag(fromRef) {
  let ref = fromRef;
  if (!ref) {
    try {
      ref = execSync("git describe --tags --abbrev=0", { encoding: "utf-8" }).trim();
    } catch {
      ref = execSync("git rev-list --max-parents=0 HEAD", { encoding: "utf-8" }).trim();
    }
  }
  const log = execSync(`git log --oneline "${ref}"..HEAD`, { encoding: "utf-8" }).trim();
  return log ? log.split("\n") : [];
}

// ── parseCommits ──────────────────────────────────────────────────────────────

/**
 * Parse raw `git log --oneline` lines into structured commit objects.
 *
 * @param {string[]} lines
 * @returns {{ hash: string, type: string, scope: string|null, breaking: boolean, description: string }[]}
 */
export function parseCommits(lines) {
  // Matches: <hash> <type>[!][(scope)][!]: <description>
  const RE = /^([a-f0-9]+)\s+([a-z]+)(!)?(?:\(([^)]+)\))?(!)?:\s+(.+)$/i;
  // Matches GitHub squash titles commonly produced from branch/PR names:
  // <hash> <type>/<description>
  const SLASH_RE = /^([a-f0-9]+)\s+([a-z]+)\/(.+)$/i;
  return lines.map((line) => {
    const m = line.match(RE);
    if (m) {
      return {
        hash: m[1],
        type: m[2].toLowerCase(),
        scope: m[4] ?? null,
        breaking: !!(m[3] || m[5]),
        description: m[6],
      };
    }

    const slash = line.match(SLASH_RE);
    if (slash) {
      return {
        hash: slash[1],
        type: slash[2].toLowerCase(),
        scope: null,
        breaking: false,
        description: slash[3].trim(),
      };
    }

    const hash = line.split(" ")[0] ?? "";
    const description = line.slice(hash.length + 1).trim();
    return { hash, type: "unknown", scope: null, breaking: false, description };
  });
}

// ── filterUserFacing ──────────────────────────────────────────────────────────

const USER_FACING_TYPES = new Set(["feat", "fix", "perf", "refactor"]);

/**
 * Keep only user-facing commits, stripping noise.
 *
 * @param {{ type: string, description: string, breaking: boolean }[]} commits
 * @returns {typeof commits}
 */
export function filterUserFacing(commits) {
  return commits.filter((c) => {
    if (!USER_FACING_TYPES.has(c.type)) return false;
    if (/\[skip ci\]/i.test(c.description)) return false;
    if (/^release v/i.test(c.description)) return false;
    return true;
  });
}

// ── groupBySection ────────────────────────────────────────────────────────────

const SECTION_ORDER = ["Breaking Changes", "Added", "Fixed", "Changed"];

const TYPE_TO_SECTION = {
  feat: "Added",
  fix: "Fixed",
  perf: "Changed",
  refactor: "Changed",
};

/**
 * Group commit descriptions into Keep-a-Changelog sections.
 *
 * @param {{ type: string, description: string, breaking: boolean }[]} commits
 * @returns {Map<string, string[]>}
 */
export function groupBySection(commits) {
  const acc = new Map();

  for (const c of commits) {
    const section = c.breaking ? "Breaking Changes" : TYPE_TO_SECTION[c.type];
    if (!section) continue;
    if (!acc.has(section)) acc.set(section, []);
    acc.get(section).push(c.description);
  }

  const result = new Map();
  for (const s of SECTION_ORDER) {
    if (acc.has(s)) result.set(s, acc.get(s));
  }
  return result;
}

// ── formatEntry ───────────────────────────────────────────────────────────────

/**
 * Render a CHANGELOG entry block as a markdown string.
 *
 * @param {string} version - e.g. "0.1.5"
 * @param {string} date    - e.g. "2026-04-09"
 * @param {Map<string, string[]>} sections - from groupBySection
 * @returns {string}
 */
export function formatEntry(version, date, sections) {
  let out = `## [${version}] — ${date}\n`;
  for (const [section, items] of sections) {
    out += `\n### ${section}\n`;
    for (const item of items) {
      out += `- ${item}\n`;
    }
  }
  return out;
}

// ── prependToChangelog ────────────────────────────────────────────────────────

/**
 * Insert `entry` into CHANGELOG.md immediately after the first `---` separator
 * that follows the header block (i.e., before the first versioned section).
 *
 * @param {string} changelogPath - absolute path to CHANGELOG.md
 * @param {string} entry         - formatted markdown block from formatEntry()
 */
export function prependToChangelog(changelogPath, entry) {
  const content = readFileSync(changelogPath, "utf-8");
  const sepIdx = content.indexOf("\n---\n");
  if (sepIdx === -1) {
    writeFileSync(changelogPath, content + "\n---\n\n" + entry);
    return;
  }
  const afterSep = sepIdx + "\n---\n".length;
  const before = content.slice(0, afterSep);
  const after  = content.slice(afterSep);
  writeFileSync(changelogPath, before + "\n" + entry + "\n---\n" + after);
}
