# Keynobi User Guide

Keynobi is a macOS companion app for Android development. It sits next to Android Studio and gives you one place for Gradle builds, logcat, devices, app health, UI hierarchy inspection, and AI client (MCP) workflows. It does not replace Android Studio: it does not edit code.

Use this guide as a quick reference. Most commands are also available from the Command Palette with `Cmd+Shift+P`.

---

## Contents

1. [Install and Update](#install-and-update)
2. [Quick Start](#quick-start)
3. [Window Overview](#window-overview)
4. [Projects](#projects)
5. [Builds](#builds)
6. [Logcat](#logcat)
7. [Layout Viewer](#layout-viewer)
8. [Devices](#devices)
9. [Settings and Health](#settings-and-health)
10. [AI Client MCP](#ai-client-mcp)
11. [Keyboard Shortcuts](#keyboard-shortcuts)
12. [Privacy and Data](#privacy-and-data)
13. [Troubleshooting](#troubleshooting)
14. [Reporting a Problem](#reporting-a-problem)

---

## Install and Update

### Requirements

- macOS 12 Monterey or later, on Apple Silicon or Intel.
- Android SDK with platform-tools (`adb`). The emulator package is needed for virtual devices.
- A JDK supported by your project's Android Gradle Plugin (JDK 17 or newer for AGP 8).
- An Android project with a `gradlew` wrapper.
- Optional: Android Studio's `studio` command-line launcher, to open stack-trace lines in Android Studio.

### Install

1. Download the DMG for your Mac (Apple Silicon or Intel) from the [GitHub releases page](https://github.com/thiagodmont/keynobi/releases).
2. Open the DMG and drag **Keynobi** to **Applications**.
3. Launch Keynobi from **Applications**. Release builds are signed and notarized by Apple.

Always run Keynobi from `/Applications` before you set up an AI client. The setup command records the app's current location, and a path inside a mounted DMG stops working after you eject it.

### Update

Keynobi checks GitHub for a newer release each time it starts. When one exists, a **New Keynobi Version Available** dialog offers **Download** or **Later**, and an **Update** button stays in the status bar. **Download** opens the release page; install the new DMG the same way you installed the first one. **Later** hides the dialog for that version.

After updating, restart your AI clients so they use the new app binary for MCP.

---

## Quick Start

### First launch

1. Open **Keynobi**.
2. Complete the setup wizard (**Welcome → Environment → Privacy → Workflow → Summary**), or choose **Skip setup** and finish later in **Settings**.
3. Set your **Android SDK Path** and **JAVA_HOME** if auto-detect does not find them.
4. Press `Cmd+O` or click **Add Project…** and choose your Android project folder. The first time you open a project, choose **Trust** so Keynobi can run its Gradle build (see [Project trust and Safe Mode](#project-trust-and-safe-mode)).
5. Select a device or start an emulator.
6. Press `Cmd+R` to build, install, and launch.

If no device is selected when you run, Keynobi asks you to pick one. Keynobi restores the last active project on later launches.

---

## Window Overview

Keynobi has three main tabs, shown in this order: **Logcat**, **Layout**, and **Build**.

- **Logcat**: live Android logs with filtering and crash detection.
- **Layout**: UI Automator accessibility tree and wireframe for the selected device.
- **Build**: Gradle output, build history, and structured errors.

Sidebars and bars:

- **Projects sidebar** on the left: saved Android projects.
- **Devices sidebar** on the right: physical devices and emulators.
- **Title bar**: **Run App** (becomes **Cancel build** while Gradle builds; install and launch cannot be cancelled), **Log Mode** to focus the window on Logcat, and **Keep window on top** to keep Keynobi above other apps. Keep on top resets when you relaunch.
- **Status bar** at the bottom: settings, project, health, build status, MCP status, update (when available), active variant, app memory, and log folder size.

---

## Projects

Use the **Projects** sidebar to add, switch, rename, or remove saved projects.

- `Cmd+O`: add or open a project folder.
- `Cmd+B`: collapse or expand the Projects sidebar.
- Click a project row to switch projects.
- **Rename** changes only the label in Keynobi; it does not rename the folder. Press Enter to save or Esc to cancel.
- **Remove from list** deletes the saved entry only; it does not delete files on disk.
- Right-click a project row for **Trust Project** or **Revoke Trust**, and **Remove from List**.

### Project trust and Safe Mode

Building a project, and detecting its build variants, runs the project's `gradlew` and Gradle build scripts, which can run any code. The first time you open a project, Keynobi asks: "This project will run its Gradle build scripts. Trust it?"

- **Trust**: Keynobi detects variants with Gradle and you can build and run.
- **Open in Safe Mode**: Keynobi runs none of the project's build code. Build variants are read from the build files only, and **Run App**, **Build Only**, `Cmd+R`, `Cmd+Shift+R`, and **Clean Project** are disabled. Health Center ignores the project's `org.gradle.java.home` and `local.properties` SDK path.

If you close the question without choosing, the project stays in Safe Mode and Keynobi asks again next time you open it. A **Safe Mode** badge shows on the project row and in the title bar; click the title bar badge, or **Trust Project…** in the Build tab, to trust the project. To stop trusting a project, right-click it and choose **Revoke Trust**; a running build of the open project is cancelled.

Keynobi remembers your choice for each project folder. Projects you had already added before this question existed are trusted. Removing a project from the list, or the list dropping it when it passes 20 projects, forgets the choice.

### Project App Info

Open **Project App Info** from the Command Palette to edit **Version Name** and **Version Code**. **Application ID** is shown read-only.

Edits are written to `app/build.gradle.kts` or `app/build.gradle`. Projects whose application module is not named `app`, or that set versions in `gradle.properties` or a version catalog, are not supported. Change those in Android Studio.

---

## Builds

The Build tab streams Gradle output and highlights structured errors.

Common actions:

Builds need a trusted project; in Safe Mode every build action is disabled (see [Project trust and Safe Mode](#project-trust-and-safe-mode)).

- `Cmd+R` or **Run App**: build, install, and launch the app on the selected device.
- `Cmd+Shift+R`: build only.
- `Cmd+Shift+V`, or click the variant pill in the status bar: choose the active build variant.
- **Clean Project** from the Command Palette: run the Gradle `clean` task.
- **Cancel Build** from the Command Palette or the title bar: stop the running Gradle task.

Build output has two views:

- **Log**: Gradle output, colored by level. Filter by level (**ALL**, **ERR**, **WARN**, **INFO**, **DBG**), by source, or by text; show or hide timestamps; copy the visible lines; or clear the view.
- **Problems (N)**: parsed errors and warnings.

The **Builds** side panel lists recent builds. Click one to see its log, or use **Clear build history**. If a past build's log cannot be read, the panel shows the error with a **Retry** button instead of the log. Keynobi keeps the last 10 builds; log files are removed after 7 days or when the log folder passes 100 MB (both configurable under **Settings → Advanced → Build**).

---

## Logcat

The Logcat tab streams logs from the selected Android device. Streaming starts at the moment you press **Start**; earlier device logs are not loaded.

Use **Log Mode** from the title bar when logs are the main task. It hides the project sidebar, device sidebar, and main tab strip while keeping the Logcat toolbar, filters, query bar, and bottom status bar visible. Turning Log Mode off restores the layout you had before.

### Toolbar

- **Start / Stop**: begin or end streaming.
- **Pause / Resume**: pause display updates without losing captured entries.
- **Restart**: stop, clear, and start logcat again.
- **Clear**: clear Keynobi's log buffer. The device's own logcat buffer is not changed.
- **Crash counter** with **Previous** / **Next**: jump between crashes.
- **Copy N rows**: appears when you select rows (Shift+click selects a range).
- **Jump to end**: resume follow-tail, apply any logs that arrived while you were reading, and clear row/detail selection. Shows how many new entries arrived.
- **Export**: save the rows currently displayed to a `.log` or `.txt` file.

If the device connection drops, Keynobi reconnects automatically. If it cannot reconnect, it stops and shows a **Logcat stopped** message.

With **Auto-start on Connect** on, logcat starts when a device comes online: on the selected device if it is online, otherwise on the first online device.

### Filter bar

- **Age**: 30s, 1m, 5m, 15m, 1h, or All.
- **Lifecycle**: show or hide lifecycle/process entries such as ActivityManager rows and process separators. Hidden entries are still captured.
- **Package dropdown**: **All packages**, **My App** (`package:mine`), or a package seen in this session.
- **Clear**: remove all filters.
- **Filters**: quick filters and your saved filters.

### Query syntax

Type in the query bar and press **Enter** to commit a condition as a filter pill.

| Query | Meaning |
|-------|---------|
| `level:warn` or `warn` | This level and above. Levels: `verbose`/`v`, `debug`/`d`, `info`/`i`, `warn`/`w`, `error`/`e`, `fatal`/`f`/`assert`/`a` |
| `tag:OkHttp` | Tag contains `OkHttp` (case-insensitive) |
| `tag~:^Ok.*Http$` | Tag matches a regular expression |
| `message:timeout` or `msg:timeout` | Message contains `timeout` |
| `message~:time(out|d)` | Message matches a regular expression |
| `package:com.example` or `pkg:com.example` | Package contains `com.example` |
| `package:mine` | Your app's application ID |
| `pid:1234`, `tid:5678` | Exact process or thread ID |
| `age:5m` | Last five minutes (`s`, `m`, `h`, `d`; decimals allowed) |
| `is:crash` | Crash entries only |
| `is:stacktrace` | Stack-trace lines |
| `timeout` | Plain text searches tag, message, and package |
| `message:"connection reset"` | Quote values that contain spaces; `\"` for a literal quote |
| `-tag:system` | Exclude matches (works on every key except `age:` and `is:`) |
| `level:warn tag:MyApp` | AND (spaces, or `&`) |
| `level:error | is:crash` | OR |
| `message:action_${action_name}_done` | Uses the value of the `action_name` variable |

An invalid regular expression is treated as plain text.

### Working with filters

- Use **+ AND** and **+ OR** to build compound filters. Click an `AND` or `OR` connector badge between pills to toggle just that connector.
- Click the power icon on a pill to temporarily disable or re-enable it without removing it. Disabled state resets when the app restarts.
- Press Backspace in an empty query bar to remove the last pill. Press Esc to close suggestions, and Esc again to clear the query.
- When a filter is active, click **Save** beside the query bar to name it and reuse it later from **Filters**. You can keep up to 50 saved filters.
- Use `${name}` in a value to create a filter variable. Click **Variables** to create, edit, insert, or delete variables. Variables also appear in a compact row under the query bar. Values reset when the app restarts; saved filters keep the `${name}` template.
- Your last query is restored when you reopen Keynobi.

### Reading logs

- Crash entries are highlighted and grouped. ANRs get an ANR badge.
- Stack-trace lines from your project are clickable and open the file in Android Studio (requires the `studio` command-line launcher).
- JSON messages show a `{}` badge; open it to view formatted JSON.
- Up/Down arrows move the selected row when focus is not in the query bar. Esc closes the context menu and Entry Detail.
- When a filter is active, right-click a row and choose **Expand 10 up** or **Expand 10 down** to reveal adjacent rows without clearing the filter. Rows added this way have a green left marker.
- Scrolling away from the end or selecting a row enters read mode: the visible list is frozen so the row you are reading cannot be pushed out. New logs are still captured; **Jump to end** applies them.
- In **Entry Detail**, click a tag, package, level, PID, TID, time, or message value to add it to the query as an **AND** or **OR** filter. Select part of the message first to filter by only that text. Use **Copy** to copy the entry.

Logcat keeps a bounded buffer in memory. Configure it under **Settings → Tools → Logcat**: **Ring buffer size**, **Max lines in Logcat**, **Logcat Output Font Size**, **Auto-start on Connect**, and **Auto-scroll Logcat to end**.

---

## Layout Viewer

The Layout tab captures the current UI hierarchy from the selected device using UI Automator. It shows the same accessibility surface used by TalkBack and automation tools, including Jetpack Compose semantics.

Basic flow:

1. Select an online device.
2. Open **Layout** with `Cmd+4`.
3. Click **Refresh**, or choose an auto-refresh interval (**2s**, **5s**, or **10s**).
4. Click a wireframe region or tree row to inspect details.

Useful controls:

- **Interactive only**: focus on actionable nodes.
- **Hide boilerplate**: collapse wrapper chains. A warning appears because tree paths then differ from the paths AI clients use.
- **Filter**: search class, resource id, text, content description, or package.
- **Prev / Next**: move between matches.
- **Expand all / Collapse all**: control tree disclosure.
- **Find parent**: jump from the selected row to its direct parent.
- **Copy bounds**, **Copy id**, **Copy summary**: copy details of the selected node.

The panel also shows capture time, screen hash, interactive node count, parser warnings, foreground activity when available, and the ADB commands used for the capture.

Notes:

- Each capture is a snapshot. Auto-refresh repeats the capture; it is not a live video.
- Compose output is the merged semantics tree, not every composable or modifier.
- `FLAG_SECURE` screens may hide or obscure content.
- For full Compose internals, use Android Studio Layout Inspector on debuggable builds.

---

## Devices

The Devices sidebar lists physical devices and emulators. Toggle it with `Cmd+3`.

Each device shows its model or name, whether it is an emulator or physical device, its API level, and a colored status dot. A check mark shows the active deploy target; click a row to make it active. Use **Refresh** to rescan devices.

Virtual devices:

- Launch an emulator from its row. Hover over a running emulator and click stop to shut it down.
- **New Virtual Device** creates an AVD, and can download a system image if needed.
- **More options** on an AVD offers **Wipe Data…** and **Delete…**.

Wireless debugging pairing is not built in. Pair with `adb pair` and `adb connect` in a terminal; the device then appears in the list.

---

## Settings and Health

Open Settings with `Cmd+,` or the gear icon. Use the search box to find a setting, or **Reset to Defaults** to start over.

| Category | Section | Settings |
|----------|---------|----------|
| User | Appearance | System Font Size |
| User | Search | Context Lines, Max Results, Max Files |
| Tools | Android SDK | SDK Path (with Auto-detect) |
| Tools | Java / JDK | JAVA_HOME (with Auto-detect) |
| Tools | Logcat | Auto-start on Connect, Auto-scroll Logcat to end, Logcat Output Font Size, Ring buffer size, Max lines in Logcat |
| Tools | MCP | Build Timeout (seconds), Default Logcat Count, Default Build Log Lines, Allow unrestricted Gradle tasks (off by default: AI clients cannot run publish, upload, or uninstall tasks) |
| Advanced | Build | Auto Install on Build, Auto-scroll build log to end, Build log retention (days), Build log folder limit (MB) |
| Advanced | Logging | Log retention, Max log folder size (MB) |
| Advanced | Privacy | Anonymous crash reporting |

Some settings have no effect yet: everything under **User → Search** and **Auto Install on Build**.

Open Health Center with `Cmd+Shift+H` or the Health status item. It checks:

- Android SDK
- ADB
- Android Emulator
- Android Studio CLI (`studio`)
- Java / JDK
- App Data Directory

The Java / JDK check shows the JDK Gradle builds use, its version, and where it was found. Keynobi picks it in this order:

1. `org.gradle.java.home` in `~/.gradle/gradle.properties`, then in the project's `gradle.properties` (the same rule Gradle follows). The project's file is skipped while the project is in Safe Mode.
2. **JAVA_HOME** in Settings.
3. The JDK bundled with Android Studio (including Android Studio Preview).
4. The newest JDK 17 or later in `/Library/Java/JavaVirtualMachines`.

Builds and the MCP server use the same JDK. The check is an error when Java does not run (for example, only the macOS `java` placeholder is installed), and a warning when the JDK is older than 17, which Android Gradle Plugin 8 and newer need.

Health Center also shows a logcat buffer warning when relevant, and the **AI Client Integration (MCP)** section with setup commands.

---

## AI Client MCP

Keynobi includes an MCP server so Claude Code, Codex, and other MCP clients can use real project and device state instead of guessing.

AI clients can:

- Run Gradle tasks and read structured build errors.
- Read logcat and crash logs.
- Inspect devices and app runtime state.
- Install, launch, stop, and restart apps.
- Inspect the UI hierarchy and drive the device UI: tap, type, swipe, scroll, press keys, open deep links, rotate, toggle network, and grant or revoke permissions.
- Run health checks and query project information.

### Recommended setup

Copy the command from **Health Center** or **Copy MCP Setup Commands** in the Command Palette. It includes the correct app path.

Claude Code:

```bash
claude mcp add --scope user --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

`--scope user` makes Keynobi available in every folder. Without it, Claude Code registers the server only for the folder where you ran the command. The copied command does not include `--scope user` yet; add it yourself.

Codex:

```bash
codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

To bind MCP to a specific Android project, append `--project /path/to/MyAndroidProject` to either command. Existing registrations keep working after updating Keynobi; nothing needs to change.

### How MCP relates to the app

Your AI client starts a small Keynobi MCP process in the background. If Keynobi is open, that process **attaches** to the app: the AI client then works on the app's project, devices, logcat, and build slot. If it cannot attach, it runs **standalone** with its own state, and its builds and logcat are not visible in the app. It never opens Keynobi for you.

- **Project**: the MCP process asks for the project given with `--project`, or else the Android project that contains the AI client's working folder (a folder with `settings.gradle` or `settings.gradle.kts`, or a parent of it).
  - If Keynobi has that project open, the client attaches and stays on that project. If you later switch Keynobi to another project, the client's project tools (builds, variants, installing, stopping apps) return an error naming both projects until you switch back or restart the MCP server in your AI client. Device, UI, and logcat tools keep working.
  - If the client did not ask for a project (no `--project`, and its folder is not inside an Android project), it attaches and follows whatever project Keynobi has open.
  - If Keynobi has a different project open, or no project, or is not running, the MCP process runs standalone on its own project (falling back to the last project you had open in Keynobi). Keynobi never switches projects for an AI client.
- **Which mode**: ask the client to call `get_project_info`. `mode` is `attached` or `standalone`, `standalone_reason` says why, and `selected_by` says how the project was chosen. Build results also say which mode ran them.
- **Trust**: an AI client can build only a project you trusted in Keynobi. For any other project, `run_gradle_task` and `run_tests` fail with a message asking you to open the project in Keynobi and choose **Trust**; the MCP server never asks itself. Other tools keep working.
- **Builds and logcat**: an attached client shares one build at a time with the app: while either is building, the other is told a build is already running. Builds started by an attached client do not stream into the Build tab yet. Builds and logcat of a standalone server are not visible in the app; its builds can appear in build history the next time Keynobi starts.
- **Quitting Keynobi** ends attached sessions; restart the MCP server in your AI client afterwards.
- **Status**: the MCP item in the status bar shows how many AI clients are attached (for example **MCP: 2 agents**) and warns about standalone servers. The **MCP Activity** panel (`Cmd+Shift+M`) lists each session, the setup commands, and recent tool calls from AI clients.

To require the app, add `--attach-only` after `--mcp`: the MCP server then exits with an error instead of running standalone.

AI clients can change your device. `restart_app` keeps the app's data unless the client explicitly passes `clear_data: true` for a specific device. Stopping or restarting an app and granting or revoking its permissions work only on your project's app (its application ID and variants such as `.debug`) unless the client passes `allow_foreign_package: true`. Keynobi refuses to turn off Wi-Fi or turn on airplane mode on a device connected over wireless debugging, because that would disconnect it. Review what your AI client asks to run.

Exact tools, prompts, and resources are discoverable from the MCP client.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+Shift+P` | Command Palette |
| `Cmd+Shift+W` | Open Setup Wizard |
| `Cmd+,` | Open Settings |
| `Cmd+O` | Open Folder (add project) |
| `Cmd+R` | Run App: build, install, launch |
| `Cmd+Shift+R` | Build Only (no deploy) |
| `Cmd+Shift+V` | Select Build Variant |
| `Cmd+1` | Build tab |
| `Cmd+2` | Logcat tab |
| `Cmd+3` | Toggle Devices sidebar |
| `Cmd+4` | Layout tab |
| `Cmd+B` | Toggle Projects sidebar |
| `Cmd+Shift+H` | Health Center |
| `Cmd+Shift+M` | MCP Activity panel |

Command Palette actions without a shortcut:

- Toggle Log Mode
- Project App Info
- Cancel Build
- Clean Project
- Manage Virtual Devices (toggles the Devices sidebar, same as `Cmd+3`)
- Copy MCP Setup Commands

In the Logcat query bar: **Enter** commits a pill, **Up/Down** and **Tab** work with suggestions, **Backspace** in an empty bar removes the last pill, and **Esc** closes suggestions, then clears the query.

---

## Privacy and Data

### Network use

Keynobi makes these network requests on its own:

- **Update check**: on every launch, a request to `api.github.com` for the latest Keynobi release. No project or device data is sent.
- **Crash reporting**: only if you turn it on.

Anonymous crash reporting is off by default. Turn it on under **Settings → Advanced → Privacy**; the native part starts after a restart. Turning it off takes effect immediately, without a restart. Reports contain only the error type, the app version, CPU architecture, macOS version, the code locations of the error inside Keynobi (function, source file within the app, line), and, for failed app commands, the error category (for example `io` or `notFound`). They never contain error messages, file paths on your Mac, source code, project files, Gradle output, logcat, MCP traffic, package names, personal identifiers, or device identifiers. As with any network request, the crash-reporting server can see your IP address; Keynobi does not put it in reports.

### Where Keynobi stores data

| Location | Contents |
|----------|----------|
| `~/.keynobi/settings.json` | Settings and saved projects |
| `~/.keynobi/logs/` | Keynobi's own app logs |
| `~/.keynobi/build-history.json`, `~/.keynobi/build-logs/` | Build history and build logs |
| `~/.keynobi/mcp-activity.jsonl` | Recent AI client activity |
| `~/.keynobi/mcp.sock`, `~/.keynobi/mcp-sessions/` | The socket AI clients attach through while Keynobi is open, and a record per standalone MCP server |
| `~/Library/WebKit/com.keynobi.app` | Saved logcat filters, last query, and dismissed updates |

### Reset or uninstall

- Reset settings: **Settings → Reset to Defaults**.
- Full reset: quit Keynobi and delete `~/.keynobi`.
- Uninstall: quit Keynobi, delete it from **Applications**, then delete `~/.keynobi` and `~/Library/WebKit/com.keynobi.app`. Remove the MCP server from your AI clients (`claude mcp remove keynobi`, `codex mcp remove keynobi`).

---

## Troubleshooting

### No devices appear

- Confirm `adb devices` works in a terminal.
- Check **Android SDK Path** in Settings and the ADB item in Health Center.
- For USB devices, confirm USB debugging is enabled and the computer is trusted on the device.
- Click **Refresh** in the Devices sidebar.

### Build fails immediately

- Open Health Center and check Java / JDK and Android SDK.
- If the Build tab shows **Safe Mode**, trust the project first.
- Confirm the project has a `gradlew` wrapper and that the JDK shown in Health Center is one your Android Gradle Plugin supports. `org.gradle.java.home` in `gradle.properties` takes precedence over **JAVA_HOME** in Settings.
- Try **Clean Project** from the Command Palette.

### Logcat is empty

- Select an online device and press **Start**.
- Logcat shows only logs written after you pressed Start. Trigger the behavior again.
- Clear restrictive filters such as package, age, or crash-only (**Clear** in the filter bar).

### Logcat stopped

- Keynobi retries automatically when the device connection drops. If you see **Logcat stopped**, check the device connection (`adb devices`) and press **Start**.

### Stack-trace lines do not open in Android Studio

- Open Health Center and follow the Android Studio CLI steps to put the `studio` command on your PATH (in Android Studio: **Tools → Create Command-line Launcher**).

### Layout capture fails

- Confirm the device is online and unlocked.
- Open the screen you want to inspect, then click **Refresh**.
- Some secure screens or OS states may return partial or empty dumps.

### MCP cannot connect

- Copy the setup command again from Health Center.
- Confirm the app path in the command exists. It must be in `/Applications`, not inside a mounted DMG.
- In Claude Code, run `claude mcp list` from your project folder. If Keynobi is missing there, re-add it with `--scope user`.
- If using `--project`, confirm the folder exists and contains the Android project.
- With `--attach-only`, the MCP server exits unless Keynobi is open with the same project; the AI client's MCP log shows why.

### MCP works on the wrong project

- The MCP server picks its project when your AI client starts it. Ask the client to call `get_project_info`: `selected_by` says whether the project came from `--project` (`argument`), the client's working folder (`working_directory`), the project open in Keynobi (`app`), or the last project open in Keynobi (`last_active_project`), and `mode`/`standalone_reason` say whether it attached to the app. Open the right project in Keynobi and restart the MCP server from your AI client, or add `--project /path/to/project` to the setup command.

### MCP builds fail with "This project is not trusted"

- Open the project in Keynobi and choose **Trust**, or right-click it in the Projects sidebar and choose **Trust Project**. Then ask the AI client to build again; the MCP server does not need a restart.

---

## Reporting a Problem

Open an issue on [GitHub](https://github.com/thiagodmont/keynobi/issues) with:

- Keynobi version, macOS version, and Mac type (Apple Silicon or Intel).
- Steps to reproduce, and what you expected.
- Relevant lines from `~/.keynobi/logs/`. Check them for private information first.

Report security vulnerabilities privately as described in `SECURITY.md`, not in a public issue.
