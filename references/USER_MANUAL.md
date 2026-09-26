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

Always run Keynobi from `/Applications` before you set up an AI client. The setup command records the app's current location, and a path inside a mounted DMG stops working after you eject it. While Keynobi runs from a disk image or from a temporary copy macOS made (App Translocation, which happens when you open the app straight from the Downloads folder or a DMG), Health Center shows an **App Location** warning and the setup commands are not offered.

### Update

Keynobi checks GitHub for a newer release each time it starts. When one exists, a **New Keynobi Version Available** dialog offers **Download** or **Later**, and an **Update** button stays in the status bar. **Download** opens the release page; install the new DMG the same way you installed the first one. **Later** hides the dialog for that version.

After updating, restart your AI clients so they use the new app binary for MCP. Until you do, the MCP status item turns yellow and the **MCP Activity** panel names the servers still running the old version.

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
- Click a project row to switch projects. From the keyboard, Tab to the list, move with Up/Down, and press Enter or Space.
- **Rename** changes only the label in Keynobi; it does not rename the folder. Press Enter to save or Esc to cancel.
- **Remove from list** deletes the saved entry only; it does not delete files on disk.
- Right-click a project row, or press Shift+F10 on it, for **Rename…**, **Trust Project** or **Revoke Trust**, and **Remove from List**.

### Project trust and Safe Mode

Building a project, and detecting its build variants, runs the project's `gradlew` and Gradle build scripts, which can run any code. The first time you open a project, Keynobi asks: "This project will run its Gradle build scripts. Trust it?"

- **Trust**: Keynobi detects variants with Gradle and you can build and run.
- **Open in Safe Mode**: Keynobi runs none of the project's build code. Build variants are read from the build files only, and **Run App**, **Build Only**, `Cmd+R`, `Cmd+Shift+R`, and **Clean Project** are disabled. Health Center ignores the project's `org.gradle.java.home` and `local.properties` SDK path.

If you close the question without choosing, the project stays in Safe Mode and Keynobi asks again next time you open it. A **Safe Mode** badge shows on the project row and in the title bar; click the title bar badge, or **Trust Project…** in the Build tab, to trust the project. To stop trusting a project, right-click it and choose **Revoke Trust**; a running build of the open project is cancelled.

Keynobi remembers your choice for each project folder. Projects you had already added before this question existed are trusted. Removing a project from the list, or the list dropping it when it passes 20 projects, forgets the choice.

### Project App Info

Open **Project App Info** from the Command Palette to edit **Version Name** and **Version Code**. **Application ID** is shown read-only.

Edits are written to the `build.gradle.kts` or `build.gradle` of the project's application module (the module that applies the Android application plugin, whatever its name). Commented-out lines are ignored. Version Code must be a whole number from 1 to 2100000000.

A field is shown read-only, with the reason, when Keynobi cannot edit it safely:

- It is set by an expression, such as `libs.versions.code.get().toInt()` or `project.property("appVersion")`. The note gives the line and where the value is probably defined (the version catalog or `gradle.properties`); change it there.
- It is set in more than one place, for example once per product flavor. The note lists the lines; edit the file directly.
- It is not set in the build file.

You can still save the other field. If saving fails, or the file already has these values, the reason is shown in the dialog and the file is not changed. Saving keeps the file's permissions. A build file (or module folder) that is a symlink leading outside the project is not read or changed. Projects with several application modules are not supported.

---

## Builds

The Build tab streams Gradle output and highlights structured errors.

Common actions:

Builds need a trusted project; in Safe Mode every build action is disabled (see [Project trust and Safe Mode](#project-trust-and-safe-mode)).

- `Cmd+R` or **Run App**: build, install, and launch the app on the selected device. Run App builds only the project's application module, whatever its name (`:mobile:assembleDebug`, or `assembleDebug` when the app is the root project); in a project with several application modules (for example a phone and a watch app), Run App stops with the list of modules. It installs the APK that build wrote for the selected variant, and the log names the build: **APK (build #12)**. When Gradle found the APK up to date and did not rewrite it, the log says **APK unchanged since build #9** (the build that wrote it); an APK of another variant or module is never installed. The build log ends with the launch time Android measured (`am start -W`), for example **Launch time: 812 ms (cold) · displayed 790 ms**; the display times appear when Logcat is streaming that device (see below). When Keynobi had to fall back to another way of starting the app, the log says no launch time was reported.
- `Cmd+Shift+R`: build only.
- `Cmd+Shift+V`, or click the variant pill in the status bar: choose the active build variant.
- **Clean Project** from the Command Palette: run the Gradle `clean` task.
- **Cancel Build** from the Command Palette or the title bar: stop the running Gradle task, including one an AI client started.

Builds an attached AI client starts show in the Build tab like your own, labelled **Started by an agent (client name)**, with their output, errors, and result. Keynobi never installs or launches an AI client's build. While it runs, **Build** is disabled and its tooltip says which agent is building; you can still cancel it.

Build output has two views:

- **Log**: Gradle output, colored by level. Filter by level (**ALL**, **ERR**, **WARN**, **INFO**, **DBG**), by source, or by text; show or hide timestamps; copy the visible lines; or clear the view.
- **Problems (N)**: parsed errors and warnings.

The **Builds** side panel lists recent builds, with who started a build when it was an AI client, and who cancelled it (you, an AI client, or Keynobi quitting). A build that **Run App** installed and launched also shows its launch time and how Android started the app (**cold**: a new process, **warm**: the process was running, **hot**: the app was brought back to the front), for example **Launch 812 ms (cold) · displayed 790 ms · fully drawn 1.4 s · +54 ms vs #41**. The change compares it with the most recent earlier build of the same task that launched in the same way on the same device (an emulator counts as the same device when it runs the same AVD); a **+** means slower and a **−** faster. With no such build there is no comparison. Launch times are measured by Android from the start request until the app draws its first frame; they are not recorded for launches by AI clients. When the Logcat panel is streaming the device the app launched on, Keynobi also records the times Android logs: **displayed**, the time to initial display (the `Displayed` line), and **fully drawn**, the time until the app called `reportFullyDrawn()` (the `Fully drawn` line), if it does so within 10 seconds. The fully drawn time can appear a few seconds after the launch. Without a Logcat stream on that device, or without the line, they are not shown. The comparison uses the launch time only. Use **Clear build history** to empty it. Keynobi keeps the last 10 builds; log files are removed after 7 days or when the log folder passes 100 MB (both configurable under **Settings → Advanced → Build**).

Select a past build (click it, or Tab to the list, move with Up/Down, and press Enter) to see it instead of the current build. The whole Build tab then describes that build: its log, **Problems**, result and duration, and who started or cancelled it. A bar above the log says **Viewing build #N from** its start time, with **Back to current build**.

- A build you start (**Run App**, **Build Only**, `Cmd+R`, `Cmd+Shift+R`) brings the tab back to the current build.
- A build an AI client starts does not replace the build you are reading. The bar says a build is running and offers **Show running build**.
- While a past build's log loads, the tab says so. If the log was removed by the retention settings above, the tab says **This build's log was removed**; its problems and result are still shown. If the log cannot be read, the tab shows the error with a **Retry** button. A build that has dropped out of the last 10 is reported as no longer in the history.
- **R8 mapping saved: release (map id 6b1c2f0)** means the build shrank and obfuscated the app with R8, and Keynobi kept a copy of the `mapping.txt` it wrote for that variant. The next build of the variant overwrites the project's file; the copy keeps the mapping that matches this build's APK, which is what turns an obfuscated crash from that APK back into class and method names. The map id is the `pg_map_id` R8 writes at the top of the mapping (not every version writes one); the tooltip adds the module, the start of the file's SHA-256, and its size. The line appears only for a successful build that wrote a mapping during that build: a build that reused the previous mapping without rewriting it (nothing changed) shows none. Copies are kept while their build is in the history and removed with it (or with **Clear build history**), except the mapping of what is still installed on a device (below); mappings over 256 MB are not copied.
- **Installed on Pixel_7 · 10:32** means this build's APK is the last one Keynobi installed of its app on that device (the emulator's AVD name, else the device model, else its serial), at that time (with the date when it was not today). Keynobi records every install it does, from **Run App** or from an AI client's `install_apk`, and recognizes the build by the APK's SHA-256, so the line appears whichever way the APK was installed through Keynobi. Installing another APK of the same app on the device replaces it. Keynobi keeps the R8 mapping of what is installed on each device even after the build leaves the history or you clear the history, so a crash on that device can still be deobfuscated; it keeps the last 16 device and app pairs. Installs done outside Keynobi (Android Studio, `adb install`) are not seen, so the line can be out of date after one.

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
- For a crash line, **Deobfuscate** in **Entry Detail** turns the whole crash's stack back into your class, method, and file names with the R8 `retrace` tool from the Android SDK, and shows which build's mapping it used and how it knew: **Deobfuscated with the R8 mapping of build #12 (:app release, map id 6b1c2f0…), matched by map id.** **Copy stack** copies the result. See [Deobfuscating crashes](#deobfuscating-crashes).

#### Deobfuscating crashes

Keynobi deobfuscates a crash only with a mapping it kept (of a build in its history, or of a build it installed on a device) and only when it can tell that this mapping produced the crashing app. The line after **matched by** says how:

- **matched by map id**: recent R8 versions write the mapping's id into the stack trace itself (frames read `(r8-map-id-6b1c2f0…:12)`), and Keynobi kept the mapping with that id. This is exact, and the device is not asked, so it works however the app was installed and even after the device is gone.
- **matched by the SHA-256 of the APK on Pixel_7 …**: Keynobi asks the device for the installed APK and compares its SHA-256 with the APKs Keynobi's builds wrote and the ones it installed. This recognises a build Keynobi made even when Android Studio or `adb install` installed it.
- **matched by Keynobi's install on Pixel_7 … and confirmed by the device**: the device could not hash its APK (very old Android versions have no `sha256sum`), so Keynobi used its own record of what it installed there and checked the app's version code and last update time.

A wrong mapping would give a stack that looks right but names the wrong code, so when Keynobi cannot be sure it shows **Not deobfuscated** and why, and leaves the stack as it is:

- The trace names a map id Keynobi has no mapping for: **no saved mapping for map id …**. The build left the history, or was not built by Keynobi. Keynobi never tries another mapping in that case.
- The APK on the device is not one Keynobi built or installed: **… was not written by a build Keynobi kept or installed**, or **… is not the one Keynobi installed … the app was reinstalled outside Keynobi**. Build it with Keynobi (for example **Run App**) and reproduce the crash.
- The app is installed as split APKs (an app bundle, for example from Play or `bundletool`): **… installed on Pixel_7 as 3 split APKs**. Install a single APK, or use a build whose traces carry a map id.
- The installed variant is not minified, or its mapping was not kept: **no R8 mapping was recorded for this install** or **no R8 mapping was saved for it**.
- The device cannot be asked (disconnected, or an emulator that no longer runs): **could not check …**.

It needs the **Android SDK Command-line Tools** (for `retrace`) in the SDK set in Settings, and a JDK 17 or newer (the same JDK builds use). Without them it shows **Retrace not available** and what to install; Health Center's **R8 retrace** check says whether the tool is there. The first deobfuscation of a crash takes about a second, longer with a large mapping; asking again for the same crash is instant.

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

Each device shows its model or name, whether it is an emulator or physical device, its API level, and a colored status dot. A check mark shows the active deploy target; click a row to make it active, or Tab to the list, move with Up/Down, and press Enter or Space. Offline devices cannot be selected. Use **Refresh** to rescan devices.

Virtual devices:

- Launch an emulator from its row. To shut a running emulator down, hover over it and click stop, press Shift+F10 on it in the connected list and choose **Stop Emulator**, or use stop on its virtual device row.
- **New Virtual Device** creates an AVD, and can download a system image if needed.
- **More options** on an AVD offers **Wipe Data…** and **Delete…**. Stop a running emulator before wiping its data; Keynobi refuses to wipe an AVD that is running.
- A virtual device's buttons appear when you hover over its row or Tab to them.

Wireless debugging pairing is not built in. Pair with `adb pair` and `adb connect` in a terminal; the device then appears in the list.

### App exit reasons

Android 11 (API 30) and later remember why each of an app's processes ended: a crash, a native crash, an ANR, the low-memory killer, the user swiping it away, a system kill, and so on. Open **Show App Exit Reasons** from the Command Palette to see that history for the selected device, newest first, with the time (the device's local time), the process, what the app was doing (foreground, cached, …), its memory, and the system's description.

- It shows exits that never reached Logcat, for example a crash while Logcat was not running. Stack traces are only in Logcat, and only if it was running at the time.
- It reads your project's app: the one build of its application ID installed on the device (such as `com.example.app.debug`). When the project has several application IDs, or several builds of the app are installed, type the package in the **Package** field and press **Refresh**. You can read any other installed app the same way.
- On Android 10 and older the dialog says the device does not keep this history.
- Android keeps a limited number of exits per app; Keynobi shows at most the newest 100.

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
- R8 retrace (the Android SDK Command-line Tools, used to [deobfuscate crashes](#deobfuscating-crashes); a warning when missing)
- Android CLI (`android`; for information only, never a warning: its version and path, or that it is not installed, with a link to its docs; see [Android CLI and the Keynobi skill](#android-cli-and-the-keynobi-skill))
- Java / JDK
- App Data Directory
- App Location (a warning while Keynobi runs from a disk image or a temporary App Translocation copy; move it to **Applications**)

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
- Read why the app's processes exited (`get_exit_reasons`, Android 11+), including crashes and ANRs that never reached logcat.
- Inspect devices and app runtime state.
- Install, launch, stop, and restart apps.
- Inspect the UI hierarchy and drive the device UI: tap, type, swipe, scroll, press keys, open deep links, rotate, toggle network, and grant or revoke permissions.
- Run health checks and query project information.

### Recommended setup

Copy the command from **Health Center**, the **MCP Activity** panel, or **Copy MCP Setup Commands** in the Command Palette. It includes the correct app path. If Keynobi runs from a disk image or a temporary App Translocation copy, no command is offered: move Keynobi to **Applications**, open it from there, and copy the command again.

Claude Code:

```bash
claude mcp add --scope user --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

`--scope user` makes Keynobi available in every folder. Without it, Claude Code registers the server only for the folder where you ran the command. Health Center shows **Configured** only for a registration that works in every folder; a registration for a single folder shows as **Registered for one folder only**. Run the copied command to add the user-wide one.

Codex:

```bash
codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

Codex has no scopes: `codex mcp add` always writes your own Codex configuration, so it works in every folder.

To bind MCP to a specific Android project, append `--project /path/to/MyAndroidProject` to either command. Existing registrations keep working after updating Keynobi; nothing needs to change.

### Android CLI and the Keynobi skill

Google's [Android CLI](https://developer.android.com/tools/agents/android-cli) (`android`) gives agents stateless commands: install SDK packages, create and start emulators, take a one-off screenshot, search Android docs, render Compose previews. Keynobi does not repeat those. It keeps state between calls: builds and their errors and history, the logcat buffer, crashes and their deobfuscation, why the app's processes exited, launch times, and the UI of the running app. Use both.

- **Detection.** Health Center shows Android CLI's version and where it is installed (symlinks such as Homebrew's are resolved), or that it is not installed. Keynobi looks where your terminal would (your login shell's `PATH`) and in the documented install locations (`~/.local/bin`, `/usr/local/bin`, `/opt/homebrew/bin`). It runs `android --no-metrics --version` to read the version, with metrics off, and nothing else. AI clients see the same thing in `run_health_check`. Android CLI is optional: a missing one never counts against your health.
- **The Keynobi skill** is a short `SKILL.md` that tells an agent which to use when. Any MCP client can read it as the resource `keynobi://skill`. To install it for Claude Code, open the **MCP Activity** panel (`Cmd+Shift+M`) and click **Install for Claude Code** under **Agent skill**: it writes `~/.claude/skills/keynobi/SKILL.md`, the path shown there, and nothing else. **Show SKILL.md** shows what will be written. If a different `SKILL.md` is already there (an older version, or your own), the button reads **Replace for Claude Code…** and asks before replacing it. For other clients, click **Copy SKILL.md** and put it where your client reads skills.

### How MCP relates to the app

Your AI client starts a small Keynobi MCP process in the background. If Keynobi is open, that process **attaches** to the app: the AI client then works on the app's project, devices, logcat, and build slot. If it cannot attach, it runs **standalone** with its own state, and its builds and logcat are not visible in the app. It never opens Keynobi for you.

- **Project**: the MCP process asks for the project given with `--project`, or else the Android project that contains the AI client's working folder (a folder with `settings.gradle` or `settings.gradle.kts`, or a parent of it).
  - If Keynobi has that project open, the client attaches and stays on that project. If you later switch Keynobi to another project, the client's project tools (builds, variants, installing, stopping apps) return an error naming both projects until you switch back or restart the MCP server in your AI client. Device, UI, and logcat tools keep working.
  - If the client did not ask for a project (no `--project`, and its folder is not inside an Android project), it attaches and follows whatever project Keynobi has open.
  - If Keynobi has a different project open, or no project, or is not running, the MCP process runs standalone on its own project (falling back to the last project you had open in Keynobi). Keynobi never switches projects for an AI client.
- **Which mode**: ask the client to call `get_project_info`. `mode` is `attached` or `standalone`, `standalone_reason` says why, and `selected_by` says how the project was chosen. Build results also say which mode ran them.
- **Trust**: an AI client can build only a project you trusted in Keynobi. For any other project, `run_gradle_task` and `run_tests` fail with a message asking you to open the project in Keynobi and choose **Trust**; the MCP server never asks itself. Other tools keep working.
- **Builds and logcat**: an attached client shares one build at a time with the app: while either is building, the other is told a build is already running. Its builds stream into the Build tab, and either side can cancel the other's; the result says who cancelled it. A build keeps running if the AI client disconnects; an AI client that cancels its build request cancels the build it started. While a build runs, AI clients that support progress show how long it has run and the Gradle task in progress. Builds and logcat of a standalone server are not visible in the app; its builds can appear in build history the next time Keynobi starts. Two Keynobi processes never build the same project at once: the second is told which process is building it.
- **Quitting Keynobi** cancels a running build (recorded as cancelled because Keynobi quit) and answers the AI client's pending requests with an error. The client then keeps working with a standalone server ("the Keynobi app quit"), without a restart. With `--attach-only` the MCP server exits instead.
- **Status**: the MCP item in the status bar shows how many AI clients are attached (for example **MCP: 2 agents**) and warns about standalone servers. The **MCP Activity** panel (`Cmd+Shift+M`) lists each session with the Keynobi version it runs, the setup commands, and recent tool calls from AI clients.
- **Versions**: an AI client keeps running the Keynobi MCP server it started, even after you update Keynobi. When a server runs another version than the app, the status item turns yellow, its tooltip and the **MCP Activity** panel say which version, and you should restart the AI client (or reconnect its Keynobi MCP server) to load the app's version.

To require the app, add `--attach-only` after `--mcp`: the MCP server then exits with an error instead of running standalone.

AI clients can change your device. `restart_app` keeps the app's data unless the client explicitly passes `clear_data: true` for a specific device. Stopping or restarting an app and granting or revoking its permissions work only on your project's app (its application ID and variants such as `.debug`) unless the client passes `allow_foreign_package: true`. Keynobi refuses to turn off Wi-Fi or turn on airplane mode on a device connected over wireless debugging, because that would disconnect it. Review what your AI client asks to run.

### Driving the device UI

AI clients read the screen through UI Automator, the same accessibility tree the Layout tab shows, and act on it with `adb shell input`.

| Tools | What they do |
|-------|--------------|
| `get_ui_hierarchy`, `find_ui_elements`, `list_clickable_elements`, `find_ui_parent` | Read the screen: the whole tree, the elements matching text, content description, resource ID, class, or package, the clickable elements, or an element's parent. Each element comes with its tree path, bounds, and center. |
| `ui_tap_element`, `ui_tap`, `ui_swipe`, `ui_scroll_until_element`, `send_ui_key` | Tap an element or a point, swipe or long-press, scroll until an element appears, and press Back, Home, Enter, the arrow keys, and a few other keys. |
| `ui_fill_input`, `ui_type_text`, `ui_type_text_unicode`, `clear_focused_input`, `hide_soft_keyboard` | Type. `ui_fill_input` taps a field, clears it, and types; `ui_type_text` types into the focused field. Both type ASCII text, up to 1,000 bytes. `ui_type_text_unicode` pastes any text, including emoji, through the device clipboard. |
| `wait_for_element`, `ui_wait_for_idle`, `ui_assert_element`, `compare_ui_state` | Wait for an element to appear (default 15 s, at most 30 s) or for the screen to stop changing (default 5 s, at most 30 s), check that an element exists and is enabled, checked, and so on, or tell whether the screen changed. |
| `screenshot` | Take a PNG of the screen. |

- **Tap elements, not coordinates.** `find_ui_elements` and `list_clickable_elements` return each element's tree path (child indexes from the root, such as `0.1.2`; the Layout tab uses the same paths unless **Hide boilerplate** is on). `ui_tap_element` and `ui_fill_input` read the screen again and tap the center of the element at that path, and refuse one that is missing or disabled. Tree paths describe the current screen, not a fixed element: after the screen changes, the client finds the element again. Clients can pass the `screenHash` they read as `expectScreenHash` so a tap or typing fails, instead of landing somewhere else, when the screen has changed since.
- **Screenshots are scaled down.** A screenshot's long edge is at most 1,280 pixels unless the client asks for another `max_dimension` (256 to 8,192) or `full_size: true`. The result gives the `scale` from image pixels to device pixels: a client tapping a point it saw multiplies by it first, or better, taps the element with `ui_tap_element`.
- **One screen read per device at a time.** A device serves one UI Automator client at a time, so Keynobi reads a device's screen one request after another; different devices are read in parallel. The Layout tab and every MCP client, attached or standalone, take turns too. A read fails at once with a "busy" error while a connected test run started from Keynobi uses the device, or while another UI Automator client, such as an Android Studio test run, holds it. Try again when it finishes.
- **Deadlines.** A screen read gives up after 1 minute in total, counting the wait for the device and every retry, and each input command after 30 seconds. Tools that wait or scroll read the screen repeatedly, and each read has its own minute.
- **Any app on screen.** These tools act on whatever is on screen; they are not limited to your project's app. Only the tools that stop or restart an app or change its permissions are (see above).

AI clients can deobfuscate crashes too: `get_crash_stack_trace` and `get_crash_logs` take `retrace: true` and then return the deobfuscated stack with the same line naming the build and mapping, or the reason it was not deobfuscated, under the same rules as **Deobfuscate** in the app (see [Deobfuscating crashes](#deobfuscating-crashes)).

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
- Show App Exit Reasons
- Copy MCP Setup Commands

In the Logcat query bar: **Enter** commits a pill, **Up/Down** and **Tab** work with suggestions, **Backspace** in an empty bar removes the last pill, and **Esc** closes suggestions, then clears the query.

### Keyboard navigation

- **Lists** (projects, connected devices, builds): each list is one Tab stop. Up/Down, Home, and End move within it; Enter or Space selects. Shift+F10, or the context-menu key, opens the actions of the focused project or running emulator.
- **Row action menus** (Shift+F10 or right-click): focus moves to the first item. Up/Down move, Enter runs an item, Esc or Tab closes the menu and returns focus to the row.
- **More options** menus: Up/Down highlight an item, Enter runs it, Esc closes the menu.
- **Dialogs** (Settings, Health Center, MCP Activity, Command Palette, App Exit Reasons, confirmations, and the device and variant pickers): focus moves into the dialog and stays there while it is open. Esc closes it, and focus returns to where it was.
- The shortcuts above do not run while a dialog or menu is open.

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
| `~/.keynobi/mappings/` | Copies of the R8 mappings of builds in the history and of builds installed on devices |
| `~/.keynobi/installed-builds.json` | Which build Keynobi last installed on each device, per app |
| `~/.keynobi/retrace/` | The crash stack being deobfuscated, only while `retrace` reads it |
| `~/.keynobi/mcp-activity.jsonl` | Recent AI client activity |
| `~/.keynobi/mcp.sock`, `~/.keynobi/mcp-sessions/` | The socket AI clients attach through while Keynobi is open, and a record per standalone MCP server |
| `~/Library/WebKit/com.keynobi.app` | Saved logcat filters, last query, and dismissed updates |
| `~/.claude/skills/keynobi/SKILL.md` | The Keynobi agent skill, only if you installed it for Claude Code |

### Reset or uninstall

- Reset settings: **Settings → Reset to Defaults**.
- Full reset: quit Keynobi and delete `~/.keynobi`.
- Uninstall: quit Keynobi, delete it from **Applications**, then delete `~/.keynobi` and `~/Library/WebKit/com.keynobi.app`. Remove the MCP server from your AI clients (`claude mcp remove keynobi`, `codex mcp remove keynobi`), and delete `~/.claude/skills/keynobi` if you installed the Keynobi skill.

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

### A crash is not deobfuscated

- **Retrace not available**: install **Android SDK Command-line Tools** in Android Studio's SDK Manager (SDK Tools tab) for the SDK set in Settings, and make sure the Java / JDK check in Health Center shows JDK 17 or newer.
- **Not deobfuscated**: the message says why. Most often the app on the device was not built by Keynobi, or that build has left the history: build it with Keynobi and install it (with **Run App**, or any other way for a single APK), using a minified variant such as `release` with `isMinifyEnabled = true`, and reproduce the crash.

### Stack-trace lines do not open in Android Studio

- Open Health Center and follow the Android Studio CLI steps to put the `studio` command on your PATH (in Android Studio: **Tools → Create Command-line Launcher**).

### Layout capture fails

- Confirm the device is online and unlocked.
- Open the screen you want to inspect, then click **Refresh**.
- Some secure screens or OS states may return partial or empty dumps.
- "Busy" means a connected test run or another UI Automator client, such as an Android Studio test run, is using the device. Capture again when it finishes.

### MCP cannot connect

- Copy the setup command again from Health Center.
- Confirm the app path in the command exists. It must be in `/Applications`, not inside a mounted DMG. Health Center's **App Location** check says when Keynobi runs from a temporary location.
- In Claude Code, run `claude mcp list` from your project folder. If Keynobi is missing there, re-add it with the copied command (it uses `--scope user`).
- If the MCP status item warns about a different version, restart your AI client.
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
