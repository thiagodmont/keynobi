# MCP Server

Keynobi exposes Android development workflows through a stdio MCP server implemented in `src-tauri/src/services/mcp_server.rs`. MCP clients (Claude Code, Codex, and others) can inspect the Android project, run builds, read logcat, manage devices and apps, inspect the UI hierarchy, and drive UI automation.

Update this file when a tool, prompt, resource, limit, or security rule changes. When the code does not yet meet a rule, keep the rule and record the gap under [Known Gaps](#known-gaps).

## Entry Points

| Entry point | Role |
|-------------|------|
| `src-tauri/src/main.rs` | `keynobi --mcp [--project <path>] [--attach-only]` runs `mcp_server::run_mcp`. Any other invocation opens the GUI. |
| `services/mcp_server.rs` | Owns the MCP server, tool definitions, prompts, resources, MCP-specific validation, session modes, and the `--mcp` launcher (attach, else standalone). |
| `services/mcp_attach.rs` | The app's socket listener, the attach handshake and its rules, and the stdio relay `keynobi --mcp` runs when attached. |
| `services/mcp_sessions.rs` | Live sessions: the app's registry of attached sessions and the standalone server records. |
| `utils/validation.rs`, `utils/path.rs` | Shared validators used by both MCP tools and Tauri commands. |
| `services/mcp_activity.rs` | Appends activity entries to the JSONL log and rotates it. |
| `commands/mcp.rs` | Tauri commands for setup commands and registration detection, activity reads (default 200, max 2,000 entries), live sessions (`get_mcp_server_status`), and clearing activity. |
| `src/stores/mcp.store.ts` | Frontend MCP state: attached sessions (live through `mcp:sessions_changed`), standalone servers, and recent activity (polled every 3 s while the MCP panel is open). |

## Modes

Every MCP client runs `keynobi --mcp`. That process first picks the project it wants (`select_headless_project`): `--project`, else the Gradle build containing the client's working directory (the nearest folder with `settings.gradle(.kts)`, the directory or a parent), else none. Then it tries to attach to the running app, and otherwise runs standalone. It never launches the app. Logs go to stderr (`RUST_LOG`, default `warn`).

| Mode | When | State |
|------|------|-------|
| **Attached** | The app is running and accepts the handshake. | `keynobi --mcp` only relays stdio to the app's socket. The app serves the session with `AndroidMcpServer::from_app_handle`, on its own `FsState`, `BuildState`, `DeviceState`, `LogcatState`, and `ProcessManager`. Many sessions can be attached; one ending does not affect the others. |
| **Standalone** | No app, the app refused, or it did not answer within `ATTACH_TIMEOUT` (1.5 s). | Fresh state in the `keynobi --mcp` process, as before attaching existed. The project falls back to `last_active_project` from settings (if it is a directory) when none was requested. |

### Attaching

- The app listens on `<data dir>/mcp.sock` (`mcp_attach::start_app_listener`, started at app launch). The data directory is made `0700` and the socket `0600`; connections from another user are dropped. On start, a socket file that answers belongs to another app instance and is left alone (this instance does not serve MCP); one that does not answer is stale and is replaced. The socket is removed when the app exits. Paths over 103 bytes (the macOS `sun_path` limit) are an error, not a panic.
- Handshake, one JSON line each way before any MCP bytes: `{"attach":1,"version":"<binary version>","project":"/gradle/root"|null,"pid":123,"selected_by":"argument"|"working_directory"|null}`, answered with `{"accepted":true,"project":"/app/project","version":"<app version>"}` or `{"accepted":false,"reason":"…","version":"<app version>"}`. The app waits `REQUEST_TIMEOUT` (5 s) for the request; lines are capped at `MAX_HANDSHAKE_BYTES` (4 KiB).
- Rules (`mcp_attach::decide_attach`): an unknown `attach` version is refused, naming both versions. `project: null` is accepted and the session follows the app's project (`selected_by: app`). A project is accepted only when the app has that project open (canonical path equal to the app's Gradle root or project root); the session is then pinned to it. Otherwise the reason names the app's project or says none is open. The app never changes its open project for an agent. At most `MAX_ATTACHED_SESSIONS` (16) sessions are served.
- A pinned session whose project the app has since closed returns a tool error for every tool not in `PROJECT_INDEPENDENT_TOOLS` ("Keynobi now has B open; this session is for A …"); project resources are refused the same way. Device, UI, logcat, `get_project_info`, and `run_health_check` keep working.
- `--attach-only`: if attaching fails, print the reason to stderr and exit with status 2 instead of running standalone.
- When the app closes the socket (for example, it quits), the relay exits with status 1 and a message on stderr.

### What each mode means for users and features

- Attached sessions share the app's single build slot: the app, and every attached agent, get the same `A Gradle build is already running` answer while a build runs (a tool error for agents, `AppError::InvalidInput` for the GUI). Their builds emit `build:complete`, but their output is not streamed into the Build panel.
- Standalone builds, logcat streams, and device selection are not visible in the app, and standalone and app builds are not mutually exclusive.
- Trust is shared through `settings.json` and read on every build, so trusting or revoking a project in the app applies to a running MCP server without a restart.
- Shared with the app through the data directory: `settings.json` (`set_active_variant` writes it), `build-history.json` (appended under a shared file lock), `mcp-activity.jsonl`, and `mcp-sessions/`.

### Reporting the mode

- `initialize`: `serverInfo` is `keynobi` with the binary's version, titled "Keynobi (attached to the app)" or "Keynobi (standalone)"; `instructions` starts with the mode and, when standalone, why, and that its builds and logcat are not visible in the app.
- `get_project_info`: `mode` (`attached` or `standalone`), `standalone_reason`, `follows_app`, and `pinned_project`. A pinned session whose project the app closed reports `open: false`, `app_project`, and the mismatch in `hint`.
- `run_gradle_task` and `run_tests` end their text with `[mode: …]`; `get_build_status` includes `mode` and `standalone_reason`.
- The activity log's lifecycle entries say `Server started (standalone: <reason>)`, or `Client attached (pid N) — project: …` and `Client detached` for attached sessions.

## Setup

The Health Center and the **Copy MCP Setup Commands** action generate commands with the running app's path:

```bash
claude mcp add --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp
```

Append `--project /path/to/project` to pin a project, or `--attach-only` to refuse running standalone. Registrations made before attaching existed keep working unchanged. Claude Code registers servers in the **local** scope by default (only the directory where the command ran); add `--scope user` to make Keynobi available in every project.

Registration is detected with `claude mcp get keynobi` or `codex mcp get keynobi --json` (5 s timeout; exit code 0 means registered). The CLI is found through `PATH`, known install locations, then a login shell's `command -v`.

## Server Identity and Capabilities

- Built on `rmcp` 3.1 (protocol `2025-11-25`).
- `serverInfo` is `keynobi` with the crate version; its title and the start of `instructions` state the mode (see [Reporting the mode](#reporting-the-mode)).
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
| `run_gradle_task` | O | `task`; `variant` is accepted but ignored. Refused with `invalid_params` unless the user trusted the project in the app (see [Project Trust](#project-trust)). Times out after `mcp.buildTimeoutSec` (default 600 s). Task names starting with `-` (Gradle options) are rejected. Unless `mcp.allowUnrestrictedGradle` is on, tasks matching `publish*`, `promote*`, `upload*`, `uninstall*`, `closeAndRelease*`, `*ToMavenCentral`, or `*PlayStore*` are refused, including Gradle abbreviations such as `pRB`. |
| `get_build_status` | R | |
| `get_build_errors` | R | Errors without a recognised location are returned with the message only. |
| `get_build_log` | R | `lines`: default `mcp.defaultBuildLogLines` (200), max 2,000. |
| `cancel_build` | W | |
| `list_build_variants` | R | |
| `set_active_variant` | W | Persists to settings (shared with the GUI). |
| `find_apk_path` | R | `variant?`. Matches the variant exactly, using `output-metadata.json` when present. Returns `found: false` with a `reason` when no APK or more than one APK matches. |
| `run_tests` | O | `test_type`. Custom tasks go through the same policy as `run_gradle_task`, including the trust check. |
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
| `screenshot` | R | `device_serial`, `max_dimension?` (long edge, default 1,280, 256–8,192), `full_size?`. Returns the PNG, then a JSON text item with `deviceWidth`/`deviceHeight` (the capture's size: the screen in its current rotation, the space `ui_tap` uses), `imageWidth`/`imageHeight`, `scale` (device pixels per image pixel), and a hint to multiply image coordinates by `scale` or use `ui_tap_element`. Larger captures are area-averaged down on the host and re-encoded; a capture that already fits, or `full_size: true`, is returned byte for byte. Passing both parameters, or a `max_dimension` out of range, is `invalid_params`. Captures over 32 MiB or 16 Mpx, and output that is not a PNG, are tool errors. 30 s timeout. |
| `dump_app_info`, `get_memory_info`, `get_app_runtime_state` | R | |
| `install_apk` | D | `device_serial`, `apk_path` (must be an `.apk` under the build outputs). |
| `launch_app` | W | `device_serial`, `package`, `activity?`. Fails when `am start` reports an error, even with exit code 0. |
| `stop_app` | D | `device_serial`, `package`, `allow_foreign_package?`. [Package-scoped](#package-scope). |
| `restart_app` | D | `package`, `device_serial?`, `clear_data?`, `allow_foreign_package?`. [Package-scoped](#package-scope). Force-stops and relaunches; app data is preserved. `clear_data: true` runs `pm clear` first (wipes data and runtime permissions) and requires `device_serial`. The removed `cold` parameter returns an error. |
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
| `open_deep_link`, `open_app_settings` | W | Return a tool error when `am start` reports one (for example "unable to resolve Intent"), even with exit code 0. |
| `set_device_orientation` | W | |
| `set_network_state` | D | `wifi?`, `mobileData?`, `airplaneMode?`. Can cut the device off the network. Turning Wi-Fi off or airplane mode on is refused for wireless-ADB serials (`host:port`, `._adb-tls-connect._tcp`, `._adb._tcp`). Returns `previous` (the prior state read from global settings) so the change can be reverted. Airplane mode uses `cmd connectivity airplane-mode`; only if that fails does it write `airplane_mode_on` and send the `AIRPLANE_MODE` broadcast, restoring the setting when the broadcast is refused. |
| `grant_runtime_permission` | W | `package`, `permission`, `allow_foreign_package?`. [Package-scoped](#package-scope). |
| `revoke_runtime_permission` | D | `package`, `permission`, `allow_foreign_package?`. [Package-scoped](#package-scope). Can kill the app process. |

Every `adb` input command has a 30 s timeout; UI Automator dumps have 25 s.

#### Package Scope

Tools that stop an app, wipe its data, or change its permissions (`stop_app`, `restart_app`, `grant_runtime_permission`, `revoke_runtime_permission`) act only on the open project's app unless the call passes `allow_foreign_package: true` (snake_case on every tool). The project's packages are:

- the `app` module's `applicationId` (or `namespace` when it has none) and any product-flavor `applicationId`,
- each of those followed by a combination of the parsed `applicationIdSuffix` values (flavor and build type, each used once), so `com.example.app.demo.debug` matches but `com.example.apple` does not,
- the exact `applicationId` of every variant in the build outputs (`output-metadata.json`), which covers suffixes set outside the build file.

A package outside the scope, or any package when no application id can be found, is rejected with `invalid_params` before any `adb` call. The message names the project's ids and tells the model to pass `allow_foreign_package: true` only when the user asked for that package. Read-only tools and `launch_app`, `open_deep_link`, and `open_app_settings` are not scoped. The check is `utils/validation.rs::check_agent_package_scope`; the scope comes from `build_inspector::project_package_scope`.

### Project and Health

| Tool | Kind |
|------|------|
| `get_project_info` | R |
| `run_health_check` | R |

`get_project_info` also returns `selected_by` (how the project was chosen: `argument`, `working_directory`, or `last_active_project`; `app` for an attached session that follows the app), the session `mode` fields (see [Reporting the mode](#reporting-the-mode)), `trusted` (whether the project may run its Gradle build), and `trust_hint` (what the user must do when it is not trusted, else `null`).

Both return the same `java` object from `services/jdk.rs`, the JDK Gradle builds use: `ok`, `java_home`, `source` (`userGradleProperties`, `projectGradleProperties`, `settings`, `androidStudio`, `installedJdk`, or `null` when `java` on `PATH` was probed), `major_version`, `version`, `bin`, `warning` (JDK below 17), and `hint`. In `run_health_check` it is `checks.java`. For an untrusted project the project's `gradle.properties` is ignored, so `source` is never `projectGradleProperties` and the project cannot choose the `java` that is probed; `run_health_check` also ignores its `local.properties` `sdk.dir`. See `DOMAIN_PATTERNS.md` § Settings → JDK Resolution and Health.

### Project Trust

Running Gradle executes the project's `gradlew` and build scripts, so builds need the user's trust, given in the Keynobi app (**Trust** when the project is first opened, or **Trust Project** in the Projects sidebar). The MCP server never prompts and cannot grant trust. For a project that is not trusted (declined, revoked, never asked, or not in the app's project list), `run_gradle_task` and `run_tests` fail with `invalid_params` before anything is spawned or made executable. The message tells the model to ask the user to open the project in the Keynobi app and choose Trust. Every other tool works on an untrusted project. The check is `project_trust::require_trusted`, reached through `build_runner::trusted_gradle_env`, the same function the GUI's build and variant commands use.

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
- **Gradle is code execution.** Any Gradle task runs the project's build scripts with the user's privileges. A freshly cloned repository controls its `gradlew`, its build scripts, and paths in its `gradle.properties` and `local.properties`.
- **Destructive device actions.** Uninstalling, clearing data, cutting the network, or stopping emulators can destroy user work. On a personal phone, an agent can also target other apps (for example `com.google.android.gms`) or drop its own wireless-ADB connection.

### Rules

1. Validate every string with the shared validators before acting (see `DOMAIN_PATTERNS.md` § MCP → Validation).
2. Spawn host processes with argv only. Every non-literal argument sent through `adb shell` is quoted for the device shell with `utils::device_shell::quote_device_shell_arg`.
3. Restrict filesystem access. MCP exposes no general path parameters. APK installs are limited to `.apk` files under the project's build outputs, and resources read fixed project files.
4. Make destructive behavior explicit and opt-in, and declare it with tool annotations (`destructiveHint`, `readOnlyHint`, `openWorldHint`) so clients can ask the user for confirmation.
5. Keep responses bounded (see the limits in the tool tables).
6. Never write to stdout except MCP JSON-RPC.
7. Tools that stop an app or change its data or permissions act only on the project's app unless the call passes `allow_foreign_package: true` (see [Package Scope](#package-scope)).
8. Never run a device command that drops the connection adb uses (Wi-Fi off or airplane mode on over wireless ADB).
9. Check `am start` output, not just its exit code (`adb_manager::am_start_failure`).
10. Run a project's build code only when the user trusted the project in the app. Never offer a tool that grants trust, and never run an executable whose path an untrusted project chose (see [Project Trust](#project-trust)).

## Activity Log

- `~/.keynobi/mcp-activity.jsonl`: one JSON entry per lifecycle event, tool call, prompt, or resource read. Each entry records kind, name, duration, status, and a summary of up to 120 bytes of the result. Arguments are not logged.
- The app and every standalone server append to it, so appends, rotation, and clearing run under `settings_manager::with_data_lock`. Rotation writes the kept lines to a temporary file and renames it over the log; since appenders take the same lock, no line is appended to a file being replaced.
- Rotated when it grows past `ROTATE_THRESHOLD_BYTES` (256 KiB), checked on every append and at standalone start: the last `ROTATE_KEEP` (500) entries are kept.
- Live sessions: the app keeps attached sessions in memory (`McpSessionRegistry`) and emits `mcp:sessions_changed` with the list when it changes. Each standalone server writes `~/.keynobi/mcp-sessions/<pid>.json` (pid, start time, project, reason, binary path) at start and removes it on exit. Readers delete records whose process is gone (`kill(pid, 0)`) or whose PID now runs a different binary (`proc_pidpath`). `get_mcp_server_status` returns `{ listening, attached, standalone }`. The single-slot `mcp-server.pid` of older releases is deleted at app and standalone start.

## Service Catalog

| Service | MCP role | Brief description |
|---------|----------|-------------------|
| `adb_manager.rs` | Direct | Resolves Android SDK tools and runs device, emulator, install, launch, and AVD operations. |
| `app_inspector.rs` | Direct | Reads app runtime state and performs app restart flows with launch timing. |
| `build_inspector.rs` | Direct | Parses Gradle files for SDK levels, application id, build types, and product flavors without running Gradle. |
| `build_parser.rs` | Indirect | Converts Gradle output (Kotlin, KSP, Java, lint, AAPT2, R8, configuration cache) into structured build lines and the diagnostics `get_build_errors` returns. |
| `build_runner.rs` | Direct | Runs Gradle tasks, tracks build state/history, captures build logs, and finds output APKs. |
| `crash_inspector.rs` | Direct | Groups and parses logcat crash entries into exception, message, stack frames, and causes. |
| `device_inspector.rs` | Direct | Collects screenshots, device properties, app package details, and memory information. |
| `fs_manager.rs` | Headless setup | Detects the Gradle root for a selected project path. |
| `health_inspector.rs` | Direct | Checks Java, Android SDK, ADB, Gradle wrapper, and project availability. |
| `jdk.rs` | Direct | Resolves the JDK Gradle uses and probes `java -version`; shared with GUI Health and the build environment. |
| `log_pipeline.rs` | Indirect | Enriches raw logcat lines with package, category, JSON, crash, and stats metadata. |
| `log_store.rs` | Indirect | Stores bounded logcat entries and supports filtered MCP log queries. |
| `log_stream.rs` | Indirect | Applies backend-side stream filters before logcat batches reach the frontend. |
| `logcat.rs` | Direct | Starts/stops logcat streaming and owns logcat state, filters, known packages, and buffer access. |
| `mcp_activity.rs` | Direct | Persists MCP lifecycle, tool, prompt, and resource activity and rotates the log. |
| `mcp_attach.rs` | Core | Serves attached sessions on the app's socket, decides the attach handshake, and relays stdio for `keynobi --mcp`. |
| `mcp_server.rs` | Core | Defines the MCP server, tools, prompts, resources, session modes, the `--mcp` launcher, validation, and activity instrumentation. |
| `mcp_sessions.rs` | Direct | Tracks attached sessions and standalone server records for `get_mcp_server_status`. |
| `monitor.rs` | Not exposed | Monitors app memory and app log folder size for the GUI status bar. |
| `process_manager.rs` | Direct | Spawns and cancels long-running child processes used by MCP Gradle builds. |
| `project_trust.rs` | Indirect | Decides whether the user trusted a project to run its Gradle build; `get_project_info` reports it and builds require it. |
| `settings_manager.rs` | Direct | Loads settings, MCP defaults, active variants, data directory paths, and Android tool paths. |
| `telemetry_sentry.rs` | Not exposed | Optional crash reporting (allowlisted fields only); not part of the MCP tool surface. |
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
8. Decide whether the tool reads or acts on the open project. If it does not, add it to `PROJECT_INDEPENDENT_TOOLS` so it keeps working in a pinned session after the app switches projects.
9. Add tests: validation (including injection cases), and behavior against the real binary in `tests/mcp_headless.rs` (fake `adb`/`gradlew` via `headless::Sandbox`; `headless::TestApp` plays the running app for attached sessions).

## Testing and Debugging

- Unit tests live in `mcp_server.rs` (validators, build slot, logcat state, session modes), `mcp_attach.rs` (handshake rules, socket binding, relay), `mcp_sessions.rs`, `mcp_activity.rs`, `commands/mcp.rs`, `utils/validation.rs`, and `ui_automation.rs`. `tests/mcp_headless.rs` covers standalone and attached sessions end to end. `src/stores/mcp.store.test.ts` and `src/components/layout/StatusBar.test.tsx` cover the frontend.
- Try tools interactively with the MCP Inspector:

  ```bash
  npx @modelcontextprotocol/inspector /Applications/Keynobi.app/Contents/MacOS/keynobi --mcp --project /path/to/project
  ```

- Debug logging: set `RUST_LOG=keynobi_lib=debug` in the client's server environment. Logs go to stderr, which most clients show in their MCP logs.
- A stray `println!` corrupts the JSON-RPC stream. If a client reports parse errors, look for stdout writes first.

## Known Gaps

Places where the code does not yet meet the rules above. Remove an entry when it is fixed.

- **Ignored parameter.** `run_gradle_task` accepts `variant` but ignores it.
- **Parameter casing.** UI tools use camelCase on the wire, while their descriptions and all other tools use snake_case.
- **Instructions drift.** The `instructions` string omits 15 tools (for example `cancel_build`, `stop_app`, `wait_for_element`, AVD tools).
- **Groovy projects.** Resources check only `.kts` files and hard-code the `app` module. APK validation also hard-codes `app`.
- **No progress or cancellation.** Tools ignore the request context. A long Gradle run blocks until it ends or times out.
- **Unredacted activity log.** Activity summaries are not redacted.
- **Attached builds in the Build panel.** Builds an attached agent starts share the app's build slot and state, but the Build panel does not stream their output or adopt a build it did not start.
- **Who cancelled a build.** Neither the app nor an agent can tell whether the other cancelled a build.
- **App quitting with clients attached.** Quitting the app cancels the running build (an agent's too), but the agent gets no result for it: its session just ends. Attached clients do not fall back to standalone mid-session; the relay exits (status 1, message on stderr) and the client must restart the MCP server.
- **Standalone build slot.** Standalone servers and the app can still build the same project at the same time; there is no cross-process build lock.
- **Pinned-session check is per call.** A pinned session checks the app's project when a tool starts; switching projects while a tool runs does not stop it.
- **Package scope sources.** The scope reads only the `app` module (or the root build file). An `applicationIdSuffix` set in a convention plugin or through a variable is known only after that variant is built; until then its package needs `allow_foreign_package: true`.
- **Screenshot coordinate space.** `screenshot` takes `deviceWidth`/`deviceHeight` from the capture itself. With a `wm size` override or on a multi-display device, the capture may not match the space `ui_tap` uses, so `scale` would be off.
- **No end-to-end test.** No test drives JSON-RPC (initialize → `tools/list` → `tools/call`).
