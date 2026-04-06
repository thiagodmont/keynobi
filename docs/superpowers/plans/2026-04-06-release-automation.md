# Release Automation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically build and publish Apple Silicon + Intel DMGs to GitHub Releases whenever a version bump is merged to `main`.

**Architecture:** A single GitHub Actions workflow (`release.yml`) with three chained jobs — `check` detects whether `package.json` version changed, `build` runs a matrix to produce two DMGs in parallel on `macos-latest`, and `publish` creates the GitHub Release and updates a `release/latest.json` manifest committed back to `main`.

**Tech Stack:** GitHub Actions, `gh` CLI (pre-installed on runners), `jq` (pre-installed on ubuntu-latest), `actions/upload-artifact@v4`, `actions/download-artifact@v4`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, Tauri CLI, Node 22.

**Spec:** `docs/superpowers/specs/2026-04-06-release-automation-design.md`

---

## File Map

**Create:**
- `.github/workflows/release.yml` — the complete 3-job release workflow
- `release/latest.json` — initial manifest (empty/placeholder, updated by CI on first release)
- `release/README.md` — explains the folder; no binaries committed here
- `RELEASING.md` — developer guide for cutting a release

**No existing files modified** — the release workflow is fully additive.

---

## Task 1: Create `release/` folder with manifest and README

**Files:**
- Create: `release/latest.json`
- Create: `release/README.md`

- [ ] **Step 1: Create the initial `release/latest.json`**

This is the initial placeholder. CI will overwrite it on every release. The `null` values signal that no release has been published yet.

```json
{
  "version": null,
  "tag": null,
  "releaseDate": null,
  "releaseUrl": null,
  "downloads": {
    "applesilicon": null,
    "intel": null
  }
}
```

- [ ] **Step 2: Create `release/README.md`**

```markdown
# Release Artifacts

This folder contains only a release manifest — no binary files are committed here.

## `latest.json`

Machine-readable metadata for the most recent release. Updated automatically by
the release workflow on every new version.

**Format:**
```json
{
  "version": "0.1.1",
  "tag": "v0.1.1",
  "releaseDate": "2026-04-06",
  "releaseUrl": "https://github.com/owner/repo/releases/tag/v0.1.1",
  "downloads": {
    "applesilicon": "https://github.com/owner/repo/releases/download/v0.1.1/Android-Dev-Companion_0.1.1_arm64.dmg",
    "intel": "https://github.com/owner/repo/releases/download/v0.1.1/Android-Dev-Companion_0.1.1_intel.dmg"
  }
}
```

## Downloads

All DMG downloads are on the [GitHub Releases page](../../releases).

- **Apple Silicon (M1/M2/M3/M4):** download the `_arm64.dmg`
- **Intel Mac:** download the `_intel.dmg`

> **First launch:** The builds are unsigned. Right-click the app → **Open** to bypass Gatekeeper. This is only needed once.
```

- [ ] **Step 3: Commit**

```bash
git add release/latest.json release/README.md
git commit -m "chore: add release/ manifest folder"
```

---

## Task 2: Create `RELEASING.md`

**Files:**
- Create: `RELEASING.md`

- [ ] **Step 1: Create `RELEASING.md` at the project root**

```markdown
# How to Release

Releases are automated. When a version bump is pushed to `main`, GitHub Actions
builds Apple Silicon + Intel DMGs and publishes them to GitHub Releases (~15 min).

---

## Steps

### 1. Finish your work

Merge all feature branches into `main` before cutting a release.

### 2. Create a release commit

```bash
# a. Bump the version in package.json
#    Edit the "version" field manually, then:
npm run version:sync          # syncs Cargo.toml + tauri.conf.json

# b. Update CHANGELOG.md with release notes for this version

# c. Stage and commit
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: release v0.1.1"
git push origin main
```

### 3. Wait for CI (~15 min)

The release workflow will automatically:
- Detect the version bump
- Build Apple Silicon + Intel DMGs in parallel
- Create git tag `v0.1.1`
- Publish a GitHub Release with both DMGs attached
- Update `release/latest.json`

Monitor progress at: **Actions → Release** in the GitHub repository.

### 4. Verify

Check the new release at: `https://github.com/<owner>/<repo>/releases`

---

## Version bump rules (semver)

| Bump | When to use | Example |
|---|---|---|
| `patch` | Bug fixes, small improvements | `0.1.0 → 0.1.1` |
| `minor` | New user-visible features | `0.1.0 → 0.2.0` |
| `major` | Breaking changes or major milestones | `0.1.0 → 1.0.0` |

The most common case is a `patch` bump.

---

## Notes

- **Only version bumps trigger a release.** Pushing doc updates, lint fixes, or
  CI changes to `main` does not create a release.
- **Builds are unsigned.** Users see a Gatekeeper prompt on first launch
  (right-click → Open). This is expected for the v0.x beta.
- **Do not edit `release/latest.json` manually.** It is updated automatically
  by the publish job.
- **If the workflow fails:** fix the underlying issue, then re-run the failed
  job from the GitHub Actions UI (Actions → Release → Re-run failed jobs).
  Do not push a new version bump just to re-trigger — that creates a duplicate
  release. Use manual re-run instead.
```

- [ ] **Step 2: Commit**

```bash
git add RELEASING.md
git commit -m "docs: add RELEASING.md developer guide for cutting releases"
```

---

## Task 3: Create the release workflow — `check` job

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Create the workflow file with just the `check` job**

```yaml
name: Release

on:
  push:
    branches: [main]

# Required to create tags, releases, and push release/latest.json back to main.
permissions:
  contents: write

jobs:
  # ── Job 1: Detect whether package.json version changed ──────────────────────
  check:
    name: Check version bump
    runs-on: ubuntu-latest
    outputs:
      should_release: ${{ steps.check.outputs.should_release }}
      version: ${{ steps.check.outputs.version }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 2   # need HEAD and HEAD~1 to compare versions

      - name: Detect version change
        id: check
        run: |
          CURRENT=$(jq -r '.version' package.json)
          PREVIOUS=$(git show HEAD~1:package.json | jq -r '.version')

          echo "Current version : $CURRENT"
          echo "Previous version: $PREVIOUS"

          if [ "$CURRENT" != "$PREVIOUS" ]; then
            echo "Version bumped: $PREVIOUS → $CURRENT — will release"
            echo "should_release=true"  >> "$GITHUB_OUTPUT"
            echo "version=$CURRENT"     >> "$GITHUB_OUTPUT"
          else
            echo "No version change — skipping release"
            echo "should_release=false" >> "$GITHUB_OUTPUT"
          fi
```

- [ ] **Step 2: Validate the YAML is syntactically correct**

```bash
# Quick syntax check using Node (no extra tools needed)
node -e "require('js-yaml')" 2>/dev/null || npm install -g js-yaml 2>/dev/null
python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML valid"
```

Expected: `YAML valid`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): add check job — detects version bump on main"
```

---

## Task 4: Add the `build` job (matrix DMG builds)

**Files:**
- Modify: `.github/workflows/release.yml` — append the `build` job

- [ ] **Step 1: Append the `build` job to `release.yml`**

Add this after the `check` job (inside the same `jobs:` block):

```yaml
  # ── Job 2: Build DMGs in parallel for each architecture ─────────────────────
  build:
    name: Build DMG (${{ matrix.arch }})
    needs: check
    if: needs.check.outputs.should_release == 'true'
    runs-on: macos-latest
    strategy:
      matrix:
        include:
          - target: aarch64-apple-darwin
            arch: arm64
          - target: x86_64-apple-darwin
            arch: intel

    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: "npm"

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
          key: ${{ matrix.target }}

      - name: Install npm dependencies
        run: npm ci

      - name: Install Rust target
        run: rustup target add ${{ matrix.target }}

      - name: Build DMG
        env:
          # Unsigned build — consistent with scripts/build-dmg.sh.
          # Users bypass Gatekeeper once on first launch (right-click → Open).
          APPLE_SIGNING_IDENTITY: "-"
        run: |
          npm run tauri build -- --target ${{ matrix.target }} --bundles dmg

      - name: Locate and rename DMG
        id: dmg
        run: |
          VERSION=${{ needs.check.outputs.version }}
          DMG=$(find "src-tauri/target/${{ matrix.target }}/release/bundle/dmg" -name "*.dmg" | head -1)
          if [ -z "$DMG" ]; then
            echo "ERROR: No DMG found after build" >&2
            exit 1
          fi
          DEST="Android-Dev-Companion_${VERSION}_${{ matrix.arch }}.dmg"
          mv "$DMG" "$DEST"
          echo "path=$DEST" >> "$GITHUB_OUTPUT"
          echo "Built: $DEST"

      - name: Upload DMG artifact
        uses: actions/upload-artifact@v4
        with:
          name: dmg-${{ matrix.arch }}
          path: ${{ steps.dmg.outputs.path }}
          retention-days: 1   # artifacts are only needed by the publish job
```

- [ ] **Step 2: Verify YAML is still valid**

```bash
python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML valid"
```

Expected: `YAML valid`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): add build job — matrix DMG builds for arm64 and intel"
```

---

## Task 5: Add the `publish` job (GitHub Release + manifest update)

**Files:**
- Modify: `.github/workflows/release.yml` — append the `publish` job

- [ ] **Step 1: Append the `publish` job to `release.yml`**

Add this after the `build` job (inside the same `jobs:` block):

```yaml
  # ── Job 3: Create GitHub Release and update release/latest.json ─────────────
  publish:
    name: Publish release
    needs: [check, build]
    if: needs.check.outputs.should_release == 'true'
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4
        with:
          # Use the default GITHUB_TOKEN — permissions: contents: write is set
          # at the workflow level so it can push tags and commits.
          token: ${{ secrets.GITHUB_TOKEN }}

      - name: Download Apple Silicon DMG
        uses: actions/download-artifact@v4
        with:
          name: dmg-arm64

      - name: Download Intel DMG
        uses: actions/download-artifact@v4
        with:
          name: dmg-intel

      - name: Configure git identity
        run: |
          git config user.name  "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

      - name: Create and push tag
        run: |
          VERSION=${{ needs.check.outputs.version }}
          git tag "v${VERSION}"
          git push origin "v${VERSION}"

      - name: Create GitHub Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          VERSION=${{ needs.check.outputs.version }}
          gh release create "v${VERSION}" \
            --title "v${VERSION}" \
            --generate-notes \
            "Android-Dev-Companion_${VERSION}_arm64.dmg" \
            "Android-Dev-Companion_${VERSION}_intel.dmg"

      - name: Update release/latest.json
        run: |
          VERSION=${{ needs.check.outputs.version }}
          REPO=${{ github.repository }}
          DATE=$(date -u +%Y-%m-%d)

          cat > release/latest.json << EOF
          {
            "version": "${VERSION}",
            "tag": "v${VERSION}",
            "releaseDate": "${DATE}",
            "releaseUrl": "https://github.com/${REPO}/releases/tag/v${VERSION}",
            "downloads": {
              "applesilicon": "https://github.com/${REPO}/releases/download/v${VERSION}/Android-Dev-Companion_${VERSION}_arm64.dmg",
              "intel": "https://github.com/${REPO}/releases/download/v${VERSION}/Android-Dev-Companion_${VERSION}_intel.dmg"
            }
          }
          EOF

          git add release/latest.json
          git commit -m "chore: update release/latest.json to v${VERSION} [skip ci]"
          git push origin main
```

The `[skip ci]` suffix in the commit message prevents the CI and release workflows from re-triggering on the manifest update commit.

- [ ] **Step 2: Verify the complete YAML**

```bash
python3 -c "import yaml, sys; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML valid"
```

Expected: `YAML valid`

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): add publish job — creates GitHub Release and updates latest.json"
```

---

## Task 6: End-to-end validation

This task verifies the complete workflow works without pushing a real release.

- [ ] **Step 1: Verify all four files are in place**

```bash
ls -la .github/workflows/release.yml release/latest.json release/README.md RELEASING.md
```

Expected: all four files exist.

- [ ] **Step 2: Verify the complete workflow YAML structure**

```bash
python3 -c "
import yaml
w = yaml.safe_load(open('.github/workflows/release.yml'))
jobs = list(w['jobs'].keys())
print('Jobs found:', jobs)
assert 'check' in jobs, 'missing check job'
assert 'build' in jobs, 'missing build job'
assert 'publish' in jobs, 'missing publish job'
assert w['jobs']['build']['needs'] == 'check', 'build must need check'
assert set(w['jobs']['publish']['needs']) == {'check', 'build'}, 'publish must need both'
print('Structure OK')
"
```

Expected:
```
Jobs found: ['check', 'build', 'publish']
Structure OK
```

- [ ] **Step 3: Dry-run the version detection logic locally**

```bash
# Simulate what the check job does, without GitHub Actions
CURRENT=$(jq -r '.version' package.json)
echo "Current version: $CURRENT"
# Verify jq works on the previous commit (requires at least 1 prior commit)
PREVIOUS=$(git show HEAD~1:package.json 2>/dev/null | jq -r '.version' || echo "no-previous")
echo "Previous version: $PREVIOUS"
if [ "$CURRENT" != "$PREVIOUS" ]; then
  echo "Would trigger release for v$CURRENT"
else
  echo "Would skip release (no version change)"
fi
```

Expected: Either "Would skip release" (normal state) or "Would trigger release" (if last commit was a version bump).

- [ ] **Step 4: Verify release/latest.json is valid JSON**

```bash
jq . release/latest.json
```

Expected: pretty-prints the JSON with `null` values (initial placeholder).

- [ ] **Step 5: Push and confirm CI workflow does NOT trigger a release**

```bash
git push origin main
```

Then check GitHub Actions → the `Release` workflow should run, and the `check` job should output `should_release=false` since the last commit was not a version bump. The `build` and `publish` jobs should be skipped.

Go to: `https://github.com/<owner>/<repo>/actions` and verify `check` passes and `build`/`publish` show as skipped.

- [ ] **Step 6: Final commit (if any files were modified during validation)**

```bash
git status  # should be clean
```

---

## Task 7: Test a real release (first release trigger)

This task validates the full pipeline with an actual version bump.

- [ ] **Step 1: Bump the version**

```bash
# Edit package.json: change "version": "0.1.0" to "version": "0.1.1"
# (or whatever the next appropriate version is)

npm run version:sync
```

Expected output: `Synced version 0.1.1 to Cargo.toml and tauri.conf.json`

- [ ] **Step 2: Verify all three files are in sync**

```bash
node scripts/sync-version.mjs --check
```

Expected: `✓ All versions in sync: 0.1.1`

- [ ] **Step 3: Update CHANGELOG.md**

Add an entry at the top of the `[Unreleased]` section or create a new `[0.1.1]` section:

```markdown
## [0.1.1] — 2026-04-06

### Fixed
- Production readiness hardening (see full list in commit history)

### Added  
- Automated release pipeline (this release was built automatically by CI)
```

- [ ] **Step 4: Commit and push**

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore: release v0.1.1"
git push origin main
```

- [ ] **Step 5: Monitor the workflow**

Navigate to `https://github.com/<owner>/<repo>/actions` and watch:
- `check` job: should output `should_release=true`, `version=0.1.1`
- `build` job (arm64): compiles Rust + Vite, produces `Android-Dev-Companion_0.1.1_arm64.dmg`
- `build` job (intel): compiles Rust + Vite, produces `Android-Dev-Companion_0.1.1_intel.dmg`
- `publish` job: creates tag `v0.1.1`, creates GitHub Release, updates `release/latest.json`

Total time: ~12–18 minutes.

- [ ] **Step 6: Verify the release**

```bash
# Check the tag was created
git fetch --tags
git tag -l "v*"
# Expected: v0.1.1

# Check release/latest.json was updated by CI
git pull origin main
jq . release/latest.json
```

Expected `latest.json`:
```json
{
  "version": "0.1.1",
  "tag": "v0.1.1",
  "releaseDate": "2026-04-06",
  "releaseUrl": "https://github.com/<owner>/<repo>/releases/tag/v0.1.1",
  "downloads": {
    "applesilicon": "https://github.com/<owner>/<repo>/releases/download/v0.1.1/Android-Dev-Companion_0.1.1_arm64.dmg",
    "intel": "https://github.com/<owner>/<repo>/releases/download/v0.1.1/Android-Dev-Companion_0.1.1_intel.dmg"
  }
}
```

Also verify on GitHub: `https://github.com/<owner>/<repo>/releases` — two DMG assets attached to `v0.1.1`.

---

## Self-Review

### Spec coverage

| Spec requirement | Task |
|---|---|
| 3-job workflow (check, build, publish) | Tasks 3–5 |
| Version change detection via `fetch-depth: 2` + `jq` | Task 3 |
| Matrix build: arm64 + intel in parallel | Task 4 |
| Unsigned builds (`APPLE_SIGNING_IDENTITY="-"`) | Task 4 |
| Predictable DMG filename format | Task 4 |
| Artifact upload/download between jobs | Tasks 4–5 |
| `permissions: contents: write` | Task 3 |
| Create and push git tag | Task 5 |
| `gh release create --generate-notes` | Task 5 |
| Upload both DMGs as release assets | Task 5 |
| Update `release/latest.json` | Task 5 |
| `[skip ci]` to prevent loop on manifest commit | Task 5 |
| `release/latest.json` initial placeholder | Task 1 |
| `release/README.md` | Task 1 |
| `RELEASING.md` developer guide | Task 2 |
| End-to-end validation steps | Tasks 6–7 |

All spec requirements covered. ✓
