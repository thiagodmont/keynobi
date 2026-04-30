# MCP Server

Keynobi exposes Android development workflows through a stdio MCP server implemented in
`src-tauri/src/services/mcp_server.rs`. The server lets MCP clients inspect the active
Android project, run builds, read logcat, manage devices, inspect UI hierarchy, and drive
basic UI automation.

## Entry Points

| Entry point | Role |
|-------------|------|
| `src-tauri/src/main.rs` | Starts headless MCP mode when the app is launched with `--mcp [--project /path]`. |
| `services/mcp_server.rs` | Owns the MCP server, tool definitions, prompts, resources, validation, and GUI/headless startup. |
| `commands/mcp.rs` | Exposes Tauri commands for setup status, client registration detection, activity reads, PID status, and activity clearing. |
| `services/mcp_activity.rs` | Writes bounded JSONL activity entries and tracks the headless MCP PID. |
| `src/stores/mcp.store.ts` | Frontend state for MCP lifecycle, connected client, server PID, and recent activity. |

## Modes

| Mode | Description |
|------|-------------|
| GUI | Started through the app with shared Tauri state and emits lifecycle events such as `mcp:started`, `mcp:client_connected`, and `mcp:stopped`. |
| Headless | Started with `keynobi --mcp`; initializes state without opening a window and logs to stderr so stdout remains reserved for MCP JSON-RPC. |

## MCP Surface

| Area | Tools |
|------|-------|
| Build | `run_gradle_task`, `get_build_status`, `get_build_errors`, `get_build_log`, `cancel_build`, `list_build_variants`, `set_active_variant`, `find_apk_path`, `run_tests`, `get_build_config` |
| Logcat and crashes | `start_logcat`, `stop_logcat`, `get_logcat_entries`, `get_crash_logs`, `get_crash_stack_trace`, `clear_logcat`, `get_logcat_stats` |
| Devices and apps | `list_devices`, `screenshot`, `get_device_info`, `dump_app_info`, `get_memory_info`, `install_apk`, `launch_app`, `stop_app`, `restart_app`, `get_app_runtime_state`, `list_avds`, `launch_avd`, `stop_avd` |
| UI hierarchy and automation | `get_ui_hierarchy`, `find_ui_elements`, `list_clickable_elements`, `find_ui_parent`, `ui_tap`, `ui_tap_element`, `ui_type_text`, `ui_fill_input`, `clear_focused_input`, `ui_type_text_unicode`, `send_ui_key`, `hide_soft_keyboard`, `ui_swipe`, `ui_scroll_until_element`, `grant_runtime_permission`, `revoke_runtime_permission`, `wait_for_element`, `ui_wait_for_idle`, `ui_assert_element`, `open_deep_link`, `open_app_settings`, `set_device_orientation`, `set_network_state`, `compare_ui_state` |
| Project and health | `get_project_info`, `run_health_check` |

The server also exposes three prompts: `diagnose-crash`, `full-deploy`, and
`build-and-fix`. Resources include project and health summaries plus project files when
available: `android://project-info`, `android://health`, `android://manifest`,
`android://app-build-gradle`, `android://build-gradle`, and `android://gradle-settings`.

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

## Validation Rules

MCP tools treat every external string as untrusted. The server validates Gradle tasks,
package names, device serials, APK paths, deep links, runtime permissions, UI key names,
coordinates, and tree paths before acting. File paths are restricted to the effective
project root or, for APK installs, the project build output directory.
