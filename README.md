# Keynobi

[![CI](https://github.com/thiagodmont/keynobi/actions/workflows/ci.yml/badge.svg)](https://github.com/thiagodmont/keynobi/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A focused **Android development companion** for macOS. It sits **next to** Android Studio so you get readable Gradle output, live logcat, devices/AVDs, and health checks in one native window. With a **MCP** that lets tools like Claude Code run builds, read logs, pull errors, and inspect devices without clicking through the UI.

**Platform:** macOS only (v0.x beta) · **Projects:** Kotlin + Gradle

Download the application on the release build and let us know if it helps in your workflow!

---

## Table of contents

- [Why Keynobi](#why-keynobi)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Features](#features)
- [How it works](#how-it-works)
- [Setup for contributing](#setup-for-contributing)
- [Quick start](#quick-start)
- [Development](#development)
- [Contributing](#contributing)
- [Troubleshooting](#troubleshooting)

---

## Why Keynobi

Keynobi is a **single place** to watch builds, tail logcat with filters, manage emulators, and sanity-check your toolchain. If you use AI agents, the MCP server exposes the same capabilities so the agent can act on real device and build state instead of guessing.

---

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+P` | Command palette |
| `Cmd+Shift+W` | Setup wizard |
| `Cmd+,` | Settings |
| `Cmd+O` | Open / add project folder |
| `Cmd+R` | Run App (build → install → launch) |
| `Cmd+Shift+R` | Build only (no deploy) |
| `Cmd+Shift+V` | Select build variant |
| `Cmd+1` | Build tab |
| `Cmd+2` | Logcat tab |
| `Cmd+3` | Toggle Devices sidebar |
| `Cmd+B` | Toggle Projects sidebar |
| `Cmd+Shift+H` | Health Center |
| `Cmd+Shift+M` | MCP activity panel |

Use the command palette for **Cancel Build**, **Clean Project**, **Copy MCP Setup Command**, and other actions without default shortcuts.

---

## Features

| Area | What you get |
|------|----------------|
| **Projects** | Multi-project registry, Gradle root detection, app `versionName` / `versionCode` editor |
| **Builds** | Streaming log, structured errors, variant matrix, clean/cancel, one-click run to device |
| **Logcat** | Live stream, filters, crash detection, large-session-safe buffering (see architecture) |
| **Devices & AVDs** | Connected devices, emulator lifecycle (create / wipe / delete) |
| **Health** | Java, Android SDK, ADB, Gradle, disk checks with actionable hints |
| **Shell** | Command palette (`Cmd+Shift+P`) backed by a single action registry |
| **MCP** | Claude Code / `keynobi` transport (`--mcp`) for agent-driven workflows |

---

## How it works

Typical loop:

1. **Open a Gradle project** (or add several to the registry). Keynobi finds the Gradle root and remembers it per project.
2. **Build** — stream `./gradlew` output with ANSI coloring, parse errors into a list you can jump from, pick variants (build types × flavors), and run **Build → Install → Launch** when you want a tight loop without leaving the app.
3. **Observe** — **Logcat** streams through Rust (ring buffer + batched events) so the UI stays fast; filter by level, tag, or free text and watch for crashes.
4. **Devices** — see USB devices and AVDs, create/wipe/delete emulators, and keep **Health** (Java, SDK, ADB, Gradle, disk) honest.
5. **Agents (optional)** — start the MCP server so external clients can invoke the same operations over stdio.

---

## Setup for contributing 
This part here is only if you want to run it locally or contribute with development. You can download the and install the last build in the release section.

### 1. Rust (stable toolchain)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version  # rustc 1.78.0 or newer
```

### 2. Node.js 20+

[Volta](https://volta.sh) pins the version automatically:

```bash
curl https://get.volta.sh | bash
volta install node@22
node --version  # v22.x.x
```

Or install directly from [nodejs.org](https://nodejs.org).

### 3. Xcode Command Line Tools

```bash
xcode-select --install
```

### 4. Tauri CLI (cargo plugin)

```bash
cargo install tauri-cli
cargo tauri --version  # tauri-cli 2.x.x
```

---

## Quick start

```bash
git clone https://github.com/thiagodmont/keynobi.git
cd keynobi
npm install
npm run tauri dev
```

`npm run tauri dev` runs Vite on `http://localhost:1420`, compiles the Rust backend when needed, and opens a native window.

> **First run:** initial Rust dependency build often takes **3–8 minutes**; later runs are usually seconds.

---

## Development

Day to day:

```bash
npm run tauri dev        # app + hot reload (TS/CSS); Rust rebuilds on .rs changes
npm run lint
npm run typescript:check
npm run test
cd src-tauri && cargo test && cargo clippy -- -D warnings
npm run generate:bindings   # after Rust model / TS export changes
```

**Contributors:** full checklist, CI parity, and review expectations are in [CONTRIBUTING.md](CONTRIBUTING.md) (and [AGENTS.md](AGENTS.md) for the end-to-end feature checklist).

---

**Design choices (short):**

- **Ring buffer (50K)** for logcat in Rust; the UI only receives what it needs — bounded memory.
- **~100 ms batching** of log events before crossing to the frontend to avoid signal storms.
- **Atomic settings writes** (temp + rename) so crashes mid-save do not corrupt JSON.
- **Mutex discipline** — no lock held across `await`; see `docs/CODE_PATTERN.md`.
- **`ts-rs`** — regenerate TypeScript with `npm run generate:bindings` after model changes.

---

## Contributing

We welcome issues and pull requests. Please read [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow, [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for community expectations, and [SECURITY.md](SECURITY.md) for reporting vulnerabilities. [AGENTS.md](AGENTS.md) is the maintainer-oriented checklist (also useful for advanced contributions).

