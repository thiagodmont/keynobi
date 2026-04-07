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
import { bumpVersion, syncToFiles } from "./bump-version.mjs";

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

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Display a prompt and wait for Enter. Ctrl+C exits cleanly.
 *
 * @param {string} prompt
 * @returns {Promise<void>}
 */
function confirm(prompt) {
  return new Promise((resolve) => {
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    rl.on("SIGINT", () => {
      rl.close();
      console.log("\nCancelled.");
      process.exit(0);
    });
    rl.question(prompt, () => {
      rl.close();
      resolve();
    });
  });
}

// ── CLI entry point (skipped when imported by tests) ─────────────────────────
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  (async () => {
    // ── Parse args ──────────────────────────────────────────────────────────
    let type;
    try {
      type = parseBumpType(process.argv);
    } catch (err) {
      fatal(err.message);
      console.log("Usage: node scripts/release.mjs [patch|minor|major]");
      process.exit(1);
    }

    // ── Read current version ─────────────────────────────────────────────────
    const pkgPath = resolve(root, "package.json");
    const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));
    const current = pkg.version;

    let next;
    try {
      next = bumpVersion(current, type);
    } catch (err) {
      fatal(err.message);
      process.exit(1);
    }

    // ── Gate 1: confirm version bump ─────────────────────────────────────────
    step("Bump version");
    console.log(`  ${current} → ${next}  (${type})\n`);
    await confirm("Press Enter to confirm, or Ctrl+C to cancel: ");

    // ── Write files ──────────────────────────────────────────────────────────
    try {
      pkg.version = next;
      writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
      syncToFiles(next);
    } catch (err) {
      fatal(err.message);
      process.exit(1);
    }

    // ── Update Cargo.lock ─────────────────────────────────────────────────────
    step("Updating Cargo.lock");
    try {
      execSync("npm run rust:check", { stdio: "inherit" });
      info("Cargo.lock updated");
    } catch (err) {
      fatal(err.message);
      process.exit(1);
    }

    // ── Show diff ────────────────────────────────────────────────────────────
    step("Changes");
    try {
      const stat = execSync(
        "git diff --stat HEAD package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock",
        { encoding: "utf-8" }
      );
      stat.trim().split("\n").forEach((l) => console.log("  " + l));
    } catch {
      // Non-fatal — diff display is informational only.
      console.log("  (could not read diff)");
    }

    // ── Gate 2: confirm commit + push ─────────────────────────────────────────
    console.log();
    await confirm("Press Enter to commit and push, or Ctrl+C to cancel: ");

    // ── Git operations ────────────────────────────────────────────────────────
    step("Releasing");
    try {
      execSync(
        "git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock",
        { stdio: "inherit" }
      );
      execSync(`git commit -m "chore: release v${next}"`, { stdio: "inherit" });
      info(`Committed: chore: release v${next}`);
    } catch {
      fatal("git commit failed. Files are updated but not committed — run git status to inspect.");
      process.exit(1);
    }

    try {
      execSync("git push origin main", { stdio: "inherit" });
      info("Pushed to main");
      info(`CI will create tag v${next} and build the release`);
    } catch {
      fatal(`git push failed. Commit exists locally — run: git push origin main`);
      process.exit(1);
    }
  })();
}
