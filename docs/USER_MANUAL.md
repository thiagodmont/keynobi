# Keynobi User Guide

Keynobi is a macOS companion app for Android development. It sits next to Android Studio and gives you one place for Gradle builds, logcat, devices, app health, UI hierarchy inspection, and Claude Code MCP workflows.

Use this guide as a quick reference. Most commands are also available from the Command Palette with `Cmd+Shift+P`.

---

## Contents

1. [Quick Start](#quick-start)
2. [Window Overview](#window-overview)
3. [Projects](#projects)
4. [Builds](#builds)
5. [Logcat](#logcat)
6. [Layout Viewer](#layout-viewer)
7. [Devices](#devices)
8. [Settings and Health](#settings-and-health)
9. [Claude Code MCP](#claude-code-mcp)
10. [Keyboard Shortcuts](#keyboard-shortcuts)
11. [Troubleshooting](#troubleshooting)

---

## Quick Start

### Requirements

- macOS, Apple Silicon or Intel.
- Android SDK with platform-tools.
- Java 11 or newer.
- Android project with a `gradlew` wrapper.

### First launch

1. Open **Keynobi**.
2. Complete the setup wizard, or skip it and finish later in **Settings**.
3. Set your **Android SDK Path** and **Java Home** if auto-detect does not find them.
4. Press `Cmd+O` or click **Add Project** and choose your Android project folder.
5. Select a device or start an emulator.
6. Press `Cmd+R` to build, install, and launch.

Keynobi restores the last active project on later launches.

### Crash reporting

Anonymous crash reporting is off by default. If enabled, it takes full effect after restart and sends only app-side diagnostics such as sanitized crash summaries. It does not send source code, project files, Gradle output, logcat, MCP traffic, personal identifiers, or device identifiers.

---

## Window Overview

Keynobi has three main tabs:

- **Build**: Gradle output, build history, and structured errors.
- **Logcat**: live Android logs with filtering and crash detection.
- **Layout**: UI Automator accessibility tree and wireframe for the selected device.

Sidebars and status:

- **Projects sidebar** on the left: saved Android projects.
- **Devices sidebar** on the right: physical devices and emulators.
- **Status bar** at the bottom: settings, project, health, build status, MCP, updates, variant, memory, and log folder size.

---

## Projects

Use the **Projects** sidebar to add, switch, rename, or remove saved projects.

- `Cmd+O`: add or open a project folder.
- `Cmd+B`: collapse or expand the Projects sidebar.
- Click a project row to switch projects.
- Rename changes only the label in Keynobi; it does not rename the folder.
- Remove deletes the saved entry only; it does not delete files on disk.

### Project App Info

Open **Project App Info** from the Command Palette to edit:

- `versionName`
- `versionCode`

The app also shows `applicationId` as read-only. Edits are written to `app/build.gradle` or `app/build.gradle.kts`.

---

## Builds

The Build tab streams Gradle output and highlights structured errors.

Common actions:

- `Cmd+R`: build, install, and launch the app.
- `Cmd+Shift+R`: build only.
- `Cmd+Shift+V`: choose the active build variant.
- **Clean Project** from Command Palette: run `./gradlew clean`.
- **Cancel Build** from Command Palette: stop the running Gradle task.

Build output has two views:

- **Log**: raw Gradle output with ANSI colors.
- **Problems**: parsed errors and warnings.

The active device and active variant are used automatically. Click the variant pill in the status bar to change variants.

---

## Logcat

The Logcat tab streams logs from the selected Android device.

### Controls

- **Start / Stop**: begin or end streaming.
- **Pause / Resume**: pause display updates without losing buffered data.
- **Jump to end**: resume follow-tail and clear row/detail selection.
- **Clear**: clear displayed entries and the in-memory buffer.
- **Export**: save filtered entries to a `.log` file.
- **Filters**: use quick filters or saved filters.
- **Package dropdown**: show all packages, only your app, or a package seen in this session.

### Filters

Use the query bar for targeted searches:

| Query | Meaning |
|-------|---------|
| `level:error` | error and fatal logs |
| `tag:OkHttp` | tag contains `OkHttp` |
| `message:timeout` | message contains `timeout` |
| `package:mine` | current app package |
| `package:com.example` | specific package |
| `age:5m` | last five minutes |
| `is:crash` | crash entries only |
| `-tag:system` | exclude matching tag |
| `level:warn tag:MyApp` | AND search |
| `level:error | is:crash` | OR search |

Press **Enter** to commit a typed condition as a filter pill. Use **+ AND** and **+ OR** for compound filters. Saved filters are capped at 50.

### Reading logs

- Crash entries are highlighted and grouped.
- ANRs get an ANR badge.
- JSON messages show a `{}` badge; open it to view formatted JSON.
- Arrow keys move the selected row when focus is not in the query bar.
- Selecting a row pauses follow-tail until you jump back to the end.
- In **Entry Detail**, click a tag, package, level, PID, TID, time, or message value to add it to the query bar as an **AND** or **OR** filter. Select part of the message before clicking to filter by only that selected text.

Logcat keeps a bounded ring buffer. Configure the ring size and max visible lines under **Settings -> Logcat**.

---

## Layout Viewer

The Layout tab captures the current UI hierarchy from the selected device using UI Automator. It shows the same accessibility surface used by TalkBack and automation tools, including Jetpack Compose semantics.

Basic flow:

1. Select an online device.
2. Open **Layout** with `Cmd+4`.
3. Click **Refresh**.
4. Click a wireframe region or tree row to inspect details.

Useful controls:

- **Interactive only**: focus on actionable nodes.
- **Hide boilerplate**: collapse wrapper chains.
- **Filter**: search class, resource id, text, content description, or package.
- **Prev / Next**: move between matches.
- **Expand all / Collapse all**: control tree disclosure.
- **Find parent**: jump from the selected row to its direct parent.

The panel also shows capture time, screen hash, interactive node count, parser warnings, foreground activity when available, and the ADB commands used for the capture.

Notes:

- The Layout tab is a snapshot, not a live video.
- Compose output is the merged semantics tree, not every composable or modifier.
- `FLAG_SECURE` screens may hide or obscure content.
- For full Compose internals, use Android Studio Layout Inspector on debuggable builds.

---

## Devices

The Devices sidebar lists physical devices and emulators. Toggle it with `Cmd+3`.

Physical devices show serial, model, API level, and connection state. Click a row to make it the active deploy target.

Emulators can be launched and stopped from the sidebar. Running emulators also appear in the connected-device list once ADB sees them.

If no suitable device is selected during a run, Keynobi may ask you to choose one.

---

## Settings and Health

Open Settings with `Cmd+,` or the gear icon.

Important settings:

- **Android SDK Path**: required for ADB, emulator support, and health checks.
- **Java Home**: required for Gradle builds.
- **Logcat auto-start** and **follow-tail** behavior.
- **Logcat ring buffer size** and visible line cap.
- **Build log follow-tail** behavior.
- **Anonymous crash reporting**.

Open Health Center with `Cmd+Shift+H` or the Health status item.

Health checks include:

- Android SDK
- ADB
- Emulator
- Java / JDK
- Gradle wrapper
- Disk space
- App data directory
- MCP setup command

---

## Claude Code MCP

Keynobi includes an MCP server so Claude Code can use real app state instead of guessing.

Claude can:

- Run Gradle tasks and read structured build errors.
- Read logcat and crash logs.
- Inspect devices and app runtime state.
- Install, launch, stop, and restart apps.
- Inspect the UI hierarchy and perform UI automation.
- Run health checks and query project information.

### Recommended setup

Use the command copied from **Health Center** or **Copy MCP Setup Command** in the Command Palette. It includes the correct local app path.

Typical command:

```bash
claude mcp add --transport stdio keynobi -- "/Applications/Keynobi.app/Contents/MacOS/keynobi" --mcp
```

To bind MCP to a specific Android project:

```bash
claude mcp add --transport stdio keynobi -- "/Applications/Keynobi.app/Contents/MacOS/keynobi" --mcp --project /path/to/MyAndroidProject
```

Exact tools, prompts, and resources are discoverable from the MCP client. In Keynobi, the MCP Activity panel shows setup status and recent tool activity.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+P` | Command Palette |
| `Cmd+Shift+W` | Setup Wizard |
| `Cmd+,` | Settings |
| `Cmd+O` | Open / add project folder |
| `Cmd+R` | Run App: build, install, launch |
| `Cmd+Shift+R` | Build only |
| `Cmd+Shift+V` | Select build variant |
| `Cmd+1` | Build tab |
| `Cmd+2` | Logcat tab |
| `Cmd+3` | Toggle Devices sidebar |
| `Cmd+4` | Layout tab |
| `Cmd+B` | Toggle Projects sidebar |
| `Cmd+Shift+H` | Health Center |
| `Cmd+Shift+M` | MCP Activity panel |

Useful Command Palette actions:

- Open Setup Wizard
- Project App Info
- Open Folder
- Cancel Build
- Clean Project
- Manage Virtual Devices
- Copy MCP Setup Command

---

## Troubleshooting

### No devices appear

- Confirm `adb devices` works in a terminal.
- Check Android SDK Path in Settings.
- For USB devices, confirm USB debugging is enabled and trusted.

### Build fails immediately

- Open Health Center and check Java, SDK, Gradle wrapper, and disk space.
- Confirm the selected project has a `gradlew` wrapper.
- Try **Clean Project** from the Command Palette.

### Logcat is empty

- Select an online device.
- Press **Start** in the Logcat toolbar.
- Clear restrictive filters such as package, age, or crash-only.

### Layout capture fails

- Confirm the device is online and unlocked.
- Open the screen you want to inspect, then click **Refresh**.
- Some secure screens or OS states may return partial or empty dumps.

### MCP cannot connect

- Copy the setup command from Health Center again.
- Confirm the app path in the command exists.
- If using `--project`, confirm the folder exists and contains the Android project.
