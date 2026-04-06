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
