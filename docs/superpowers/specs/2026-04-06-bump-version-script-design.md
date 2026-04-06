# Bump Version Script Design

**Date:** 2026-04-06  
**Status:** Approved  
**Scope:** A single CLI script that auto-increments the project version and syncs all three version files

---

## Overview

Replace the manual "edit `package.json`" step in the release flow with a script that takes a bump type (`patch`, `minor`, `major`) and does everything automatically: increments the version, syncs all files, and prints the before/after.

---

## Script: `scripts/bump-version.mjs`

### Usage

```bash
npm run version:bump             # patch (default): 0.1.0 → 0.1.1
npm run version:bump -- minor    # minor:           0.1.0 → 0.2.0
npm run version:bump -- major    # major:           0.1.0 → 1.0.0
```

### Behaviour

1. Read current version from `package.json` (single source of truth)
2. Parse semver — split on `.` into `[major, minor, patch]` as integers
3. Increment the requested part; reset lower parts to `0`:
   - `patch`: `[M, N, P]` → `[M, N, P+1]`
   - `minor`: `[M, N, P]` → `[M, N+1, 0]`
   - `major`: `[M, N, P]` → `[M+1, 0, 0]`
4. Write new version back to `package.json`
5. Sync to `src-tauri/Cargo.toml` (replace `version = "..."` line) and `src-tauri/tauri.conf.json` (update `"version"` field) — same logic as `scripts/sync-version.mjs`
6. Print: `0.1.0 → 0.1.1`
7. Exit 0 on success

### Error handling

- If argument is provided but not one of `patch`, `minor`, `major`: print error and exit 1
  ```
  Error: unknown bump type "hotfix". Use: patch, minor, or major.
  ```
- If `package.json` version is not valid semver (three dot-separated integers): print error and exit 1
  ```
  Error: could not parse version "x.y" from package.json. Expected semver (e.g. 1.2.3).
  ```

---

## `package.json` change

Add to `"scripts"`:

```json
"version:bump": "node scripts/bump-version.mjs"
```

---

## `RELEASING.md` change

Replace step 2 (`### 2. Create a release commit`) with:

```bash
# a. Bump the version:
npm run version:bump           # patch (most common)
npm run version:bump -- minor  # new features
npm run version:bump -- major  # breaking changes

# b. Update CHANGELOG.md with release notes for this version

# c. Stage and commit
git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: release v0.1.1"
git push origin main
```

The `npm run version:sync` line is removed — `version:bump` already performs the sync.

---

## Out of Scope

- Git staging or committing (developer does this manually after)
- CHANGELOG update (requires human judgement)
- Pre-release tags (e.g. `1.0.0-beta.1`)
- The existing `version:sync` and `version:check` scripts — unchanged
