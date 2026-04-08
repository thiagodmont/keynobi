# Keynobi — User Manual

> A focused Android development companion for build logs, logcat, and device management.  
> Works alongside Android Studio and Claude Code.

The distributed app and installer are named **Keynobi**.

---

## Table of Contents

1. [Getting Started](#1-getting-started)
2. [Layout Overview](#2-layout-overview)
3. [Build Panel](#3-build-panel)
4. [Logcat Panel](#4-logcat-panel)
5. [Devices Sidebar](#5-devices-sidebar)
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

1. Launch **Keynobi**. A **Setup** wizard runs once: it can auto-detect your **Android SDK** and **Java** home (or you can enter paths manually), asks whether to enable **anonymous crash reporting** (off by default), and offers optional defaults (MCP auto-start, logcat auto-start). You can **Skip setup** and finish later in **Settings** (Cmd+,).
2. If you enable crash reporting, it takes effect after the **next app restart** (the setting is saved immediately). Reports are limited to **app-side** diagnostics (e.g. crash type and sanitized stack information) so we can fix stability issues. They do **not** include your project paths, source code, Gradle or log output, or device identifiers.
3. Add a project: press **Cmd+O** or click **Add Project…** at the bottom of the left **Projects** sidebar, then choose your Android project folder.
4. The app detects your Gradle root, saves the project to the registry, and initializes build variants.
5. If you skipped the wizard or need to change paths later, open **Settings** (Cmd+,) and configure:
   - **Android SDK Path** — path to `$ANDROID_HOME` (e.g. `~/Library/Android/sdk`)
   - **Java Home** — path to JDK (e.g. `/Library/Java/JavaVirtualMachines/jdk-17.jdk/Contents/Home`)

To open the setup wizard again, use the Command Palette (**Cmd+Shift+P** → “Open Setup Wizard”) or **Cmd+Shift+W**.

### Subsequent Launches

The app automatically restores the last-active project. No need to re-open the folder each time.

---

## 2. Layout Overview

The window has a **title bar** (draggable; shows the app name and current project), a **tab bar** with two tabs, **sidebars** on the left and right, and a **status bar** at the bottom.

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Title bar (app name — project name)                                        │
├───┬──[ Build ]──[ Logcat ]──────────────────────────────────────────────┬───┤
│ P │                                                                      │ D │
│ r │  Active tab content (Build or Logcat)                                 │ e │
│ o │                                                                      │ v │
│ j │                                                                      │ i │
│ e │                                                                      │ c │
│ c │                                                                      │ e │
│ t │                                                                      │ s │
│ s │                                                                      │   │
├───┴──────────────────────────────────────────────────────────────────────┴───┤
│  Status bar: Settings · project · Health · Build · MCP · variant · memory…  │
└────────────────────────────────────────────────────────────────────────────┘
```

- **Build** tab — Gradle build output and error list
- **Logcat** tab — Android device log streaming
- **Projects** sidebar (left) — project registry, open/switch projects (**Cmd+B** toggles collapse)
- **Devices** sidebar (right) — connected devices and emulators (**Cmd+3** toggles visibility)

---

## 2a. Projects sidebar

Use the **Projects** sidebar on the left to work with saved projects. The title bar only shows the current project name; it does not open a menu.

```
┌────────────────────┐
│ PROJECTS        ‹› │  ← collapse/expand
│ ┌────────────────┐ │
│ │ MyApp  gradlew │ │  ← active row highlighted
│ │ ~/code/...     │ │
│ └────────────────┘ │
│   OtherProject …  │
│ ─────────────────  │
│  ⊕ Add Project…    │  ← same as Cmd+O
└────────────────────┘
```

### Actions
- **Click a project row** — switch the active project (cancels a running build for the previous project; **does not stop logcat**)
- **Pencil** (on hover) — rename the project in the list (does not rename the folder on disk)
- **×** (on hover, non-active projects only) — remove from the list (**does not** delete the project from disk)
- **Add Project…** — browse for a new folder (same as **Cmd+O**)
- **Sidebar header control** — collapse or expand the sidebar; **Cmd+B** toggles the same

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

1. Connect an Android device or start an emulator (see [Devices Sidebar](#5-devices-sidebar))
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

## 5. Devices Sidebar

The **Devices** sidebar on the right lists connected physical devices and available emulators. **Cmd+3** toggles this sidebar open and closed.

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

The selected device is used for installs and launches from the Build panel. When no suitable device is available, the app may prompt you to pick one. Device choice is not shown as a separate pill on the status bar—use the Devices sidebar (or the picker when prompted).

---

## 6. Status Bar

The status bar at the bottom shows (left to right on the main strip, then indicators on the right):

| Item | Description |
|------|-------------|
| ⚙ | Settings gear — click to open settings |
| Project name | Name of the open Android project (or app name when none is open) |
| Health | App health indicator — click to open Health Center |
| Build status | Current or last build — click to switch to the Build tab |
| MCP | MCP integration — click to open the MCP activity panel (setup, copy command, activity log) |
| Variant pill | Active build variant (when a project is open) — click to change |
| App memory | Approximate app memory use (right side) |
| Log folder size | Log folder size vs configured cap (right side); tooltip shows rotation when applicable |

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

### Telemetry (crash reporting)

Under **Privacy**, **Anonymous crash reporting** is **off** by default. When enabled, the app may send **minimal, non-identifying** crash reports from the **native** layer and the **UI** (for example, panic and UI error summaries with paths stripped) to help fix bugs. It does **not** send your Android project files, Gradle log content, logcat, MCP traffic, or personal identifiers. Changing the toggle applies fully after **restart** (same as in the setup wizard).

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
| **Cmd+O** | Open / add Android project folder |
| **Cmd+R** | Run App (build → install → launch) |
| **Cmd+Shift+R** | Build Only (no deploy) |
| **Cmd+Shift+V** | Select Build Variant |
| **Cmd+1** | Open Build tab |
| **Cmd+2** | Open Logcat tab |
| **Cmd+3** | Toggle Device Sidebar |
| **Cmd+B** | Toggle Project Sidebar |
| **Cmd+,** | Open Settings |
| **Cmd+Shift+W** | Open Setup Wizard |
| **Cmd+Shift+H** | Open Health Center |
| **Cmd+Shift+M** | Open MCP Activity Panel |
| **Cmd+Shift+P** | Command Palette |

**Command Palette actions** (Cmd+Shift+P, then type):
- `Open Setup Wizard` — environment paths, privacy, and workflow defaults
- `Project App Info` — open the version name/code editor
- `Open Folder` — browse for a new project
- `Cancel Build` — stop the current Gradle run
- `Clean Project` — run `./gradlew clean`
- `Manage Virtual Devices` — toggle the Device Sidebar
- `Copy MCP Setup Command` — copy the `claude mcp add …` line (uses this app’s real binary path)

---

## 10. Claude Code Integration

Keynobi exposes an **MCP server** that Claude Code can connect to. This lets Claude Code:
- Trigger builds and read structured errors
- Read logcat output and crash logs
- Manage devices, install APKs, and launch apps
- List and switch build variants
- Run health checks and query project info

### Setup

**Option A — Headless mode (recommended for Claude Code)**

The app binary can run as a headless MCP server with no GUI window. Register it once with Claude Code using the **exact path** to the installed binary (the Health Center shows the command with your real path). A typical install location looks like:

```bash
claude mcp add --transport stdio keynobi -- "/Applications/Keynobi.app/Contents/MacOS/keynobi" --mcp
```

Always prefer the command copied from **Health Center** (Cmd+Shift+H) or **Copy MCP Setup Command** in the command palette—paths vary by machine and install location.

The MCP server uses whichever Android project is currently open in Keynobi. No extra configuration is needed — open your project in the app and the MCP session will see it.

To override and point at a specific project regardless of what the app has open, append `--project`:

```bash
claude mcp add --transport stdio keynobi -- "/Applications/Keynobi.app/Contents/MacOS/keynobi" --mcp --project /path/to/MyAndroidProject
```

**Option B — GUI mode**

1. Open Keynobi and load your project
2. Use **Cmd+Shift+M** or click the **MCP** pill in the status bar to open the MCP panel
3. Run **Copy MCP Setup Command** from the command palette (Cmd+Shift+P) and paste the line in your terminal to register with Claude Code, or copy the same command from **Health Center** (Cmd+Shift+H)
4. The MCP pill updates to reflect server/client state (see [MCP Status Indicator](#mcp-status-indicator) below)

### Available MCP Tools

| Tool | Description |
|------|-------------|
| `run_gradle_task` | Run a Gradle task (e.g. `assembleDebug`), returns result + structured errors |
| `get_build_status` | Current build status (idle, running, success, failed, cancelled) |
| `get_build_errors` | Structured compiler errors and warnings from the last build (JSON) |
| `get_build_log` | Raw Gradle output lines (last N lines, capped) |
| `cancel_build` | Cancel the running Gradle build |
| `list_build_variants` | List variants and active variant (JSON) |
| `set_active_variant` | Set the active build variant (persists in settings) |
| `find_apk_path` | Output APK path for a variant after a build |
| `get_build_config` | Parse `build.gradle(.kts)` for SDK levels, `applicationId`, build types, flavors (no Gradle run) |
| `run_tests` | Run unit tests (`testDebug`), connected tests, or a custom test task |
| `get_crash_stack_trace` | Parsed crash from logcat buffer (frames, caused-by); needs logcat streaming |
| `restart_app` | Force-stop or clear data, relaunch, wait for display |
| `get_app_runtime_state` | Processes, threads, RSS for an app package |
| `start_logcat` | Start logcat streaming (required in headless mode before reads) |
| `stop_logcat` | Stop the logcat stream |
| `get_logcat_entries` | Recent logcat entries with filters (JSON) |
| `get_crash_logs` | FATAL EXCEPTION, ANR, and native crash entries (JSON) |
| `clear_logcat` | Clear the in-memory logcat buffer |
| `get_logcat_stats` | Logcat statistics (counts by level, crashes, packages) |
| `list_devices` | Connected ADB devices (JSON) |
| `screenshot` | Capture a screenshot (inline image) |
| `get_device_info` | SDK level, model, screen, battery |
| `dump_app_info` | App version, install path, activities |
| `get_memory_info` | Memory breakdown (PSS, heap, native, graphics) |
| `install_apk` | Install an APK (path-validated) |
| `launch_app` | Launch an app (`am start`) |
| `stop_app` | Force-stop an app |
| `list_avds` | List AVDs (JSON) |
| `launch_avd` | Start an emulator |
| `stop_avd` | Stop a running emulator |
| `get_project_info` | Project name, path, Gradle root |
| `run_health_check` | Java, SDK, ADB, Gradle wrapper, disk, app dir (JSON) |

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

The status bar shows an **MCP** pill:
- **Dim** when idle (no server process or client activity detected)
- **Blue** when an MCP server process is alive (stdio transport)
- **Amber** when the GUI believes a server run is in progress
- **Green** when a client (e.g. Claude Code) is connected — the pill may show the client name
- **Click** opens the MCP activity panel (setup, copied command reminder, activity log)

The **Health Center** (Cmd+Shift+H) shows MCP integration status and the exact `claude mcp add` command with your real binary path.
