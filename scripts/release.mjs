#!/usr/bin/env node
/**
 * release.mjs — Interactive release script.
 *
 * Usage:
 *   node scripts/release.mjs           # patch (default)
 *   node scripts/release.mjs patch
 *   node scripts/release.mjs minor
 *   node scripts/release.mjs major
 *
 * Or via npm:
 *   npm run release
 *   npm run release -- minor
 *   npm run release -- major
 */
import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";
import { fileURLToPath } from "url";
import { createInterface } from "readline";
import { execSync } from "child_process";
import { bumpVersion } from "./bump-version.mjs";

const root = resolve(fileURLToPath(import.meta.url), "../..");

// ── Colour helpers (matches build-dmg.sh conventions) ────────────────────────
const BOLD  = "\x1b[1m";
const GREEN = "\x1b[0;32m";
const RED   = "\x1b[0;31m";
const RESET = "\x1b[0m";

function step(msg)  { console.log(`\n${BOLD}▶ ${msg}${RESET}`); }
function info(msg)  { console.log(`${BOLD}${GREEN}  ✓${RESET} ${msg}`); }
function fatal(msg) { console.error(`${BOLD}${RED}  ✗${RESET} ${msg}`); }

// ── Exported pure helpers (for tests) ────────────────────────────────────────

/**
 * Parse and validate the bump type from process.argv.
 *
 * @param {string[]} argv - process.argv array
 * @returns {"patch"|"minor"|"major"}
 * @throws {Error} if the type is not one of the three valid values
 */
export function parseBumpType(argv) {
  const type = argv[2] ?? "patch";
  const VALID = ["patch", "minor", "major"];
  if (!VALID.includes(type)) {
    throw new Error(`Unknown bump type "${type}". Use: patch, minor, or major.`);
  }
  return type;
}
