/**
 * Release workflow guard.
 *
 * The Release workflow starts from a completed CI run and its build job holds
 * the signing and notarization secrets. It must only go on for CI runs that a
 * push to this repository started, and values that come from the triggering
 * run or from earlier jobs must reach shell scripts as environment variables,
 * never pasted into the script text by `${{ }}`.
 *
 * This test reads release.yml directly and fails if either rule is broken.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// vitest runs with the repo root as cwd (see vite.config.ts `test.include`).
const REPO_ROOT = process.cwd();

function readReleaseWorkflow() {
  return readFileSync(join(REPO_ROOT, ".github/workflows/release.yml"), "utf8");
}

/** The names of all jobs (two-space indent under `jobs:`). */
function jobNames(yml) {
  const lines = yml.split("\n");
  const start = lines.indexOf("jobs:");
  expect(start, "jobs: not found in release.yml").toBeGreaterThan(-1);
  return lines
    .slice(start + 1)
    .map((line) => /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line)?.[1])
    .filter(Boolean);
}

/** The lines of one top-level job (two-space indent under `jobs:`). */
function jobBlock(yml, name) {
  const lines = yml.split("\n");
  const start = lines.findIndex((line) => line === `  ${name}:`);
  expect(start, `job "${name}" not found in release.yml`).toBeGreaterThan(-1);
  const end = lines.findIndex((line, i) => i > start && /^ {2}\S/.test(line));
  return lines.slice(start, end === -1 ? undefined : end);
}

/** The job's own `if:` condition, without the `${{ }}` wrapper. */
function jobCondition(yml, name) {
  const line = jobBlock(yml, name).find((l) => /^ {4}if:/.test(l));
  expect(line, `job "${name}" has no if: condition`).toBeDefined();
  return line
    .replace(/^ {4}if:\s*/, "")
    .replace(/^\$\{\{\s*/, "")
    .replace(/\s*\}\}\s*$/, "");
}

/** Every `run:` script in the file: inline values and block scalars. */
function runScripts(yml) {
  const lines = yml.split("\n");
  const scripts = [];
  for (let i = 0; i < lines.length; i++) {
    const match = /^(\s*(?:- )?)run:\s*(.*)$/.exec(lines[i]);
    if (!match) continue;
    const keyIndent = match[1].length;
    const value = match[2];
    if (!/^[|>]/.test(value)) {
      scripts.push({ line: i + 1, text: value });
      continue;
    }
    const body = [];
    let j = i + 1;
    for (; j < lines.length; j++) {
      const line = lines[j];
      if (line.trim() !== "" && line.search(/\S/) <= keyIndent) break;
      body.push(line);
    }
    scripts.push({ line: i + 1, text: body.join("\n") });
    i = j - 1;
  }
  return scripts;
}

describe("release workflow guard", () => {
  it("starts only for successful CI runs of a push to this repository", () => {
    const condition = jobCondition(readReleaseWorkflow(), "check");

    expect(condition).toContain("github.event.workflow_run.conclusion == 'success'");
    expect(condition).toContain("github.event.workflow_run.event == 'push'");
    expect(condition).toContain(
      "github.event.workflow_run.head_repository.full_name == github.repository"
    );
    expect(condition).not.toContain("||");
  });

  it("gates every other job on the check job", () => {
    const yml = readReleaseWorkflow();

    const names = jobNames(yml);
    expect(names).toContain("check");
    expect(names.length).toBeGreaterThan(1);
    for (const name of names.filter((n) => n !== "check")) {
      const needs = jobBlock(yml, name).find((l) => /^ {4}needs:/.test(l));
      expect(needs, `job "${name}" does not need check`).toMatch(/\bcheck\b/);
    }
    // always() and friends run a job even when the jobs it needs were skipped.
    expect(yml).not.toMatch(/\b(always|failure|cancelled)\(\)/);
  });

  it("finds the run scripts it checks", () => {
    const scripts = runScripts(readReleaseWorkflow());

    expect(scripts.length).toBeGreaterThan(5);
    expect(scripts.some((s) => s.text.includes('mv "$DMG" "$DEST"'))).toBe(true);
  });

  it("passes event and job values to scripts through env, not ${{ }}", () => {
    const offenders = runScripts(readReleaseWorkflow())
      .filter((s) => /\$\{\{[^}]*\b(needs|github\.event|steps)\./.test(s.text))
      .map((s) => `release.yml:${s.line}`);

    expect(offenders).toEqual([]);
  });
});
