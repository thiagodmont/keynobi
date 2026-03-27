# Android Dev Companion — User Manual

> A focused Android development companion for build logs, logcat, and device management.  
> Works alongside Android Studio and Claude Code.

---

## Table of Contents

1. [Getting Started](#1-getting-started)
2. [Layout Overview](#2-layout-overview)
3. [Build Panel](#3-build-panel)
4. [Logcat Panel](#4-logcat-panel)
5. [Devices Panel](#5-devices-panel)
6. [Status Bar](#6-status-bar)
7. [Settings](#7-settings)
8. [Health Center](#8-health-center)
9. [Keyboard Shortcuts](#9-keyboard-shortcuts)
10. [Claude Code Integration](#10-claude-code-integration)

---

## 1. Getting Started

### Requirements
- macOS (Apple Silicon or Intel)
- Android SDK with platform-tools (for ADB)
- Java 11+ (for Gradle builds)
- Android project with a `gradlew` wrapper

### First Launch

1. Launch **Android Dev Companion**
2. Press **Cmd+O** or click the title bar to open the project switcher and select your Android project folder
3. The app detects your Gradle root, saves the project to the registry, and initializes build variants
4. Open **Settings** (Cmd+,) and configure:
   - **Android SDK Path** — path to `$ANDROID_HOME` (e.g. `~/Library/Android/sdk`)
   - **Java Home** — path to JDK (e.g. `/Library/Java/JavaVirtualMachines/jdk-17.jdk/Contents/Home`)

### Subsequent Launches

The app automatically restores the last-active project. No need to re-open the folder each time.

---

## 2. Layout Overview

```
┌──────────────────────────────────────────────────────┐
│  TitleBar (project name)                              │
├──[Build]──[Logcat]──[Devices]─────────────────────────│
│                                                       │
│  Active Panel Content                                 │
│  (fills all available space)                          │
│                                                       │
├──────────────────────────────────────────────────────┤
│  StatusBar: Health | Build Status | Variant | Device  │
└──────────────────────────────────────────────────────┘
```

Three main panels — always accessible, no toggling required:
- **Build** — Gradle build output and error list
- **Logcat** — Android device log streaming
- **Devices** — Connected devices and emulator management

---

## 2a. Project Switcher

The **title bar** doubles as a project switcher. Click the project name (or "Android IDE" when no project is open) to open the dropdown.

```
┌─────────────────────────────────────────────────────────────┐
│  ●  ○  ○   Android IDE — MyApp ▾                             │
└─────────────────────────────────────────────────────────────┘
                │
                ▼
  ┌──────────────────────────────┐
  │ ★ MyApp          ~/code/...  │  ← pinned / active (highlighted)
  │   OtherProject   ~/dev/...   │
  │   ThirdProject   ~/work/...  │
  │ ─────────────────────────── │
  │   Open Folder…  Cmd+O        │
  └──────────────────────────────┘
```

### Actions
- **Click a project** — switch to it (cancels any running build, stops logcat, opens the new project)
- **★ star icon** — pin/unpin a project (pinned entries stay at the top)
- **× button** (appears on hover for non-active projects) — remove from the list (does NOT delete from disk)
- **Open Folder…** — browse for a new project folder

### Project App Info

Access via **Command Palette → "Project App Info"** to open a lightweight editor for:
- **Version Name** (e.g. `1.0.0`) — edits `versionName` in `app/build.gradle(.kts)`
- **Version Code** (integer) — edits `versionCode` in `app/build.gradle(.kts)`
- **Application ID** — read-only display of `applicationId`

Changes are written directly to the Gradle file using a regex replace (safe for standard DSL; does not reformat the file).

---

## 3. Build Panel

The Build panel streams Gradle output in real time.

### Running a Build

- **Cmd+R** — Build and deploy (assemble → install → launch app)
- **Cmd+Shift+R** — Build only (no deploy)
- Click **Run** in the Build panel toolbar
- The active device and build variant are used automatically

### Build Panel Views

- **Log tab** — Raw streaming Gradle output with ANSI colors
- **Problems tab** — Structured list of errors and warnings with file paths

### Build Variants

- Click the variant pill in the status bar (e.g. `debug`) to open the variant picker
- **Cmd+Shift+V** — Open variant picker from keyboard
- Variants are auto-discovered from `app/build.gradle.kts`

### Clean

Use the command palette (Cmd+Shift+P) → **Clean Project** to run `./gradlew clean`.

---

## 4. Logcat Panel

The Logcat panel streams Android device log output in real time.

### Starting Logcat

1. Connect an Android device or start an emulator (see Devices panel)
2. Click **Start** in the Logcat toolbar, or the app will auto-connect to the selected device
3. Log entries appear immediately as they arrive

### Log Levels

Entries are color-coded by log level:

| Level | Color | Meaning |
|-------|-------|---------|
| V (Verbose) | Gray | Most verbose, all messages |
| D (Debug) | Blue | Debug messages |
| I (Info) | Green | Informational |
| W (Warning) | Yellow | Potential issues |
| E (Error) | Red | Error conditions |
| F (Fatal) | Purple | Fatal errors / crashes |

### Filtering

- **Level filter** — Dropdown to show only entries at or above a minimum level
- **Tag filter** — Type to filter by tag name (case-insensitive substring)
- **Text filter** — Type to search in message text and tag

### Controls

- **Start / Stop** — Begin or end logcat streaming
- **Pause / Resume** — Pause new entries (no data is lost, buffer continues)
- **Clear** — Clear the display buffer and the in-memory ring buffer

### Ring Buffer

The logcat ring buffer holds up to **50,000 entries** in memory. The oldest entries are evicted when the buffer is full. All entries since starting are kept until you click Clear.

### Crash Detection

When a `FATAL EXCEPTION` or `AndroidRuntime` crash is detected, the entry is highlighted in red regardless of other filters.

---

## 5. Devices Panel

The Devices panel shows connected physical devices and available emulators.

### Physical Devices

- Devices connected via USB appear automatically (polled every 2 seconds)
- The device serial, model name, API level, and connection state are shown
- Click a device row to select it as the active deployment target

### Emulators (AVDs)

- Available AVDs from `~/.android/avd/` are listed
- **Launch** — Start an emulator from its AVD name
- **Stop** — Terminate a running emulator
- Emulators appear in both the AVD list and the physical device list once running

### Device Selection

The selected device is shown in the status bar. Builds use this device for install and launch operations.

---

## 6. Status Bar

The status bar at the bottom shows:

| Item | Description |
|------|-------------|
| ⚙ | Settings gear — click to open settings |
| Project name | Name of the open Android project |
| Health | IDE health indicator — click to open Health Center |
| Build status | Last build result — click to switch to Build panel |
| Variant pill | Active build variant — click to change |
| Device pill | Selected device — click to open device selector |

---

## 7. Settings

Open Settings with **Cmd+,** or the gear icon in the status bar.

### Android SDK

Set the path to your Android SDK installation. This is required for:
- ADB device communication
- Emulator launching
- Health checks

Use **Auto-detect** to find the SDK from your shell environment.

### Java / JDK

Set the path to your JDK installation. Required for Gradle builds.

Use **Auto-detect** to find Java from your shell environment.

### Build Settings (persisted automatically)

- **Last-used variant** — restored on next launch
- **Last-used device serial** — restored on next launch
- **Gradle JVM args** — passed to the Gradle daemon
- **Parallel builds** — `--parallel` flag
- **Offline mode** — `--offline` flag

---

## 8. Health Center

Open the Health Center with **Cmd+Shift+H** or by clicking the Health indicator in the status bar.

The Health Center checks:

| Check | What it verifies |
|-------|-----------------|
| Android SDK | Path exists and contains `platforms/` or `platform-tools/` |
| ADB | Found in `$ANDROID_HOME/platform-tools/` or PATH |
| Emulator | Found in `$ANDROID_HOME/emulator/` |
| Java / JDK | `java -version` exits successfully |
| Gradle Wrapper | `gradlew` exists at the project root |
| Disk Space | Free space in `~/.androidide/` |
| App Directory | `~/.androidide/` is writable |

Click **Refresh** to re-run all checks.

---

## 9. Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Cmd+O** | Open Android project folder |
| **Cmd+R** | Run App (build → install → launch) |
| **Cmd+Shift+R** | Build Only (no deploy) |
| **Cmd+Shift+V** | Select Build Variant |
| **Cmd+1** | Switch to Build panel |
| **Cmd+2** | Switch to Logcat panel |
| **Cmd+3** | Switch to Devices panel |
| **Cmd+,** | Open Settings |
| **Cmd+Shift+H** | Open Health Center |
| **Cmd+Shift+P** | Command Palette |

**Command Palette actions** (Cmd+Shift+P, then type):
- `Project App Info` — open the version name/code editor
- `Open Folder` — browse for a new project

---

## 10. Claude Code Integration

Android Dev Companion exposes an **MCP server** that Claude Code can connect to. This lets Claude Code:
- Trigger builds and read errors
- Read logcat entries and crash logs
- Manage devices and install APKs
- Query build variants

### Setup (coming in Phase 4)

```bash
claude mcp add android-companion --command "/path/to/android-dev-companion --mcp"
```

### Available MCP Tools

Once connected, Claude Code can use:

- `run_gradle_task` — trigger any Gradle task
- `get_build_status` / `get_build_errors` — check build results
- `get_logcat_entries` — read recent device logs
- `get_crash_logs` — get recent crash stack traces
- `list_devices` — see connected devices
- `install_apk` / `launch_app` — deploy and run
- `list_build_variants` / `set_active_variant`

### Workflow Example

1. Write code in Android Studio
2. Ask Claude Code: *"Build the app and show me any errors"*
3. Claude calls `run_gradle_task("assembleDebug")`
4. Build panel shows live output
5. Claude calls `get_build_errors()` and explains the issues
6. Fix in Android Studio
7. Ask Claude: *"Run it on the connected emulator"*
8. Claude calls `run_gradle_task`, `install_apk`, `launch_app`
