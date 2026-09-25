<p align="center">
  <img src="src-tauri/icons/128x128.png" width="96" alt="Keynobi icon">
</p>

<h1 align="center">Keynobi</h1>

<p align="center">
  <strong>Build, run, and debug Android apps from one fast macOS window, and let your AI agent do the same.</strong>
</p>

<p align="center">
  <a href="https://github.com/thiagodmont/keynobi/releases/latest"><strong>Download for macOS</strong></a> ·
  <a href="references/USER_MANUAL.md">User guide</a> ·
  <a href="references/MCP_SERVER.md">MCP reference</a>
</p>

<p align="center">
  <a href="https://github.com/thiagodmont/keynobi/actions/workflows/ci.yml"><img src="https://github.com/thiagodmont/keynobi/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
</p>

---

## What is Keynobi?

Keynobi is a companion app for Android developers on macOS. Keep writing code in Android Studio (or your editor); use Keynobi for the loop around it: **build → install → run → read logs → fix**.

It puts Gradle builds, logcat, devices, and the UI hierarchy in one lightweight window. It also includes an **MCP server**, so AI coding agents such as Claude Code and Codex can build your app, read its crashes, and drive the device using real data instead of guesses.

## What You Can Do

- **Build and run in one keystroke.** `Cmd+R` builds the selected variant, installs it, and launches it on your device. Errors are parsed into a clickable Problems list.
- **Read logcat that keeps up.** A live stream with a real query language (`level:error tag:OkHttp -package:com.google`), saved filters, crash grouping, JSON viewer, and jump-to-source in Android Studio.
- **Inspect any screen.** Capture the UI hierarchy (including Jetpack Compose semantics) from any running app, search it, and see the wireframe.
- **Manage devices and emulators.** See connected devices, and create, launch, wipe, or delete emulators.
- **Check your setup.** Health Center verifies the Android SDK, ADB, emulator, JDK, and Android Studio CLI, and tells you what to fix.

## Works With Your AI Agent

Connect Keynobi to your agent once, then ask for things like:

> "Build the debug variant and fix any compile errors."
>
> "The app crashes on launch. Find the crash in logcat and tell me the cause."
>
> "Open the login screen, type a wrong password, and check which error message appears."

Your agent gets 56 tools for builds, logcat, crashes, devices, and UI automation. Set it up from **Health Center**, which copies the exact command for your install, or run:

```bash
# Claude Code
claude mcp add --scope user --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp

# Codex
codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

The agent starts a small Keynobi process in the background. When Keynobi is open on the same project, the agent works through the app: its builds appear in the Build tab and it shares the app's devices and logcat. Otherwise it runs on its own, so the window does not need to be open. See [AI Client MCP](references/USER_MANUAL.md#ai-client-mcp) for details.

## Get Started

**You need:** macOS 12 or later (Apple Silicon or Intel), the Android SDK with platform-tools, JDK 17 or newer, and an Android project with a Gradle wrapper (`gradlew`).

1. [Download the latest release](https://github.com/thiagodmont/keynobi/releases/latest): the `arm64` DMG for Apple Silicon, or `intel` for Intel Macs.
2. Drag **Keynobi** to **Applications** and open it. The setup wizard auto-detects your SDK and JDK, or lets you pick them.
3. Press `Cmd+O` and choose your Android project folder.
4. Connect a device or start an emulator, then press `Cmd+R`.

Press `Cmd+Shift+P` to see every command. The [User guide](references/USER_MANUAL.md) covers each feature, all shortcuts, and troubleshooting.

## Status and Privacy

Keynobi is in **beta** (0.x) and runs on **macOS only**. It works with Gradle-based Android projects.

- Crash reporting is **off by default**. When on, reports contain only the error type, versions, and code locations: never error messages, your code, logs, or project files.
- The only other network request is a check for new releases on GitHub at startup.
- Everything else stays on your Mac. See [Privacy and Data](references/USER_MANUAL.md#privacy-and-data).

Found a bug or have an idea? [Open an issue](https://github.com/thiagodmont/keynobi/issues).

## Build From Source

Install Xcode Command Line Tools (`xcode-select --install`), [Rust](https://rustup.rs) (the pinned toolchain installs automatically), and Node.js 22. Then:

```bash
git clone https://github.com/thiagodmont/keynobi.git
cd keynobi
npm ci
npm run tauri dev
```

The first build compiles the Rust dependencies and takes a few minutes. [CONTRIBUTING.md](CONTRIBUTING.md) lists the checks to run before a PR.

## Contributing

Issues and pull requests are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md). Engineering rules live in [`references/`](references/), and [AGENTS.md](AGENTS.md) has the rules for AI coding agents. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md), and report security issues privately as described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
