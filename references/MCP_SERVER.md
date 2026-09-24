# MCP Server

Keynobi exposes Android development workflows through a stdio MCP server implemented in `src-tauri/src/services/mcp_server.rs`. MCP clients (Claude Code, Codex, and others) can inspect the Android project, run builds, read logcat, manage devices and apps, inspect the UI hierarchy, and drive UI automation.

Update this file when a tool, prompt, resource, limit, or security rule changes. When the code does not yet meet a rule, keep the rule and record the gap under [Known Gaps](#known-gaps).

## Entry Points

| Entry point | Role |
|-------------|------|
| `src-tauri/src/main.rs` | `keynobi --mcp [--project <path>]` starts headless MCP mode. Any other invocation opens the GUI. |
| `services/mcp_server.rs` | Owns the MCP server, tool definitions, prompts, resources, MCP-specific validation, and GUI/headless startup. |
| `utils/validation.rs`, `utils/path.rs` | Shared validators used by both MCP tools and Tauri commands. |
| `services/mcp_activity.rs` | Appends activity entries to the JSONL log and manages the headless PID file. |
| `commands/mcp.rs` | Tauri commands for setup commands and registration detection, activity reads (default 200, max 2,000 entries), server PID status, and clearing activity. |
| `src/stores/mcp.store.ts` | Frontend MCP state: running flag, connected client, server PID, and recent activity (polled every 3 s). |

## Modes

| Mode | How it starts | State |
|------|---------------|-------|
| **Headless** (supported) | An MCP client runs `keynobi --mcp`. | A separate process with fresh `FsState`, `BuildState`, `DeviceState`, `LogcatState`, and `ProcessManager`. The project is chosen once at startup: `--project`, then `last_active_project` from settings (if it is a directory), then the client's working directory. Logs to stderr (`RUST_LOG`, default `warn`). |
| GUI (in-process) | `settings.mcp.auto_start` at app launch. | Shares the GUI's managed state and emits `mcp:started`, `mcp:client_connected`, `mcp:stopped`, and `mcp:startup-failed`. It serves stdio of the GUI process, so a client can reach it only if it launched the app binary itself. Standard setup never uses it. |

What headless mode means for users and features:

- MCP builds, logcat streams, and device selection are not visible live in the GUI.
- Switching projects in the GUI does not change the MCP server's project until the client restarts it.
- Headless and GUI builds are not mutually exclusive across processes.
- Shared with the GUI: `settings.json` (`set_active_variant` writes it), `build-history.json` (last writer wins), `mcp-activity.jsonl`, and `mcp-server.pid`.

## Setup

The Health Center and the **Copy MCP Setup Commands** action generate commands with the running app's path:

```bash
claude mcp add --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

Append `--project /path/to/project` to pin a project. Claude Code registers servers in the **local** scope by default (only the directory where the command ran); add `--scope user` to make Keynobi available in every project.

Registration is detected with `claude mcp get keynobi` or `codex mcp get keynobi --json` (5 s timeout; exit code 0 means registered). The CLI is found through `PATH`, known install locations, then a login shell's `command -v`.

## Server Identity and Capabilities

- Built on `rmcp` 3.1 (protocol `2025-11-25`).
- Capabilities: tools, prompts, resources. No logging, completions, subscriptions, or `listChanged`.
- `instructions` summarizes the tool surface for the model. Update it when adding or removing tools.

## Tools

56 tools. Parameters marked † are camelCase on the wire (UI automation structs use `rename_all = "camelCase"`); all others are snake_case.

**Kind** is the tool's declared MCP annotation. Every tool declares all three hints:

| Kind | Meaning | `readOnlyHint` | `destructiveHint` | `openWorldHint` |
|------|---------|----------------|-------------------|-----------------|
| **R** | Read-only | `true` | `false` | `false` |
| **W** | Changes state, not destructively | `false` | `false` | `false` |
| **D** | Destructive or hard to reverse | `false` | `true` | `false` |
| **O** | Open-world: runs arbitrary project code | `false` | `true` | `true` |

The test `every_tool_declares_annotations_matching_the_reference_docs` fails if a tool's hints do not match its Kind in the tables below, so update both together.

### Build

| Tool | Kind | Notes |
|------|------|-------|
| `run_gradle_task` | O | `task`; `variant` is accepted but ignored. Times out after `mcp.buildTimeoutSec` (default 600 s). Task names starting with `-` (Gradle options) are rejected. Unless `mcp.allowUnrestrictedGradle` is on, tasks matching `publish*`, `promote*`, `upload*`, `uninstall*`, `closeAndRelease*`, `*ToMavenCentral`, or `*PlayStore*` are refused, including Gradle abbreviations such as `pRB`. |
| `get_build_status` | R | |
| `get_build_errors` | R | |
| `get_build_log` | R | `lines`: default `mcp.defaultBuildLogLines` (200), max 2,000. |
| `cancel_build` | W | |
| `list_build_variants` | R | |
| `set_active_variant` | W | Persists to settings (shared with the GUI). |
| `find_apk_path` | R | `variant?`. Matches the variant exactly, using `output-metadata.json` when present. Returns `found: false` with a `reason` when no APK or more than one APK matches. |
| `run_tests` | O | `test_type`. Custom tasks go through the same policy as `run_gradle_task`. |
| `get_build_config` | R | `module?`; rejects `/`, `\`, and `..`. |

### Logcat and Crashes

| Tool | Kind | Notes |
|------|------|-------|
| `start_logcat` | W | `device_serial?`; 5 s startup timeout. |
| `stop_logcat` | W | |
| `clear_logcat` | D | Clears Keynobi's buffer, not the device buffer. |
| `get_logcat_entries` | R | `count` (default `mcp.logcatDefaultCount` = 200, max 10,000), `min_level`, `tag`, `text`, `package`, `only_crashes`. |
| `get_logcat_stats` | R | |
| `get_crash_logs` | R | `count`: default 20, max 200. |
| `get_crash_stack_trace` | R | `package?`, `crash_group_id?` |

### Devices and Apps

| Tool | Kind | Notes |
|------|------|-------|
| `list_devices`, `get_device_info` | R | |
| `screenshot` | R | 20 s timeout. |
| `dump_app_info`, `get_memory_info`, `get_app_runtime_state` | R | |
| `install_apk` | D | `device_serial`, `apk_path` (must be an `.apk` under the build outputs). |
| `launch_app` | W | `device_serial`, `package`, `activity?` |
| `stop_app` | W | |
| `restart_app` | D | `package`, `device_serial?`, `clear_data?`. Force-stops and relaunches; app data is preserved. `clear_data: true` runs `pm clear` first (wipes data and runtime permissions) and requires `device_serial`. The removed `cold` parameter returns an error. |
| `list_avds` | R | |
| `launch_avd` | W | `name` |
| `stop_avd` | D | `serial` |

### UI Hierarchy and Automation

| Tool | Kind | Notes |
|------|------|-------|
| `get_ui_hierarchy`, `list_clickable_elements` | R | Interactive rows: default 80, max 500. |
| `find_ui_elements` | R | Max 100 results. |
| `find_ui_parent` | R | `treePath`†, `expectScreenHash`† |
| `compare_ui_state` | R | Default 30 results. |
| `wait_for_element`, `ui_wait_for_idle`, `ui_assert_element` | R | Wait timeouts max 30 s (defaults 15 s / 5 s). |
| `ui_tap`, `ui_tap_element` | W | `expectScreenHash`† |
| `ui_type_text`, `ui_fill_input` | W | Max 1,000 bytes; `expectScreenHash`†, `clearBefore`† |
| `ui_type_text_unicode`, `clear_focused_input`, `hide_soft_keyboard`, `send_ui_key` | W | |
| `ui_swipe` | W | |
| `ui_scroll_until_element` | W | Max 25 swipes. |
| `open_deep_link`, `open_app_settings` | W | |
| `set_device_orientation` | W | |
| `set_network_state` | D | Can cut the device off the network. |
| `grant_runtime_permission` | W | |
| `revoke_runtime_permission` | D | Can kill the app process. |

Every `adb` input command has a 30 s timeout; UI Automator dumps have 25 s.

### Project and Health

| Tool | Kind |
|------|------|
| `get_project_info` | R |
| `run_health_check` | R |

## Prompts and Resources

Prompts:

| Prompt | Arguments |
|--------|-----------|
| `diagnose-crash` | `package`, `device_serial?` |
| `full-deploy` | `device_serial`, `variant?` (default `debug`), `package?` |
| `build-and-fix` | `task?` (default `assembleDebug`) |

Resources (no templates or subscriptions; unknown URIs return `resource_not_found`):

| URI | Listed when |
|-----|-------------|
| `android://project-info`, `android://health` | Always |
| `android://manifest` | `app/src/main/AndroidManifest.xml` exists |
| `android://app-build-gradle` | `app/build.gradle.kts` exists |
| `android://build-gradle` | Root `build.gradle.kts` exists |
| `android://gradle-settings` | `settings.gradle.kts` exists |

## Error Model

| Situation | Return | Example |
|-----------|--------|---------|
| Arguments fail validation or are malformed | `McpError::invalid_params` | Bad package name, flag-shaped Gradle task |
| Server bug or unexpected internal failure | `McpError::internal_error` | Lock or serialization failure |
| A valid request fails while executing | `CallToolResult::error(...)` (`isError: true`) | Device offline, build failed, element not found, timeout |

Tool errors are for the model to read and recover from, so make the message actionable: say what failed and what to try next. Structured results use `CallToolResult::structured(json!(...))`; plain text uses `CallToolResult::success(...)`. No tool declares an `outputSchema` yet.

## Security Model

### Threats

- **Prompt-injected agents.** Log lines, UI text, web pages, and project files the agent has read can steer it. Treat every tool argument as hostile.
- **Device shell re-parsing.** `adb shell` joins its arguments and the device's `/system/bin/sh` parses them again.
- **Gradle is code execution.** Any Gradle task runs the project's build scripts with the user's privileges.
- **Destructive device actions.** Uninstalling, clearing data, cutting the network, or stopping emulators can destroy user work.

### Rules

1. Validate every string with the shared validators before acting (see `DOMAIN_PATTERNS.md` § MCP → Validation).
2. Spawn host processes with argv only. Every non-literal argument sent through `adb shell` is quoted for the device shell with `utils::device_shell::quote_device_shell_arg`.
3. Restrict filesystem access. MCP exposes no general path parameters. APK installs are limited to `.apk` files under the project's build outputs, and resources read fixed project files.
4. Make destructive behavior explicit and opt-in, and declare it with tool annotations (`destructiveHint`, `readOnlyHint`, `openWorldHint`) so clients can ask the user for confirmation.
5. Keep responses bounded (see the limits in the tool tables).
6. Never write to stdout except MCP JSON-RPC.

## Activity Log

- `~/.keynobi/mcp-activity.jsonl`: one JSON entry per lifecycle event, tool call, prompt, or resource read. Each entry records kind, name, duration, status, and a summary of up to 120 bytes of the result. Arguments are not logged.
- Rotated at server start: over 1,000 lines are trimmed to the last 500.
- `~/.keynobi/mcp-server.pid`: written by headless mode, so the GUI can show whether a server is alive.

## Service Catalog

| Service | MCP role | Brief description |
|---------|----------|-------------------|
| `adb_manager.rs` | Direct | Resolves Android SDK tools and runs device, emulator, install, launch, and AVD operations. |
| `app_inspector.rs` | Direct | Reads app runtime state and performs app restart flows with launch timing. |
| `build_inspector.rs` | Direct | Parses Gradle files for SDK levels, application id, build types, and product flavors without running Gradle. |
| `build_parser.rs` | Indirect | Converts Gradle, Kotlin, Java, and AAPT output into structured build lines and diagnostics. |
| `build_runner.rs` | Direct | Runs Gradle tasks, tracks build state/history, captures build logs, and finds output APKs. |
| `crash_inspector.rs` | Direct | Groups and parses logcat crash entries into exception, message, stack frames, and causes. |
| `device_inspector.rs` | Direct | Collects screenshots, device properties, app package details, and memory information. |
| `fs_manager.rs` | Headless setup | Detects the Gradle root for a selected project path. |
| `health_inspector.rs` | Direct | Checks Java, Android SDK, ADB, Gradle wrapper, and project availability. |
| `log_pipeline.rs` | Indirect | Enriches raw logcat lines with package, category, JSON, crash, and stats metadata. |
| `log_store.rs` | Indirect | Stores bounded logcat entries and supports filtered MCP log queries. |
| `log_stream.rs` | Indirect | Applies backend-side stream filters before logcat batches reach the frontend. |
| `logcat.rs` | Direct | Starts/stops logcat streaming and owns logcat state, filters, known packages, and buffer access. |
| `mcp_activity.rs` | Direct | Persists MCP lifecycle, tool, prompt, and resource activity; rotates logs and manages PID status. |
| `mcp_server.rs` | Core | Defines the MCP server, tools, prompts, resources, mode startup, validation, and activity instrumentation. |
| `monitor.rs` | Not exposed | Monitors app memory and app log folder size for the GUI status bar. |
| `process_manager.rs` | Direct | Spawns and cancels long-running child processes used by MCP Gradle builds. |
| `settings_manager.rs` | Direct | Loads settings, MCP defaults, active variants, data directory paths, and Android tool paths. |
| `telemetry_sentry.rs` | Not exposed | Optional crash/error reporting with privacy scrubbing; not part of the MCP tool surface. |
| `ui_automation.rs` | Direct | Implements MCP UI queries and actions using UI Automator snapshots and `adb shell input`. |
| `ui_hierarchy.rs` | Direct | Captures UI Automator XML, screenshot/context data, foreground activity, and parsed hierarchy snapshots. |
| `ui_hierarchy_parse.rs` | Direct | Parses hierarchy XML into bounded node trees, interactive rows, tree paths, and screen hashes. |
| `ui_hierarchy_xml_sanitize.rs` | Indirect | Repairs common malformed UI Automator XML before strict parsing. |
| `variant_manager.rs` | Direct | Discovers build variants and derives Gradle assemble/install task names. |

## Adding or Changing a Tool

1. Put the behavior in a service function shared with the Tauri command, if one exists. The tool is a thin adapter.
2. Define a params struct with `JsonSchema` and doc comments on every field. Use snake_case field names for new tools.
3. Validate every argument with the shared validators; add a validator to `utils/validation.rs` if none fits.
4. Choose the error type from the [Error Model](#error-model).
5. Bound the output: add a default and a maximum for any count, size, or timeout.
6. Decide the tool's kind (R/W/D/O). Destructive behavior must be opt-in through an explicitly named parameter.
7. Update the `instructions` string, the tool tables in this file, and `USER_MANUAL.md` if users see the change.
8. Add tests: validation (including injection cases), and behavior against the real headless binary in `tests/mcp_headless.rs` (fake `adb`/`gradlew` via `headless::Sandbox`).

## Testing and Debugging

- Unit tests live in `mcp_server.rs` (validators, build slot, logcat state), `commands/mcp.rs`, `utils/validation.rs`, and `ui_automation.rs`. `src/stores/mcp.store.test.ts` covers the frontend store.
- Try tools interactively with the MCP Inspector:

  ```bash
  npx @modelcontextprotocol/inspector /Applications/Keynobi.app/Contents/MacOS/keynobi --mcp --project /path/to/project
  ```

- Debug logging: set `RUST_LOG=keynobi_lib=debug` in the client's server environment. Logs go to stderr, which most clients show in their MCP logs.
- A stray `println!` corrupts the JSON-RPC stream. If a client reports parse errors, look for stdout writes first.

## Known Gaps

Places where the code does not yet meet the rules above. Remove an entry when it is fixed.

- **Server identity.** `Implementation::from_build_env()` resolves inside rmcp, so `serverInfo` reports `rmcp` and rmcp's version instead of Keynobi's.
- **Ignored parameter.** `run_gradle_task` accepts `variant` but ignores it.
- **Parameter casing.** UI tools use camelCase on the wire, while their descriptions and all other tools use snake_case.
- **Instructions drift.** The `instructions` string omits 15 tools (for example `cancel_build`, `stop_app`, `wait_for_element`, AVD tools).
- **Groovy projects.** Resources check only `.kts` files and hard-code the `app` module. APK validation also hard-codes `app`.
- **No progress or cancellation.** Tools ignore the request context. A long Gradle run blocks until it ends or times out.
- **Multi-client PID file.** The PID file is single-slot. With two clients, the first to exit deletes it and the GUI reports MCP as stopped.
- **Unbounded activity log.** The activity log grows without limit during a session, and summaries are not redacted.
- **No end-to-end test.** No test drives JSON-RPC (initialize → `tools/list` → `tools/call`).
