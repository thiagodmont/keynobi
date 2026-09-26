---
name: keynobi
description: Use for Android work that needs state kept between calls - Gradle builds with their errors and history, logcat streaming and filtering, crashes and deobfuscated stack traces, app exit reasons, launch times, debug sessions that record each install and what followed, and UI automation on the running app - through the Keynobi MCP server. Use Android CLI (`android`) instead for stateless tasks such as SDK packages, creating and starting emulators, one-off screenshots, Android docs, and Compose previews.
---

# Keynobi and Android CLI

Keynobi is a macOS companion app for Android development. Its MCP server (`keynobi`) keeps state between calls: the open project and its package scope, the build history, a logcat buffer, the crashes in it, and the builds it installed on each device. Android CLI (`android`, from Google) is stateless: each command starts fresh. Use each for what it is good at.

## Start here

Call `get_project_info` (project, mode, trust) and `run_health_check` (JDK, SDK, adb, retrace, and whether Android CLI is installed).

When the Keynobi app is open, the MCP server attaches to it and shares its builds, logcat, and devices with the user. Otherwise it runs standalone with its own state; `get_project_info` says which.

## Use Keynobi for

- **Builds**: `run_gradle_task`, `run_tests`, `get_build_status`, `get_build_errors` (structured errors with file and line), `get_build_log`, `cancel_build`, `list_build_variants`, `set_active_variant`, `find_apk_path`, `get_build_config`. Gradle runs only in a project the user trusted in the Keynobi app; if a build is refused for trust, ask the user to trust it there.
- **Logcat**: `start_logcat`, then `get_logcat_entries` (filter by package, tag, level, or text), `get_logcat_stats`, `clear_logcat`, `stop_logcat`. The buffer keeps what happened before you asked.
- **Crashes**: `get_crash_logs` and `get_crash_stack_trace`. Pass `retrace: true` to deobfuscate with the R8 mapping of the build Keynobi installed on that device. `get_exit_reasons` lists why the app's processes exited (Android 11 and later), including ANRs and kills that never reached logcat.
- **Install and launch**: `install_apk` (records which build is on the device, which deobfuscation needs), `launch_app` and `restart_app` (both report the launch time), `stop_app`, `dump_app_info`, `get_memory_info`, `get_app_runtime_state`.
- **Debug sessions**: each install of the app on a device opens a session that records its launches, crashes, exits, bookmarks, and the tool calls you make on that device. `list_debug_sessions` finds them, `get_debug_session` gives one's timeline and crashes with how each was attributed to the installed build, and `compare_debug_sessions` compares two, by default the last run without a crash and the first crashing one after it.
- **UI automation on the running app**: `get_ui_hierarchy`, `find_ui_elements`, or `list_clickable_elements`, then `ui_tap_element` or `ui_fill_input` with the element's tree path. Check results with `wait_for_element`, `ui_assert_element`, and `compare_ui_state`. Tap elements, not coordinates.
- **Package scope**: `stop_app`, `restart_app`, `grant_runtime_permission`, and `revoke_runtime_permission` act only on the project's own app. Pass `allow_foreign_package: true` only when the user asked for another package.

## Use Android CLI for

- SDK packages: `android sdk install`, `list`, `update`, `remove`.
- Emulators: `android emulator create`, `start`, `stop`, `list`, `remove`. Keynobi can also list, launch, and stop existing AVDs.
- One-off screenshots and layout dumps outside a Keynobi session: `android screen capture`, `android layout`.
- Android documentation: `android docs search`, `android docs fetch`.
- Compose previews: `android studio render-compose-preview`.
- New projects: `android create`.

If `run_health_check` reports Android CLI as not installed, tell the user rather than installing it.

## Keep the two in step

- Build and install through Keynobi when you will need crash deobfuscation, launch times, or build errors later. A build done with `android run` or outside Keynobi is not in Keynobi's history, so its crashes cannot be deobfuscated. An APK Keynobi built can be installed by any tool.
- Both tools act on the same devices. After starting an emulator with Android CLI, call `list_devices` to get its serial for Keynobi.
- A Keynobi server started with `--toolsets` serves only some tools. If a tool you need is refused, the error names the toolset to add; tell the user rather than working around it.
