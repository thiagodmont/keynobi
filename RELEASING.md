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
git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json CHANGELOG.md
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
