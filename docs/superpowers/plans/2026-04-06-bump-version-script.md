# Bump Version Script Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create `scripts/bump-version.mjs` — a CLI script that increments the project version (patch/minor/major), syncs all three version files, and prints the before/after.

**Architecture:** The script exports a pure `bumpVersion(current, type)` function for testability, and runs CLI logic only when invoked directly (detected via `import.meta.url`). File I/O mirrors the existing `scripts/sync-version.mjs` pattern. Tests use Vitest (already in the project) and import the exported function directly.

**Tech Stack:** Node.js ESM, Vitest, `fs/readFileSync/writeFileSync`, same file-patching approach as `scripts/sync-version.mjs`.

**Spec:** `docs/superpowers/specs/2026-04-06-bump-version-script-design.md`

---

## File Map

**Create:**
- `scripts/bump-version.mjs` — the script; exports `bumpVersion()` for testability
- `scripts/bump-version.test.mjs` — Vitest unit tests for `bumpVersion()`

**Modify:**
- `package.json` — add `"version:bump"` script
- `RELEASING.md` — replace manual version step with `npm run version:bump`

---

## Task 1: Write the `bumpVersion` pure function (TDD)

**Files:**
- Create: `scripts/bump-version.test.mjs`
- Create: `scripts/bump-version.mjs` (exports only — no CLI logic yet)

- [ ] **Step 1: Write the failing tests**

Create `scripts/bump-version.test.mjs`:

```javascript
import { describe, it, expect } from "vitest";
import { bumpVersion } from "./bump-version.mjs";

describe("bumpVersion", () => {
  // ── patch ─────────────────────────────────────────────────────────────────
  it("increments patch by 1", () => {
    expect(bumpVersion("0.1.0", "patch")).toBe("0.1.1");
  });

  it("patch is the default when type is omitted", () => {
    expect(bumpVersion("0.1.0")).toBe("0.1.1");
  });

  it("patch increment resets nothing", () => {
    expect(bumpVersion("1.2.3", "patch")).toBe("1.2.4");
  });

  // ── minor ─────────────────────────────────────────────────────────────────
  it("increments minor by 1 and resets patch to 0", () => {
    expect(bumpVersion("0.1.3", "minor")).toBe("0.2.0");
  });

  it("minor increment preserves major", () => {
    expect(bumpVersion("2.4.7", "minor")).toBe("2.5.0");
  });

  // ── major ─────────────────────────────────────────────────────────────────
  it("increments major by 1 and resets minor and patch to 0", () => {
    expect(bumpVersion("0.1.3", "major")).toBe("1.0.0");
  });

  it("major increment from non-zero minor and patch", () => {
    expect(bumpVersion("3.7.9", "major")).toBe("4.0.0");
  });

  // ── error cases ───────────────────────────────────────────────────────────
  it("throws on unknown bump type", () => {
    expect(() => bumpVersion("0.1.0", "hotfix")).toThrow(
      'unknown bump type "hotfix". Use: patch, minor, or major.'
    );
  });

  it("throws when version is not valid semver", () => {
    expect(() => bumpVersion("1.2", "patch")).toThrow(
      'could not parse version "1.2" from package.json. Expected semver (e.g. 1.2.3).'
    );
  });

  it("throws when version contains non-numeric parts", () => {
    expect(() => bumpVersion("1.x.0", "patch")).toThrow(
      'could not parse version "1.x.0" from package.json. Expected semver (e.g. 1.2.3).'
    );
  });
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
npx vitest run scripts/bump-version.test.mjs 2>&1 | head -20
```

Expected: fail with `Cannot find module './bump-version.mjs'`

- [ ] **Step 3: Create `scripts/bump-version.mjs` — exported function only**

```javascript
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
```

- [ ] **Step 4: Run tests to confirm they pass**

```bash
npx vitest run scripts/bump-version.test.mjs
```

Expected:
```
✓ scripts/bump-version.test.mjs (11)
  ✓ bumpVersion > increments patch by 1
  ✓ bumpVersion > patch is the default when type is omitted
  ...
Test Files  1 passed (1)
Tests       11 passed (11)
```

- [ ] **Step 5: Commit**

```bash
git add scripts/bump-version.mjs scripts/bump-version.test.mjs
git commit -m "feat(scripts): add bumpVersion helper with full test coverage"
```

---

## Task 2: Add CLI entry point, wire up npm script, update RELEASING.md

**Files:**
- Modify: `scripts/bump-version.mjs` — append the CLI `main()` block
- Modify: `package.json` — add `version:bump` script
- Modify: `RELEASING.md` — replace manual version bump step

- [ ] **Step 1: Append the CLI block to `scripts/bump-version.mjs`**

Add this at the bottom of the file, after the exported `bumpVersion` function:

```javascript
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

  // Bump (throws on invalid input — let the error propagate to stderr)
  let next;
  try {
    next = bumpVersion(current, type);
  } catch (err) {
    console.error(`Error: ${err.message}`);
    process.exit(1);
  }

  // Write package.json
  pkg.version = next;
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

  // Sync Cargo.toml + tauri.conf.json
  syncToFiles(next);

  // Print the result
  console.log(`${current} → ${next}`);
}
```

- [ ] **Step 2: Verify the tests still pass after adding the CLI block**

```bash
npx vitest run scripts/bump-version.test.mjs
```

Expected: 11 passed, 0 failed.

- [ ] **Step 3: Smoke test the CLI directly**

```bash
# Check current version without changing it
node -e "const p=require('./package.json'); console.log('current:', p.version)"

# Dry-run to see what patch would produce (don't actually commit this)
node scripts/bump-version.mjs patch
```

Expected: prints something like `0.1.0 → 0.1.1`

Then verify all three files updated:
```bash
node scripts/sync-version.mjs --check
```

Expected: `✓ All versions in sync: 0.1.1`

**Immediately revert** (we're not releasing right now):
```bash
git checkout -- package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json
```

- [ ] **Step 4: Add `version:bump` to `package.json`**

Read `package.json`. In the `"scripts"` section, add after `"version:sync"` and `"version:check"`:

```json
"version:bump": "node scripts/bump-version.mjs"
```

Verify it looks correct:
```bash
node -e "const p=require('./package.json'); console.log(p.scripts['version:bump'])"
```

Expected: `node scripts/bump-version.mjs`

- [ ] **Step 5: Update `RELEASING.md` — replace the manual bump step**

Read `RELEASING.md`. Find the `### 2. Create a release commit` section. Replace it entirely with:

```markdown
### 2. Create a release commit

```bash
# a. Bump the version (patch is the default and most common):
npm run version:bump             # 0.1.0 → 0.1.1  (bug fixes, small improvements)
npm run version:bump -- minor    # 0.1.0 → 0.2.0  (new user-visible features)
npm run version:bump -- major    # 0.1.0 → 1.0.0  (breaking changes or milestones)

# b. Update CHANGELOG.md with release notes for this version

# c. Stage and commit
git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: release v0.1.1"
git push origin main
```
```

- [ ] **Step 6: Run the full test suite to confirm no regressions**

```bash
npm run test 2>&1 | grep -E "Tests|passed|failed"
```

Expected: `Tests  386 passed`

- [ ] **Step 7: Commit**

```bash
git add scripts/bump-version.mjs package.json RELEASING.md
git commit -m "feat(scripts): wire up version:bump npm script and update RELEASING.md"
```

---

## Self-Review

### Spec coverage

| Spec requirement | Task |
|---|---|
| `bumpVersion(current, type)` pure function | Task 1 |
| `patch` default when type omitted | Task 1 |
| Resets lower parts on minor/major | Task 1 |
| Error on unknown type | Task 1 |
| Error on invalid semver | Task 1 |
| Reads from `package.json` | Task 2 |
| Writes back to `package.json` | Task 2 |
| Syncs `Cargo.toml` and `tauri.conf.json` | Task 2 |
| Prints `before → after` | Task 2 |
| Exit 1 on error | Task 2 |
| `"version:bump"` npm script | Task 2 |
| `RELEASING.md` updated | Task 2 |

All spec requirements covered. ✓
