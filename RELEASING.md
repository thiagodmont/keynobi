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
- Code-sign and notarize Apple Silicon + Intel DMGs in parallel (requires repository secrets below)
- Create git tag `v0.1.1`
- Publish a GitHub Release with both DMGs attached

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

## Apple code signing and notarization (CI)

Release DMGs are **Developer ID**–signed and **notarized** via [App Store Connect API keys](https://appstoreconnect.apple.com/access/integrations/api), as described in the [Tauri 2 macOS signing guide](https://v2.tauri.app/distribute/sign/macos/).

Configure these secrets in GitHub under **Settings → Secrets and variables → Actions**, in the **`keynobi_app`** environment (the release workflow’s `build` job uses `environment: keynobi_app` so environment-scoped secrets are available). You can use the same names as **environment secrets** (or repository secrets; both are visible to that job). A missing required value fails the release build with a clear error.

| Secret | Purpose |
|--------|---------|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` export of your **Developer ID Application** certificate (see Tauri doc: `openssl base64 -A -in certificate.p12`). |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12`. |
| `KEYCHAIN_PASSWORD` | Any strong random string; used only on the runner to create/unlock an ephemeral keychain (not your Apple ID). |
| `APPLE_API_ISSUER` | App Store Connect **Issuer ID** (UUID above the API keys table). |
| `APPLE_API_KEY` | App Store Connect **Key ID** (10-character id from the key row). This is **not** the `.p8` file contents. |
| `APPLE_API_PRIVATE_KEY` | Full text of the downloaded `AuthKey_*.p8` file (`-----BEGIN PRIVATE KEY-----` … `-----END PRIVATE KEY-----`). Multiline paste is supported. |
| `APPLE_API_PRIVATE_KEY_BASE64` | **Optional alternative** to `APPLE_API_PRIVATE_KEY`: single-line base64 of the `.p8` file (`openssl base64 -A -in AuthKey_XXX.p8`). Use one or the other, not both. |

Create the API key in App Store Connect with **Developer** access (per Tauri). The workflow writes the private key to `$RUNNER_TEMP` and sets `APPLE_API_KEY_PATH` for the Tauri build; you do not configure `APPLE_API_KEY_PATH` as a secret.

### Troubleshooting “missing or empty” signing secrets

GitHub **does not** clear secret values before your script runs; masking only hides them in **logs**. If the workflow reports a secret as empty, the runner truly did not receive a value. Common causes:

- **Wrong place:** Secrets must be under **Settings → Secrets and variables → Actions** for this repository (not Dependabot-only, not Codespaces-only, unless you use those runners).
- **Wrong name:** Names are case-sensitive (`APPLE_CERTIFICATE`, not `Apple_Certificate`).
- **Organization secret:** If the secret lives at the **org** level, this repo must be **allowed** in the org secret’s repository list.
- **Environment secrets:** The release **`build`** job uses **`environment: keynobi_app`**. Signing and notarization secrets must exist on that environment (or as repository secrets, which are still available). If you use a different environment name, update `.github/workflows/release.yml` to match.

---

## Notes

- **Only version bumps trigger a release.** Pushing doc updates, lint fixes, or
  CI changes to `main` does not create a release.
- **Release builds require Apple signing secrets.** Local `scripts/build-dmg.sh` may still use ad-hoc signing for developer machines; CI release builds do not.
- **If the workflow fails:** fix the underlying issue, then re-run the failed
  job from the GitHub Actions UI (Actions → Release → Re-run failed jobs).
  Do not push a new version bump just to re-trigger — that creates a duplicate
  release. Use manual re-run instead.
