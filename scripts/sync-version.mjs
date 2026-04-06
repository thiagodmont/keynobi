#!/usr/bin/env node
/**
 * Single-source version management.
 *
 * Reads the version from package.json (canonical source) and syncs it into:
 *   - src-tauri/Cargo.toml
 *   - src-tauri/tauri.conf.json
 *
 * Usage:
 *   node scripts/sync-version.mjs          # sync all files
 *   node scripts/sync-version.mjs --check  # exit 1 if any file is out of sync
 */
import { readFileSync, writeFileSync } from "fs";
import { resolve } from "path";
import { fileURLToPath } from "url";

const root = resolve(fileURLToPath(import.meta.url), "../..");

const pkg = JSON.parse(readFileSync(resolve(root, "package.json"), "utf-8"));
const version = pkg.version;

const cargoPath = resolve(root, "src-tauri/Cargo.toml");
const tauriPath = resolve(root, "src-tauri/tauri.conf.json");

const cargo = readFileSync(cargoPath, "utf-8");
const tauri = JSON.parse(readFileSync(tauriPath, "utf-8"));

const cargoVersion = cargo.match(/^version = "(.+?)"/m)?.[1];
const tauriVersion = tauri.version;

if (process.argv.includes("--check")) {
  const mismatches = [];
  if (cargoVersion !== version)
    mismatches.push(`Cargo.toml: ${cargoVersion} !== ${version}`);
  if (tauriVersion !== version)
    mismatches.push(`tauri.conf.json: ${tauriVersion} !== ${version}`);
  if (mismatches.length > 0) {
    console.error("Version mismatch detected:\n" + mismatches.join("\n"));
    console.error('\nFix with: node scripts/sync-version.mjs');
    process.exit(1);
  }
  console.log(`✓ All versions in sync: ${version}`);
  process.exit(0);
}

const updatedCargo = cargo.replace(
  /^version = ".+?"/m,
  `version = "${version}"`
);
writeFileSync(cargoPath, updatedCargo);

tauri.version = version;
writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");

console.log(`Synced version ${version} to Cargo.toml and tauri.conf.json`);
