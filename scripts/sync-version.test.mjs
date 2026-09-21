/**
 * Version drift guard.
 *
 * package.json is the single editable version; scripts/sync-version.mjs
 * copies it into src-tauri/Cargo.toml and src-tauri/tauri.conf.json. Nothing
 * runs that check automatically today, so a hand-edited version or a merge
 * that resolves one of the three files and not the others ships a signed DMG
 * whose internal version disagrees with the git tag. This test reads all
 * three real files and fails as soon as they disagree.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// vitest runs with the repo root as cwd (see vite.config.ts `test.include`).
const REPO_ROOT = process.cwd();

const SEMVER_RE = /^\d+\.\d+\.\d+/;

function pkgVersion() {
  const pkg = JSON.parse(
    readFileSync(join(REPO_ROOT, "package.json"), "utf8")
  );
  return pkg.version;
}

// Same regex sync-version.mjs uses (line 28) to read the field it maintains.
function cargoVersion() {
  const cargo = readFileSync(
    join(REPO_ROOT, "src-tauri/Cargo.toml"),
    "utf8"
  );
  return cargo.match(/^version = "(.+?)"/m)?.[1];
}

function tauriVersion() {
  const tauri = JSON.parse(
    readFileSync(join(REPO_ROOT, "src-tauri/tauri.conf.json"), "utf8")
  );
  return tauri.version;
}

describe("version sync", () => {
  it("each of the three files has a parseable semver version", () => {
    expect(
      pkgVersion(),
      "package.json's version is not a MAJOR.MINOR.PATCH semver string."
    ).toMatch(SEMVER_RE);
    expect(
      cargoVersion(),
      "src-tauri/Cargo.toml's [package] version is not a MAJOR.MINOR.PATCH " +
        "semver string."
    ).toMatch(SEMVER_RE);
    expect(
      tauriVersion(),
      "src-tauri/tauri.conf.json's version is not a MAJOR.MINOR.PATCH semver " +
        "string."
    ).toMatch(SEMVER_RE);
  });

  it("src-tauri/Cargo.toml's version matches package.json", () => {
    const pkg = pkgVersion();
    const cargo = cargoVersion();
    expect(
      cargo,
      `src-tauri/Cargo.toml's version (${cargo}) does not match ` +
        `package.json's version (${pkg}). Run \`npm run version:sync\` to fix.`
    ).toBe(pkg);
  });

  it("src-tauri/tauri.conf.json's version matches package.json", () => {
    const pkg = pkgVersion();
    const tauri = tauriVersion();
    expect(
      tauri,
      `src-tauri/tauri.conf.json's version (${tauri}) does not match ` +
        `package.json's version (${pkg}). Run \`npm run version:sync\` to fix.`
    ).toBe(pkg);
  });
});
