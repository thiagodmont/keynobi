# Release Artifacts

This folder documents how releases are distributed. Binary DMGs are not committed here; they are attached to each version on GitHub Releases.

## Downloads

All DMG downloads are on the [GitHub Releases page](../../releases).

- **Apple Silicon (M1/M2/M3/M4):** download the `_arm64.dmg`
- **Intel Mac:** download the `_intel.dmg`

GitHub Release DMGs are **code-signed and notarized** in CI (see [RELEASING.md](../RELEASING.md) and the [Tauri 2 macOS signing guide](https://v2.tauri.app/distribute/sign/macos/)). Local builds from source may still be ad-hoc signed unless you configure a Developer ID identity on your machine.
