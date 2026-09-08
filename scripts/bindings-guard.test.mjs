/**
 * TypeScript-bindings staleness guard, guard.
 *
 * The CI "Check TypeScript bindings are up to date" step and the local
 * `check:bindings` npm script both regenerate `src/bindings/` with
 * `cargo test`/`cargo test --lib` and then assert the working tree is clean.
 * Two failure modes have crept into that pair before:
 *
 *   1. Swallowing the regeneration command's own failure (`2>/dev/null`,
 *      piping into `grep`, or `|| true`), so a compile error or a failing
 *      lib test never surfaces.
 *   2. Asserting cleanliness with `git diff --exit-code`, which is blind to
 *      untracked files — a brand-new `src/bindings/*.ts` file from a newly
 *      exported Rust type passes vacuously.
 *
 * This test reads the two source files directly and fails if either pattern
 * reappears, so the regression is caught by `npm test` rather than by
 * reviewer memory.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// vitest runs with the repo root as cwd (see vite.config.ts `test.include`).
const REPO_ROOT = process.cwd();

function readCiWorkflow() {
  return readFileSync(join(REPO_ROOT, ".github/workflows/ci.yml"), "utf8");
}

function readPackageJson() {
  return readFileSync(join(REPO_ROOT, "package.json"), "utf8");
}

/** The "Check TypeScript bindings are up to date" step body, isolated from the rest of ci.yml. */
function bindingsStep(ciYml) {
  const marker = "Check TypeScript bindings are up to date";
  const start = ciYml.indexOf(marker);
  expect(start, `"${marker}" step not found in ci.yml`).toBeGreaterThan(-1);

  const nextStep = ciYml.indexOf("\n      - name:", start);
  return nextStep === -1 ? ciYml.slice(start) : ciYml.slice(start, nextStep);
}

describe("bindings staleness guard", () => {
  it("runs the regeneration command without swallowing its failure", () => {
    const step = bindingsStep(readCiWorkflow());

    expect(step).toContain("cargo test --lib");
    expect(step).not.toContain("2>/dev/null");
    expect(step).not.toContain("|| true");
    expect(step).not.toContain('grep -q "test result"');
  });

  it("asserts cleanliness with git status --porcelain, not git diff --exit-code", () => {
    const ciYml = readCiWorkflow();
    const step = bindingsStep(ciYml);

    expect(step).toContain("git status --porcelain src/bindings/");
    expect(step).toContain("npm run generate:bindings");
    expect(step).toMatch(/exit 1/);
    expect(ciYml).not.toContain("git diff --exit-code");
  });

  it("check:bindings in package.json asserts emptiness of git status --porcelain", () => {
    const pkg = JSON.parse(readPackageJson());
    const script = pkg.scripts["check:bindings"];

    expect(script).toContain("git status --porcelain src/bindings/");
    expect(script).not.toContain("git diff --exit-code");
    expect(script).toContain("&&");
    expect(script).toContain("generate:bindings");
  });
});
