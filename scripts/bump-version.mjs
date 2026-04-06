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

  // Strict semver validation: three non-negative integers, no leading zeros,
  // no whitespace, no other characters.
  const SEMVER_RE = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
  if (!SEMVER_RE.test(current)) {
    throw new Error(
      `could not parse version "${current}" from package.json. Expected semver (e.g. 1.2.3).`
    );
  }

  let [major, minor, patch] = current.split(".").map(Number);
  if (type === "patch") patch += 1;
  if (type === "minor") { minor += 1; patch = 0; }
  if (type === "major") { major += 1; minor = 0; patch = 0; }

  return `${major}.${minor}.${patch}`;
}

// ── File sync helpers (same logic as sync-version.mjs) ──────────────────────

function syncToFiles(newVersion) {
  // Cargo.toml — replace `version = "..."` (first occurrence, the package version)
  const cargoPath = resolve(root, "src-tauri/Cargo.toml");
  const cargo = readFileSync(cargoPath, "utf-8");
  const updatedCargo = cargo.replace(
    /^version = ".+?"/m,
    `version = "${newVersion}"`
  );
  writeFileSync(cargoPath, updatedCargo);

  // tauri.conf.json — replace the top-level "version" field
  const tauriPath = resolve(root, "src-tauri/tauri.conf.json");
  const tauri = JSON.parse(readFileSync(tauriPath, "utf-8"));
  tauri.version = newVersion;
  writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");
}

// ── CLI entry point ──────────────────────────────────────────────────────────

// Only run when executed directly (not when imported by tests).
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const type = process.argv[2] ?? "patch";

  // Read current version
  const pkgPath = resolve(root, "package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
  const current = pkg.version;

  // Bump and sync versions (throws on invalid input or I/O errors)
  try {
    const next = bumpVersion(current, type);

    // Write package.json
    pkg.version = next;
    writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

    // Sync Cargo.toml + tauri.conf.json
    syncToFiles(next);

    // Print the result
    console.log(`${current} → ${next}`);
  } catch (err) {
    console.error(`Error: ${err.message}`);
    process.exit(1);
  }
}
