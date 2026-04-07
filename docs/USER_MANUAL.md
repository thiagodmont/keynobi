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

1. Launch **Android Dev Companion**. A **Setup** wizard runs once: it can auto-detect your **Android SDK** and **Java** home (or you can enter paths manually), asks whether to enable **anonymous crash reporting** (off by default), and offers optional defaults (MCP auto-start, logcat auto-start). You can **Skip setup** and finish later in **Settings** (Cmd+,).
2. If you enable crash reporting, it takes effect after the **next app restart** (the setting is saved immediately). Reports are limited to **app-side** diagnostics (e.g. crash type and sanitized stack information) so we can fix stability issues. They do **not** include your project paths, source code, Gradle or log output, or device identifiers.
3. Press **Cmd+O** or click the title bar to open the project switcher and select your Android project folder.
4. The app detects your Gradle root, saves the project to the registry, and initializes build variants.
5. If you skipped the wizard or need to change paths later, open **Settings** (Cmd+,) and configure:
   - **Android SDK Path** — path to `$ANDROID_HOME` (e.g. `~/Library/Android/sdk`)
   - **Java Home** — path to JDK (e.g. `/Library/Java/JavaVirtualMachines/jdk-17.jdk/Contents/Home`)

To open the setup wizard again, use the Command Palette (**Cmd+Shift+P** → “Open Setup Wizard”) or **Cmd+Shift+W**.

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

The **title bar** doubles as a project switcher. Click the project name (or "Android Dev Companion" when no project is open) to open the dropdown.

```
┌─────────────────────────────────────────────────────────────┐
│  ●  ○  ○   Android Dev Companion — MyApp ▾                   │
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

The Logcat panel streams Android device log output in real time through a multi-stage processing pipeline that enriches entries before display.

### Starting Logcat

1. Connect an Android device or start an emulator (see Devices panel)
2. Click **Start** in the Logcat toolbar, or the app will auto-connect to the selected device
3. Log entries appear immediately as they arrive, enriched with package resolution and category detection

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

The query bar supports a rich filter syntax. Simple filters (level, tag, text, package) are applied in the Rust backend before entries cross the IPC bridge — this keeps the JS thread responsive even at 1000+ lines/sec.

| Syntax | Example | Description |
|--------|---------|-------------|
| `level:X` | `level:error` | Minimum log level |
| `tag:X` | `tag:OkHttp` | Tag substring |
| `tag~:X` | `tag~:Ok.*Http` | Tag regex |
| `message:X` | `message:null` | Message substring |
| `package:X` | `package:com.example` | Package name |
| `package:mine` | `package:mine` | Filter to the current app |
| `age:N` | `age:5m` | Only last N seconds/minutes/hours |
| `is:crash` | `is:crash` | Crash entries only |
| `is:stacktrace` | `is:stacktrace` | Stack trace lines only |
| `-tag:system` | `-tag:system` | Negate — exclude entries matching |
| bare text | `login` | Search tag + message + package |
| `A && B` | `level:error && tag:MyApp` | **AND** — explicit AND connector (same as space) |
| `A \| B` | `level:error tag:MyApp \| is:crash` | **OR** — entries matching either group |

Multiple tokens within a group are AND-ed together (space or `&&`). Use `|` to separate OR groups — an entry is shown if it satisfies **any** group. For example:

```
level:error && tag:MyApp | is:crash
```

Shows all error-or-above logs from `MyApp` **or** any crash entry from any process.

**Building compound filters:**
- Click **+ AND** to append `&&` to the active group (makes the AND relationship explicit).
- Click **+ OR** to start a new OR group with ` | `.
- You can mix both: `tag:App && level:warn | is:crash | package:mine && age:5m`.
- The query bar shows an **N OR** badge when multiple OR groups are active.
- Autocomplete works independently within each group — suggestions are scoped to wherever the cursor is.

### Saved Filters

Use the **☰ Filters** button to manage filters:

- **Quick Filters** — built-in one-click presets (My App, Crashes, Errors+, Last 5 min, and more).
- **Saved** — your saved filters, shown with a `N / 50` count. Up to 50 filters can be saved.
  - Click a filter name to apply it.
  - Click **✎** to rename a filter inline (press Enter to confirm, Esc to cancel).
  - Click **✕** to delete a filter.
- **+ Save current filter** — saves the active query under a name you choose. If a filter with the same name already exists, it is overwritten.

Your last active query is automatically restored the next time you open the Logcat panel.

> **Migration**: filters saved under the old "Presets" system are automatically migrated to the new Saved Filters format on first use.

### Package Filter Dropdown

The second toolbar row contains a **package filter dropdown** (labelled "All packages" by default). Click it to open a searchable list of all package names that have produced log output in the current session.

- **All packages** — removes the package filter and shows everything
- **My App** — shortcut for `package:mine` (resolves to the project's `applicationId`)
- **Individual packages** — any package name seen so far, sorted alphabetically
- Use the search box inside the dropdown to narrow the list when many packages are present

Selecting a package inserts a `package:` token into the query bar, which triggers backend-side filtering (entries from other packages are not forwarded across IPC). The dropdown reflects whatever package token is currently in the query, and editing the query bar directly keeps the dropdown in sync.

### JSON Viewer

When a log entry's message contains valid JSON, a `{}` badge appears on the row. Click the badge to open the **JSON Detail Panel** at the bottom of the log, which shows the JSON formatted and syntax-highlighted. Click **Copy** to copy the raw JSON, or **✕** to close the panel.

### Controls

- **Start / Stop** — Begin or end logcat streaming
- **Pause / Resume** — Pause new entries (no data is lost, buffer continues)
- **Clear** — Clear the display buffer and the in-memory ring buffer
- **Age pills** — Quick-select time window (30s, 1m, 5m, 15m, 1h, All)
- **☰ Filters** — Open the saved filters dropdown (Quick Filters + your saved filters)
- **↓ Export** — Save the currently filtered entries to a `.log` file
- **⎘ N rows** — Copy multiple selected rows (Shift+click to select a range)

### Ring Buffer

The logcat ring buffer holds up to **50,000 entries** in memory. The oldest entries are evicted when the buffer is full. All entries since starting are kept until you click Clear.

### Crash Detection

When a `FATAL EXCEPTION`, `AndroidRuntime` crash, ANR, or native signal crash is detected:
- The entry is highlighted in red with a red left border
- All consecutive lines in the same stack trace share a `crash_group_id` and get the same red indicator
- The crash counter in the toolbar (⚡ N) increments
- Use **↑ / ↓** beside the crash counter to navigate between crashes
- ANR entries are highlighted in yellow with an **ANR** badge

### Entry Categories

The pipeline automatically classifies entries by tag into categories (visible in filtering):
- **Network** — OkHttp, Retrofit, Volley
- **Lifecycle** — ActivityManager, Fragment, Application
- **Performance** — Choreographer, SurfaceFlinger, OpenGLRenderer
- **GC** — art, dalvikvm
- **Database** — SQLiteDatabase, Room

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
| Health | App health indicator — click to open Health Center |
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

### Telemetry (crash reporting)

Under **Privacy**, **Anonymous crash reporting** is **off** by default. When enabled, the app may send **minimal, non-identifying** crash reports from the desktop app itself (for example, panic and error summaries with paths stripped) to help fix bugs. It does **not** send your Android project files, Gradle log content, logcat, MCP traffic, or personal identifiers. Changing the toggle applies fully after **restart** (same as in the setup wizard).

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
| Disk Space | Free space in `~/.keynobi/` |
| App Directory | `~/.keynobi/` is writable |

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
| **Cmd+Shift+W** | Open Setup Wizard |
| **Cmd+Shift+H** | Open Health Center |
| **Cmd+Shift+P** | Command Palette |

**Command Palette actions** (Cmd+Shift+P, then type):
- `Open Setup Wizard` — environment paths, privacy, and workflow defaults
- `Project App Info` — open the version name/code editor
- `Open Folder` — browse for a new project

---

## 10. Claude Code Integration

Android Dev Companion exposes an **MCP server** that Claude Code can connect to. This lets Claude Code:
- Trigger builds and read structured errors
- Read logcat output and crash logs
- Manage devices, install APKs, and launch apps
- List and switch build variants
- Run health checks and query project info

### Setup

**Option A — Headless mode (recommended for Claude Code)**

The companion binary can run as a headless MCP server with no GUI window:

```bash
claude mcp add --transport stdio android-companion -- "/Applications/AndroidDevCompanion.app/Contents/MacOS/android-dev-companion" --mcp
```

The MCP server automatically uses whichever Android project is currently open in the companion app. No extra configuration is needed — just open your project and the MCP will pick it up.

To override and point at a specific project regardless of what the companion app has open:

```bash
claude mcp add --transport stdio android-companion -- "/path/to/android-dev-companion" --mcp --project /path/to/MyAndroidProject
```

**Option B — GUI mode**

1. Open the companion app and load your project
2. Open the command palette (Cmd+Shift+P) and run `Start MCP Server (for Claude Code)`
3. The MCP indicator in the status bar turns blue
4. Add to Claude Code using the setup command shown in the Health panel (Cmd+Shift+H)

### Available MCP Tools

| Tool | Description |
|------|-------------|
| `run_gradle_task` | Run a Gradle task (e.g. `assembleDebug`), returns result + structured errors |
| `get_build_status` | Current build status with duration and error counts |
| `get_build_errors` | Structured compiler errors with file:line locations (JSON) |
| `get_build_log` | Raw build output lines (last N lines) |
| `cancel_build` | Cancel a running build |
| `list_build_variants` | List variants and show the active one (JSON) |
| `set_active_variant` | Switch to a different build variant |
| `find_apk_path` | Find the output APK path after a build |
| `run_tests` | Run unit or connected tests |
| `start_logcat` | Start logcat streaming (required in headless mode) |
| `stop_logcat` | Stop the logcat stream |
| `get_logcat_entries` | Recent logcat entries with level/tag/text/package filter (JSON) |
| `get_crash_logs` | FATAL EXCEPTION, ANR, and native crash entries (JSON) |
| `clear_logcat` | Clear the in-memory logcat buffer |
| `get_logcat_stats` | Logcat statistics (counts by level, crashes, packages) |
| `list_devices` | Connected ADB devices — always queries ADB fresh (JSON) |
| `refresh_devices` | Force-refresh device list from ADB |
| `screenshot` | Capture a screenshot — returns inline image |
| `get_device_info` | SDK level, model, screen size, battery |
| `dump_app_info` | App version, install path, activities |
| `get_memory_info` | PSS, heap, native, graphics memory breakdown |
| `install_apk` | Install an APK on a device (path-validated) |
| `launch_app` | Launch an app with `am start` |
| `stop_app` | Force-stop an app |
| `list_avds` | List configured Android Virtual Devices (JSON) |
| `launch_avd` | Start an emulator |
| `stop_avd` | Stop a running emulator |
| `get_project_info` | Project name, path, and Gradle root |
| `run_health_check` | Java, SDK, ADB, Gradle wrapper status (JSON) |

### MCP Prompts

Three built-in prompts wire multiple tools together:

| Prompt | Description |
|--------|-------------|
| `diagnose-crash` | Fetches crash logs, memory, and app info for root-cause analysis |
| `full-deploy` | Builds, finds APK, installs, and launches in one workflow |
| `build-and-fix` | Runs build and explains each error with suggested fixes |

### MCP Resources

The server exposes project files as readable resources:

- `android://manifest` — AndroidManifest.xml
- `android://app-build-gradle` — app/build.gradle.kts
- `android://build-gradle` — root build.gradle.kts
- `android://gradle-settings` — settings.gradle.kts
- `android://project-info` — project name and path

### Workflow Example

1. Ask Claude Code: *"Build the app and show me any errors"*
2. Claude calls `get_project_info()` to verify the project is open
3. Claude calls `run_gradle_task("assembleDebug")`
4. Claude calls `get_build_errors()` and explains the issues
5. You fix in your editor
6. Ask Claude: *"Run it on the connected emulator"*
7. Claude calls `list_devices()` to pick a device, then `install_apk()` and `launch_app()`

### MCP Status Indicator

The status bar shows an **MCP pill** that:
- Is grey when the server is not started
- Turns blue when the MCP server is running (stdio transport)
- Click to start the server (if not running) or copy the setup command

The **Health panel** (Cmd+Shift+H) shows the MCP server status and the exact `claude mcp add` command to run.
