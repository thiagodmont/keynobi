# Release Automation Design

**Date:** 2026-04-06  
**Status:** Approved  
**Scope:** Automated DMG release pipeline triggered by version bumps on `main`

---

## Overview

Every merge to `main` that includes a `package.json` version bump automatically builds signed-off DMGs for Apple Silicon and Intel, creates a git tag, publishes a GitHub Release, and updates a `release/latest.json` manifest in the repository. No release happens for non-version-bump commits (lint fixes, docs, CI changes, etc.).

---

## Architecture

Three-job GitHub Actions workflow (`.github/workflows/release.yml`) using a matrix for parallel builds.

```
push to main
    │
    ▼
[check] ── detects version change ──► should_release=false → stop
    │
    │ should_release=true
    ▼
[build] matrix: aarch64 | x86_64   (parallel, macos-latest, ~15 min each)
    │
    ▼
[publish] ubuntu-latest (~1 min)
  - create tag + GitHub Release
  - upload both DMGs as release assets
  - commit release/latest.json back to main
```

---

## Job Specifications

### Job 1: `check`

**Runner:** `ubuntu-latest`  
**Trigger:** `push` to `main` only  
**Duration:** ~10 seconds

Steps:
1. Checkout with `fetch-depth: 2` (to access `HEAD~1`)
2. Compare `package.json` `version` field between `HEAD` and `HEAD~1`
3. Output `should_release=true` and `version=X.Y.Z` if version changed; `should_release=false` otherwise

All downstream jobs include `if: needs.check.outputs.should_release == 'true'` and skip on false.

### Job 2: `build`

**Runner:** `macos-latest`  
**Matrix:** `[{ target: aarch64-apple-darwin, arch: arm64 }, { target: x86_64-apple-darwin, arch: intel }]`  
**Duration:** ~12–15 min per arch (parallel)  
**Signing:** Unsigned (`APPLE_SIGNING_IDENTITY="-"`) — consistent with existing `build-dmg.sh`

Steps:
1. Checkout
2. Set up Node 22 + npm cache
3. Set up Rust stable + `Swatinem/rust-cache`
4. `npm ci`
5. `rustup target add ${{ matrix.target }}`
6. `APPLE_SIGNING_IDENTITY="-" npm run tauri build -- --target ${{ matrix.target }} --bundles dmg`
7. Locate DMG: `src-tauri/target/${{ matrix.target }}/release/bundle/dmg/*.dmg`
8. Rename to: `Android-Dev-Companion_${{ needs.check.outputs.version }}_${{ matrix.arch }}.dmg`
9. Upload as GitHub Actions artifact (name: `dmg-${{ matrix.arch }}`)

### Job 3: `publish`

**Runner:** `ubuntu-latest`  
**Needs:** `[check, build]`  
**Duration:** ~1 min

Steps:
1. Checkout (with write token for pushing `latest.json`)
2. Download artifacts `dmg-arm64` and `dmg-intel`
3. Configure git identity (`github-actions[bot]`)
4. Create and push tag `v${{ needs.check.outputs.version }}`
5. Create GitHub Release via `gh release create`:
   - Tag: `v${{ needs.check.outputs.version }}`
   - Title: `v${{ needs.check.outputs.version }}`
   - Auto-generated notes (`--generate-notes`) from commits since last tag
   - Attach both DMGs as assets
6. Build `release/latest.json` with version, tag, date, release URL, and download URLs
7. Commit and push `release/latest.json` directly to `main`

---

## `release/latest.json` Format

```json
{
  "version": "0.1.1",
  "tag": "v0.1.1",
  "releaseDate": "2026-04-06",
  "releaseUrl": "https://github.com/${{ github.repository }}/releases/tag/v0.1.1",
  "downloads": {
    "applesilicon": "https://github.com/${{ github.repository }}/releases/download/v0.1.1/Android-Dev-Companion_0.1.1_arm64.dmg",
    "intel": "https://github.com/${{ github.repository }}/releases/download/v0.1.1/Android-Dev-Companion_0.1.1_intel.dmg"
  }
}
```

This file can also be consumed by the in-app update checker (P-4) as an alternative to direct GitHub API calls — making it faster and not subject to rate limiting.

---

## Repository Structure

```
release/
  latest.json   ← updated by CI on every release (only manifest, no binaries)
  README.md     ← explains the folder; links to GitHub Releases for downloads
```

No binary files are ever committed to the repository.

---

## `RELEASING.md` — Developer Documentation

```markdown
# How to Release

This project releases automatically when the version in `package.json` is bumped on `main`.

## Steps

1. **Finish your work.** Merge all feature branches into `main` before cutting a release.

2. **Create a release commit** (directly on main or via a release PR):

   a. Bump the version in `package.json`:
      - `patch` (0.1.0 → 0.1.1): bug fixes, small improvements
      - `minor` (0.1.0 → 0.2.0): new user-visible features
      - `major` (0.1.0 → 1.0.0): breaking changes or major milestones

   b. Sync all version files:
      ```bash
      npm run version:sync
      ```
      This updates `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`.

   c. Update `CHANGELOG.md` with release notes for this version.

   d. Commit:
      ```bash
      git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json CHANGELOG.md
      git commit -m "chore: release v0.1.1"
      git push origin main
      ```

3. **Wait ~15 minutes.** The release workflow will:
   - Build Apple Silicon and Intel DMGs in parallel
   - Create git tag `v0.1.1`
   - Publish a GitHub Release with both DMGs attached
   - Update `release/latest.json`

4. **Verify** the release at: `https://github.com/${{ github.repository }}/releases`

## Notes

- Only version bumps trigger a release. Pushing other commits to main (docs,
  CI fixes, lint) does not create a release.
- Builds are unsigned. Users will see a Gatekeeper prompt on first launch
  (right-click → Open). This is expected for unsigned macOS apps.
- The `release/latest.json` file is updated automatically. Do not edit it manually.
- If the workflow fails, fix the issue and re-push the same version commit — the
  check job will detect it as unchanged and skip. Instead, push a minimal fix
  commit and then a new version bump, or re-run the failed workflow job manually
  from the GitHub Actions UI.
```

---

## Permissions Required

The workflow needs `contents: write` permission to:
- Push the git tag
- Create the GitHub Release
- Commit `release/latest.json` back to `main`

Add to the workflow:
```yaml
permissions:
  contents: write
```

---

## Out of Scope

- Code signing (Developer ID certificate) — can be added later by storing the certificate as a GitHub secret and removing `APPLE_SIGNING_IDENTITY="-"`
- Windows or Linux builds — macOS only for v0.x
- Notarization — requires Apple Developer account; out of scope for unsigned beta
- Automated `CHANGELOG.md` generation — developer updates it manually as part of the release commit
