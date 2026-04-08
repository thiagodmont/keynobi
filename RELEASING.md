# How to Release

Releases are automated. When a version bump is pushed to `main`, GitHub Actions
builds Apple Silicon + Intel DMGs and publishes them to GitHub Releases (~15 min).

---

## Steps

### 1. Finish your work

Merge all feature branches into `main` before cutting a release.

### 2. Create a release commit

```bash
# a. Bump the version (patch is the default and most common):
npm run version:bump             # 0.1.0 → 0.1.1  (bug fixes, small improvements)
npm run version:bump -- minor    # 0.1.0 → 0.2.0  (new user-visible features)
npm run version:bump -- major    # 0.1.0 → 1.0.0  (breaking changes or milestones)

# b. Run cargo check
npm run rust:check

# c. Stage and commit
git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
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

## Sentry (optional crash reporting)

Release DMGs are built with the Rust `telemetry` Cargo feature and compile-time `SENTRY_DSN` so the app *can* upload crash reports when the user turns **Anonymous crash reporting** on in Settings. The **frontend** bundle can include a separate browser DSN via **`VITE_SENTRY_DSN`** (Sentry project for the Solid/WebView layer).

- Add repository secrets **`SENTRY_DSN`** (Rust) and **`VITE_SENTRY_DSN`** (browser). These are injected at build time only; they are not committed to the repo.
- Add **`SENTRY_AUTH_TOKEN`** (Organization Settings → Developer Settings → Auth Tokens, scope `project:releases` and `org:read` as needed) so the release workflow can upload **frontend source maps** to the `javascript-solid` project. Without it, the DMG build still succeeds; maps are simply not uploaded.
- If a DSN secret is absent, the build still succeeds, but that layer has no DSN embedded (users can opt in, yet nothing is uploaded for that layer until a build that embeds the DSN).

Local release-like builds:

```bash
SENTRY_DSN='https://…@….ingest.sentry.io/…' \
VITE_SENTRY_DSN='https://…@….ingest.sentry.io/…' \
npm run tauri build -- -f telemetry --bundles dmg
```

### Smoke test (verify the Sentry project receives events)

Use the **Rust** `SENTRY_DSN` when compiling so `option_env!("SENTRY_DSN")` embeds it. Optionally set **`VITE_SENTRY_DSN`** for the browser SDK (can be a separate Sentry project). Turn **Anonymous crash reporting** on in Settings and restart so the native client initializes; the web client gates on the same setting without restart.

```bash
export SENTRY_DSN='https://YOUR_KEY@YOUR_ORG.ingest.sentry.io/YOUR_PROJECT'
export VITE_SENTRY_DSN='https://YOUR_BROWSER_KEY@YOUR_ORG.ingest.sentry.io/YOUR_BROWSER_PROJECT'
npm run tauri dev -- --features telemetry
```

In the dev app, open the **command palette** (**⌘⇧P**) and run **Send test native (Rust) Sentry event** or **Send test web Sentry event** (Debug category). You should see a matching event in each configured Sentry project.

We keep **`send_default_pii: false`** and path scrubbing in code; do not paste a real DSN into the repository.

The WebView **CSP** in `tauri.conf.json` must allow `connect-src` to Sentry ingest hosts (`*.ingest.sentry.io` / `*.ingest.us.sentry.io`); otherwise the browser SDK cannot upload events.

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
