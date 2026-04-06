#!/usr/bin/env node
/**
 * bump-version.mjs — Increment the project version (patch / minor / major).
 *
 * Usage:
 *   npm run version:bump             # patch (default): 0.1.0 → 0.1.1
 *   npm run version:bump -- minor    # minor:           0.1.0 → 0.2.0
 *   npm run version:bump -- major    # major:           0.1.0 → 1.0.0
 */
import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";
import { fileURLToPath } from "url";

const root = resolve(fileURLToPath(import.meta.url), "../..");

// ── Pure semver helper (exported for tests) ──────────────────────────────────

/**
 * Increment a semver string.
 *
 * @param {string} current  - Current version, e.g. "0.1.0"
 * @param {"patch"|"minor"|"major"} [type="patch"]
 * @returns {string} New version string
 * @throws {Error} If type is unrecognised or version is not valid semver
 */
export function bumpVersion(current, type = "patch") {
  const VALID_TYPES = ["patch", "minor", "major"];
  if (!VALID_TYPES.includes(type)) {
    throw new Error(
      `unknown bump type "${type}". Use: patch, minor, or major.`
    );
  }

  const parts = current.split(".").map(Number);
  if (
    parts.length !== 3 ||
    parts.some((p) => !Number.isInteger(p) || isNaN(p))
  ) {
    throw new Error(
      `could not parse version "${current}" from package.json. Expected semver (e.g. 1.2.3).`
    );
  }

  let [major, minor, patch] = parts;
  if (type === "patch") patch += 1;
  if (type === "minor") { minor += 1; patch = 0; }
  if (type === "major") { major += 1; minor = 0; patch = 0; }

  return `${major}.${minor}.${patch}`;
}
