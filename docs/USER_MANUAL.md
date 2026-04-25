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
4a. [Layout Panel](#4a-layout-panel)
5. [Devices Sidebar](#5-devices-sidebar)
6. [Status Bar](#6-status-bar)
7. [Settings](#7-settings)
8. [Health Center](#8-health-center)
9. [Keyboard Shortcuts](#9-keyboard-shortcuts)
10. [Claude Code Integration](#10-claude-code-integration)
11. [Theme and colors](#11-theme-and-colors)

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

The window has a **title bar** (draggable; shows the app name and current project, plus a **Build** button that always opens the Build panel), a **tab bar** with **Logcat**, **Layout**, and **Build**, **sidebars** on the left and right, and a **status bar** at the bottom.

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Title bar (app — project)                                    [ Build ]     │
├───┬──[ Logcat ]──[ Layout ]──[ Build ]───────────────────────────────────┬───┤
│ P │                                                                      │ D │
│ r │  Active tab content (Logcat, Layout, or Build)                         │ e │
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
- **Layout** tab — UI Automator / accessibility hierarchy for the focused screen (native Views and Jetpack Compose)
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

The log toolbar includes **↓** to turn follow-tail (auto-scroll to the latest line) on or off. Whether follow-tail starts enabled when you open the app is set under **Settings → Advanced → Auto-scroll build log to end** (on by default).

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

### Keyboard navigation in the log list

With the **Logcat** tab active (and focus not in the query bar, a text field, or the command palette), use **↑** and **↓** to move the selection between log lines. Process start/stop separator rows are skipped. The focused line shows a **thick accent bar** on the left (the same bar marks the row shown in the **Entry Detail** panel); Shift+click ranges use a lighter tint on all included rows, with the bar on the anchor row only. The list scrolls to keep the focused row in view. **Esc** closes the detail panel. Plain **click** still toggles the detail panel for the same row. While a line is selected (mouse or keyboard) or the JSON detail panel is open, new log lines do not auto-scroll the list; use **↓** in the toolbar to return to the end and resume follow-tail.

### Controls

- **Start / Stop** — Begin or end logcat streaming
- **Pause / Resume** — Pause new entries (no data is lost, buffer continues)
- **↓** (first toolbar row) — Jump to the latest entry and resume follow-tail. Clears row selection, JSON detail, and entry detail so new lines can auto-scroll again.
- **Clear** — Clear the display buffer and the in-memory ring buffer
- **Age pills** — Quick-select time window (30s, 1m, 5m, 15m, 1h, All)
- **☰ Filters** — Open the saved filters dropdown (Quick Filters + your saved filters)
- **↓ Export** — Save the currently filtered entries to a `.log` file
- **⎘ N rows** — Copy multiple selected rows (Shift+click to select a range)
- **Line count** (right side of the first toolbar row) — With a filter active, shows **`shown / ring`** (for example `50 / 1,000`): lines currently listed after all filters, then the **total lines stored in the app’s logcat ring buffer** (includes lines hidden by the stream filter and not sent to the list). With no query, a single total is shown when it matches the ring. Hover the count for details.

### Ring Buffer

The logcat ring buffer size is configurable under **Settings → Logcat → Ring buffer size** (default **50,000** entries, up to **100,000**). The oldest entries are evicted when the buffer is full. All entries since starting are kept until you click Clear. The **listed** lines in the panel are capped separately by **Max lines in Logcat** (default 20k) and cannot exceed the ring size.

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

## 4a. Layout Panel

The **Layout** tab shows the **accessibility / UI Automator** tree for whatever is on screen on the **selected device** — the same surface **UI Automator**, **TalkBack**, and MCP automation see. **Jetpack Compose** is primary: composables draw a **semantics** tree that merges into this view (not the full composable call tree). **Android Views / XML** appear as usual when not using Compose.

### How to use

1. Select an **online** device in the **Devices** sidebar (right).
2. Open the **Layout** tab (**Cmd+4**).
3. Click **Refresh**. Keynobi runs official **platform-tools** commands: `dumpsys activity activities`, capped excerpts from **`dumpsys window windows`**, **`dumpsys display`**, **`wm size`**, **`wm density`**, then **`uiautomator dump`** (tries **`--compressed`** first when the device supports it, then plain dump and file fallbacks). The XML is parsed into a tree.

### Toolbar

- **Refresh** — Capture the current hierarchy (snapshot; not live video).
- **Interactive only** — Hide branches that are not clickable/scrollable/editable and do not carry text or content description (similar to automation “actionable” lists). Parent rows may show a short **inherited** snippet from a descendant so clickable Compose groups stay readable.
- **Hide boilerplate** — Collapse long chains of single-child, same-bounds, empty wrappers (sibling-safe: parallel subtrees like status bar vs content stay separate).
- **Filter** — Narrow the tree by substring in class, resource id, text, content description, or package. **Prev / Next** steps through matches and expands the path to each hit.
- **Expand all / Collapse all** — Control disclosure state. Default expansion depth scales with tree size (deeper on smaller dumps).

### Tree and detail

- The **left column** is a **wireframe** (SVG) in **device pixel** coordinates: every node with valid bounds is drawn as a rectangle, using the same filtered tree as the list (interactive only, hide boilerplate, search). **Click a region** to select that node: the **tree expands only along the path** from the root to that row (ancestors open; the node’s own subtree stays collapsed unless you expand it or shallow auto-expand applies), then **scrolls** the row into view, highlights it, and updates the **detail** panel. Overlapping rects prefer the **smallest** containing hit (so nested controls win). Very large trees may show only the first **2000** rects (a short notice appears); refine filters to reduce count.
- Rows show **class**, a **resource-id** chip (when present), **text** and **content-desc** previews, bounds with **width×height**, and badges such as **minified** (short obfuscated class names), **merged target** (typical Compose tap parent), **selected** (e.g. tab), and **Compose** where applicable. The dominant **package** for the dump is shown under the toolbar; it is omitted on rows when it matches, to reduce noise.
- Click a row to select it; the **right-hand detail** panel lists every field (full class, id, text, descriptions, flags) and offers **Copy bounds**, **Copy id**, **Copy summary**, and **Find parent** (selects the **direct parent** of the current row in the displayed tree—the node one level up; disabled on the display root). Accessibility flags in dumps are not always accurate (for example, some `HorizontalScrollView` nodes report `scrollable=false`); use them as hints.

### Metadata

Below the tree, Keynobi shows capture time, a **screen hash** (for comparing whether the screen changed), interactive node count, parser **warnings**, and a best-effort **foreground activity** line from `dumpsys activity activities` when available. The same refresh still records capped window/display/`wm` excerpts on the snapshot (for MCP and other tooling); they are **not** shown in this panel.

At the **bottom of the Layout panel**, after a successful refresh, Keynobi lists **ADB commands (this capture)** — every `adb -s …` line run for that refresh so you can paste them into a terminal.

### Deeper inspection (debuggable apps, Compose-first)

The Layout tab is the right default for **automation-aligned** trees. It does **not** replace Android Studio’s **full composable / modifier** view.

- **Semantics and stable ids** — Follow [Semantics in Compose](https://developer.android.com/develop/ui/compose/accessibility/semantics): `Modifier.testTag`, `testTagsAsResourceId` (for `resource-id` in dumps), `contentDescription`, and custom `Modifier.semantics` where needed. See also [Inspect and debug (Compose accessibility)](https://developer.android.com/develop/ui/compose/accessibility/inspect-debug).
- **Layout Inspector (Android Studio)** — For **debuggable** builds, use [Layout Inspector](https://developer.android.com/studio/debug/layout-inspector) for composable parameters, semantics properties (e.g. hidden from accessibility), and recompositions. Studio can enable **view attribute inspection** via the global setting below.
- **View attribute inspection (optional, terminal)** — On a device or emulator: `adb shell settings put global debug_view_attributes 1` (and `0` or `delete` to turn off). This is primarily for **Layout Inspector** and richer **View** metadata on **debuggable** processes; impact on `uiautomator dump` XML varies by OS and app—treat any extra fields as a bonus, not a guarantee.
- **Semantics text via tests (terminal)** — Run instrumented UI tests from your project, for example `./gradlew :app:connectedDebugAndroidTest`, and use `ComposeTestRule.onRoot().printToLog("tag")` (or similar) so the **semantics tree** is printed to **logcat**—official AndroidX testing, useful when the XML snapshot is not enough.

**Reference:** the `uiautomator` shell command is part of the platform ([AOSP `cmds/uiautomator`](https://github.com/aosp-mirror/platform_frameworks_base/tree/master/cmds/uiautomator)); **adb** only transports it ([adb module](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main)). For a broad menu of other official-style `adb` / `dumpsys` / `logcat` ideas (validate on your API level), see the community gist [Adb useful commands](https://gist.github.com/Pulimet/5013acf2cd5b28e55036c82c91bd56d8).

### Limitations

- **Snapshot only** — the UI may change between refreshes; rapid polling can occasionally fail or return empty dumps.
- **Compose vs composables** — You see **merged semantics** as exposed to accessibility, not every composable or internal modifier chain. Shallow trees are normal when children merge.
- **`FLAG_SECURE`** — can hide or obscure content from accessibility dumps.

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
| Update | New Keynobi version indicator — click to open the GitHub release download page |
| Variant pill | Active build variant (when a project is open) — click to change |
| App memory | Approximate app memory use (right side) |
| Log folder size | Log folder size vs configured cap (right side); tooltip shows rotation when applicable |

### Update Notification

On startup, Keynobi checks the latest GitHub release at `thiagodmont/keynobi`. When a newer version is available, a modal offers **Download** or **Later**. **Download** opens the GitHub release page. **Later** dismisses that release's modal permanently, but the status bar still shows the update indicator so you can open the release page later.

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

### Logcat

- **Auto-start on connect** — Start logcat streaming when a device connects (on by default).
- **Auto-scroll Logcat to end** — When on (default), the Logcat tab opens with follow-tail enabled so new lines scroll into view. Follow-tail pauses automatically while a log line is selected, while the JSON detail panel is open, or when you scroll away from the bottom; use the **↓** control in the Logcat toolbar to jump to the end and resume. You can still pause follow-tail from that toolbar at any time.
- **Ring buffer size** — How many lines the app stores in the in-memory capture ring before the oldest are dropped (default **50,000**, range **1,000**–**100,000**). Changing it applies immediately after settings save.
- **Max lines in Logcat** — How many lines the Logcat tab keeps in the UI list and requests when loading from the capture buffer (default **20,000**). It cannot exceed the ring buffer size.

### Advanced (Build)

Under **Settings → Advanced**, the **Build** section includes **Auto-scroll build log to end** (on by default), which sets the initial follow-tail state for the Build log tab. The log toolbar **↓** still toggles follow-tail for the current session.

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
| **Cmd+4** | Open Layout tab |
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
| `get_ui_hierarchy` | Focused window UI tree from UI Automator (full JSON, or `interactive_only` for compact rows) |
| `find_ui_elements` | Fresh dump + search by text, content-desc, resource-id, class, or package; returns `treePath`, bounds, `centerX`/`centerY`, flags, and `screenHash` (needs at least one primary filter) |
| `find_ui_parent` | Given a non-empty `treePath` from `find_ui_elements` or the Layout tab, returns the **direct parent** node (same match shape: `treePath`, bounds, centers, flags) plus `screenHash`; optional `expect_screen_hash` like `ui_tap` |
| `ui_tap` | Tap device pixel coordinates (use centers from `find_ui_elements`); optional `expect_screen_hash` to refuse if the UI changed |
| `ui_type_text` | `adb shell input text` after optional tap to focus; ASCII-oriented (no emoji); optional `expect_screen_hash` |
| `ui_swipe` | Swipe or long-press (`duration_ms`, same start/end) in device pixels |
| `send_ui_key` | Allowlisted keyevent (Back, Home, Enter, Delete, Tab, D-pad, Menu, AppSwitch, …) |
| `grant_runtime_permission` | `pm grant` for `android.permission.*` on a package |
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

For **UI automation**, use `find_ui_elements` to locate controls, then `ui_tap` on the returned centers. Use **`find_ui_parent`** with a match’s `treePath` to walk up one level in the same tree as the Layout viewer (repeat to reach an ancestor container). Pass `screenHash` from the find result into `expect_screen_hash` on tap/type when you need to avoid acting on a stale screen. `ui_type_text` is limited by what `adb input text` supports (mostly ASCII).

### MCP Status Indicator

The status bar shows an **MCP** pill:
- **Dim** when idle (no server process or client activity detected)
- **Info (blue)** when an MCP server process is alive (stdio transport)
- **Warning (amber)** when the GUI believes a server run is in progress
- **Success (green)** when a client (e.g. Claude Code) is connected — the pill may show the client name
- **Click** opens the MCP activity panel (setup, copied command reminder, activity log)

The **Health Center** (Cmd+Shift+H) shows MCP integration status and the exact `claude mcp add` command with your real binary path.

---

## 11. Theme and colors

The app uses **CSS variables** in `src/styles/theme.css` (backgrounds, text, borders, and **semantic** colors: success, error, warning, info). Build status, device connection state, log levels, logcat chips, and status-bar indicators draw from these tokens so the UI stays consistent. Toolbar icon buttons use the shared **IconButton** control; destructive actions in menus use a dedicated **destructive** style where applicable.
