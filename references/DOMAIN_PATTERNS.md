# Domain Patterns

Domain-specific rules for Keynobi. These supplement `CODE_PATTERN.md`; keep this file focused on invariants that are not obvious from local code.

Update this file when a domain workflow, boundary, or safety rule changes. When the code does not yet meet a rule, keep the rule and record the gap under [Known Gaps](#known-gaps).

---

## Process Model

Every domain below runs in two independent contexts (see `BEST_PRACTICES.md` § Process Model):

- **GUI**: Tauri commands operating on the app's managed state.
- **Headless MCP**: `keynobi --mcp` started by an MCP client, with its own `FsState`, `BuildState`, `DeviceState`, `LogcatState`, and `ProcessManager`.

Invariants that say "one at a time" or "the GUI sees it" hold **within one process**. Across processes, only files in `~/.keynobi/` are shared. The GUI also contains an in-process MCP mode (`start_mcp_server`, gated by `settings.mcp.auto_start`), but it serves stdio of the GUI process, so standard MCP clients cannot reach it. Do not rely on it for shared state.

---

## Build

### Backend State

`BuildState` (`services/build_runner.rs`) owns build status, the starting flag, the in-flight process ID, bounded history, accumulated errors, and the bounded raw build log. Keep process lifecycle state in the build domain; do not spread Gradle process ownership into UI code.

Key caps and persistence:

- Build history in memory: `MAX_HISTORY` (10), persisted to `~/.keynobi/build-history.json`.
- Raw build output retained for MCP/history: `MAX_BUILD_LOG` (5,000 lines). Each line keeps at most `MAX_LINE_BYTES` (64 KiB); longer lines end with `… [truncated N bytes]`.
- Structured errors/warnings per build: `MAX_BUILD_ERRORS` (1,000). The newest are kept and a truncation notice is prepended; `errorCount`/`warningCount` count only what was kept (see Known Gaps).
- Per-build logs: `~/.keynobi/build-logs/build-{id}.jsonl`, pruned after each build by `rotate_build_logs` in two passes: first, files older than `build.buildLogRetentionDays` (default 7; 0 disables) or not in history; then oldest-first until the folder is under `build.buildLogMaxFolderMb` (default 100).

### Execution Paths

Both front doors reserve the same slot and share finalization:

| Front door | Reserve | Finalize | Output |
|------------|---------|----------|--------|
| Tauri `run_gradle_task` (`commands/build.rs`) | `try_reserve_build_slot` | `finalize_completed_build`, then emits `build:complete` | Streams through a Tauri `Channel<BuildLine>` |
| MCP `build_runner::run_task` | `try_reserve_build_slot` | `emit_build_complete` (wraps `finalize_completed_build`; emits only when an `AppHandle` exists) | Not streamed; the result returns when the build ends |

Gradle runs with `--console=plain`. Every path that spawns Gradle must call `try_reserve_build_slot` first.

Three paths run the project's `gradlew`: Tauri `run_gradle_task`, MCP `run_gradle_task`/`run_tests` (through `build_runner::run_task`), and Tauri `get_variants_from_gradle`. Each gets its Gradle environment only from `build_runner::trusted_gradle_env`, which refuses an untrusted project before making `gradlew` executable or spawning anything (see [Project Trust](#project-trust)). `build_env_vars` is private so no path can skip the check.

### Frontend Flow

`build.service.ts` owns build orchestration:

```text
runBuild()
  -> runGradleTask (Channel streams lines into build.store, flushed every 50 ms)
  -> wait for build:complete (timeout: settings.mcp.buildTimeoutSec, clamped 60–3600 s)

runAndDeploy()
  -> resolve target device (prompt with the device picker if none is online)
  -> runBuild()
  -> install -> launch
  -> finally: clear deployPhase
```

### Output Parsing

`services/build_parser.rs` turns each output line into a `BuildLine`. Error and warning lines become the diagnostics that the Problems view and MCP `get_build_errors` show. Both front doors parse the same way.

- Formats are verified against real build output in `src-tauri/tests/fixtures/build_output/` (the README lists the toolchain versions). When you support a new format, capture a real log first, add it there, and assert every diagnostic in a fixture test.
- Supported with a location: Kotlin 1 and 2 (Kotlin 2 omits the colon after the column), KSP, javac, Android lint, AAPT2 link errors, resource merger and R8 `ERROR: path:line:col:` lines, and configuration cache problems (warnings).
- Error lines without a recognised location (`e:`, `w:`, `ERROR:`, `error:`, R8 `Missing class`) still become diagnostics with the message only. A failing build must never report zero errors when its log contains error lines.
- Gradle repeats javac and lint errors indented under "What went wrong". Those copies are skipped. AAPT2 link errors appear only there, so they are parsed indented.

### Invariants

- `runBuild()` resolves only after completion/cancellation state is known.
- Cancellation must clear pending build-completion waiters and process IDs.
- Parsed build errors are persisted by backend finalization before `build:complete` is emitted.
- **Only one build at a time per process.** Every path that spawns Gradle must call `try_reserve_build_slot` first.
- **Success requires exit code 0 AND a `BUILD SUCCESSFUL` summary line.** The exit code is authoritative; the summary line alone is not sufficient.
- A cancelled or timed-out build still records a history entry. A cancelled build is recorded as `cancelled`, not `failed`.
- **A run is identified by its Gradle process ID** (`BuildFinalization.run_id`, `build:complete` `runId`). Once the process has spawned, both front doors set `latest_run`. Finalization always appends history, but only the latest run may update the shared status, errors, current build, and cancellable process, so a cancelled build that finishes after its replacement started cannot take the replacement over.
- **The frontend follows only its own run.** `build.service.ts` takes the run ID from `run_gradle_task` and applies a `build:complete` only when `runId` matches; a completion that arrives before the ID is returned is held until it is. Events from cancelled, timed-out, or replaced runs refresh history only.
- `cancelBuild()` releases the waiting `runBuild()` even when the cancel request fails. Cancel is offered only while Gradle runs; install and launch cannot be cancelled.
- **Untrusted projects never build.** The backend refuses (GUI: `AppError::PermissionDenied`; MCP: `invalid_params`), and `runBuild()`/`runAndDeploy()` reject early in Safe Mode with the same instruction.
- **Project opens are generation-counted** (`beginProjectOpen()` in `project.store.ts`). Every open, select, and restore stops after an `await` once a newer open started, so it cannot write another project's registry entry, variant, or device. Deploy stops before APK lookup and before install when the generation changed.
- **Each run has its own output buffer** (`BuildLogSlot::start_run`). `get_build_log` reads the latest run's; a run's history entry saves its own lines even when it finishes late.
- Build history IDs must stay unique across restarts and clears so log filenames never collide. `persist_build_record_in` allocates them under the data lock, above every ID in the persisted history and in `build-logs/`.
- A finished build is appended to the history as re-read from disk, so builds another process recorded are kept, and log rotation checks against that merged history.
- `save_settings` (the settings UI's full snapshot) keeps `recentProjects` and `lastActiveProject` from disk; the backend owns them, including each project's trust.
- **Deploy installs only the requested variant's APK.** `find_output_apk` reads AGP's `output-metadata.json` (else the directory path under `apk/`) and returns an error when no APK or more than one APK matches. It never falls back to another variant's APK.
- **Launch uses the installed APK's package name** (aapt2, else `output-metadata.json`). If neither works, deploy installs but does not launch; it never guesses from the project's `applicationId`.

---

## Device Management

### Backend State

`DeviceState` (`services/adb_manager.rs`) owns connected devices, the selected serial, and the polling state. Device polling runs as a detached task every 3 s and emits `device:list_changed` when any device's serial or connection state changes (for example unauthorized → online).

- **One polling loop.** Start and stop go through `DeviceStateInner::begin_polling` / `stop_polling`, which bump a generation. A loop runs only while its generation is current (`is_current_polling`), checks it again before writing the device list, and is woken by stop instead of finishing its sleep, so a stop followed by a quick start (or shutdown) never leaves two loops.
- **Current SDK path.** Each tick resolves `adb` from settings again, so changing the Android SDK path takes effect on the next poll. The loop (`poll_devices` in `commands/device.rs`) takes the path resolver and the event emitter as parameters and is tested without Tauri.

adb and SDK tool calls have per-operation deadlines (`utils/process.rs`): 10 s for queries (`devices`, `getprop`, `pm`, `dumpsys`, `am force-stop`), 30 s for launches and screenshots, 5 min for `adb install`, 15 s for `emu kill`, 30 s for aapt2, 60 s for avdmanager, and 2 min for `sdkmanager --list`. When `adb devices` times out, `list_devices` logs it and returns an empty list, so polling continues on the next tick; enrichment skips a device whose `getprop` times out.

### AVDs

AVD lifecycle commands go through Android SDK tools. `create_avd_device` and `delete_avd_device` return the refreshed AVD list so the frontend updates in one round-trip; `launch_avd` returns the new serial and emits `device:list_changed`. Validate AVD names, system image IDs, and device profile IDs with the validators in `adb_manager.rs`.

### Frontend

- `DeviceSidebar` is the device management surface; `DevicePickerDialog` handles "choose a device" during run flows.
- Device-picking flows must validate that `selectedSerial` is still online before using it (`resolveDevice` in `build.service.ts`).
- `pickDevice` and `selectVariant` update the selection before the backend confirms it and roll back if the backend rejects it. Each call takes a revision number; a response from a call that is no longer the latest neither rolls back nor persists project meta, so it cannot undo a newer selection.
- Activity names passed to `am start` must be validated (`validate_activity_name`).

### Device Commands

- `am start` usually exits 0 even when nothing started ("Error: Activity not started, unable to resolve Intent"). Every `am start` path (launch, restart, deep links, app settings) checks the output with `adb_manager::am_start_failure` and reports a failure.
- Resolving an installed variant from a base `applicationId` matches the id exactly or at a `.`/`:` boundary: `com.example.app` covers `com.example.app.debug`, never `com.example.apple`.
- A wireless-ADB device (`adb_manager::is_wireless_adb_serial`: `host:port` or an `._adb-tls-connect._tcp` / `._adb._tcp` mDNS name) is reached over its own network. Never turn its Wi-Fi off or airplane mode on; nothing could restore the connection.
- Network toggles read the previous state first and return it. Airplane mode falls back to `settings put global airplane_mode_on` plus the `AIRPLANE_MODE` broadcast only when `cmd connectivity airplane-mode` fails.

---

## Logcat

### Pipeline

Logcat is a backend-first streaming pipeline:

```text
adb logcat -T 1                       (starts at "now"; no earlier history)
  -> raw line ingestion               (lines capped at MAX_LINE_BYTES = 64 KiB; bounded channel, RAW_LOG_LINE_CHANNEL_CAPACITY = 10,000)
  -> processor chain                  (PackageResolver -> CrashAnalyzer -> JsonExtractor -> CategoryClassifier)
  -> bounded LogStore                 (ring buffer: setting, default 50,000, 1,000–100,000)
  -> backend filter                   (log_stream.rs)
  -> batched IPC emit                 (logcat:entries every 100 ms, up to MAX_BATCH_SIZE = 500)
  -> frontend render/filter refinements
```

Backend processing owns package resolution, crash detection, JSON detection, category classification, stats, and ring-buffer storage.

### Stream Lifecycle

The stream is owned by a **generation token**, not by the `streaming` bool. `LogcatStateInner.stream_generation` is bumped by every start _and_ every stop; a running stream task exits as soon as it no longer owns the current generation.

- A bool alone cannot express this: `stop` then `start` flips it back to `true` before the old task's 100 ms tick observes the `false`, leaving two live `adb logcat` processes feeding the same store.
- Starting with a **different** serial supersedes the running stream rather than returning `Ok(())` and silently streaming the old device.
- A stream task clears `streaming` on return **only** if it still owns its generation, so a dying task cannot clobber its replacement.
- The reader task selects against a shutdown channel. Otherwise it parks in `next_line().await` and the child survives `stop_logcat` on an idle device.
- Reconnect is bounded: exponential backoff capped at 30 s (`RECONNECT_BACKOFF_MAX_MS`), giving up after 10 consecutive attempts that produced no output (`RECONNECT_MAX_ATTEMPTS`), with a terminal `logcat:stopped` event. `logcat:reconnecting` is emitted on each retry.
- Lines dropped on a saturated ingest channel are counted in `LogStats.dropped_lines` and surfaced in the toolbar. Never drop silently.
- Clear bumps `clear_epoch`, resets the pipeline context fully (including pre-seeded PIDs), and emits `logcat:cleared`. It clears Keynobi's buffer only, not the device's logcat buffer.

Start, stop, and clear go through `services::logcat::{request_start, request_stop, request_clear}`. The Tauri commands, the MCP tools, and app shutdown all call them; do not re-implement the state transitions at a call site.

### Entry Identity

Entry IDs and crash-group IDs come from one `IdAllocator` on `LogcatStateInner`, shared by every pipeline context. They increase for the life of the process and are never reset: not on reconnect, on stop→start, on a device switch, or on clear.

- `LogStore` binary-searches by ID for context queries, crash lookup picks the newest group by ID, and the frontend selects rows, de-duplicates expanded context, and orders merged rows by ID. A reused ID breaks all of them.
- Clear resets the ring but not the IDs: the frontend may still hold, or be about to receive, entries from before the clear.
- Only the stream task that owns the current generation stores entries, so the store stays in ID order even while a superseded task is winding down.

On the frontend, every clear (Clear, Restart, or an MCP client) arrives as `logcat:cleared`, which invalidates the filter-sync guard. A backfill still in flight when the clear lands is dropped instead of restoring pre-clear entries. The mount backfill takes a guard token too, and the panel subscribes to `logcat:cleared` before starting it.

Auto-start on device connect uses the selected device when it is online and falls back to the first online device (`components/logcat/logcat-auto-start.ts`).

### Filtering

Handle high-volume and simple filters in Rust before crossing IPC. The backend filter spec has one slot each for level, tag, text, and package. Everything else runs in the frontend (`lib/logcat-frontend-only-tokens.ts`): age, negation, regex (`tag~:`, `message~:`), `is:stacktrace`, `pid:`, `tid:`, `time:`, and any second value for a slot the backend already filled.

Filtered Logcat context expansion fetches adjacent raw rows from the backend ring buffer by anchor entry id (`get_logcat_context_entries`, orchestrated by `components/logcat/logcat-context-expansion.ts`). Do not clear the active filter or replace the filtered backfill just to show surrounding context; merge bounded context rows into the visible frontend list and mark them as expanded context.

### Frontend Boundaries

Keep `LogcatPanel.tsx` as composition/orchestration. It is large (about 1,100 lines); extract new logic into focused helpers instead of growing it. Domain logic lives in:

| Concern | Files |
|---------|-------|
| Query parsing and matching | `lib/logcat-query.ts`, `lib/logcat-query-types.ts` |
| Query variables | `lib/logcat-query-variables.ts` |
| Backend/frontend filter boundary | `lib/logcat-frontend-only-tokens.ts`, `lib/logcat-filter-spec.ts` |
| Saved filters and last query persistence | `lib/logcat-filter-storage.ts` |
| Follow-tail and read mode | `lib/logcat-follow-tail.ts` |
| Lifecycle row detection | `lib/logcat-lifecycle.ts` |
| UI line cap | `lib/logcat-ui-lines.ts`, `stores/logcat.store.ts` |
| Stack-frame parsing for Studio jumps | `lib/logcat-stack-frame.ts` |
| Current project package resolution | `lib/logcat-mine-package.ts` |
| Autocomplete data | `lib/logcat-suggestions.ts`, `components/logcat/logcat-suggestion-runtime.ts` |
| Async request ordering | `services/logcat.service.ts` |
| Query state/debounce orchestration | `components/logcat/logcat-query-controller.ts` |
| Query interaction | `QueryBar.tsx`, `QueryBarParts.tsx`, `querybar-query-state.ts`, `querybar-styles.ts` |
| Row selection and navigation | `logcat-row-selection.ts`, `logcat-selection-nav.ts` |
| Context expansion | `logcat-context-expansion.ts` |
| Auto-start device choice | `logcat-auto-start.ts` |
| Saved filter menu | `saved-filter-presets.ts`, `SavedFilterMenu.tsx`, `SavedFilterMenuParts.tsx` |
| Copy/export formatting | `logcat-entry-format.ts` |
| Levels and toolbar counts | `logcat-levels.ts`, `logcat-toolbar-count.ts` |
| Presentational pieces | `LogcatToolbar.tsx`, `LogcatRows.tsx`, `LogcatFilterControls.tsx`, `PackageDropdown.tsx`, `LogEntryDetailPanel.tsx`, `LogcatJsonDetailPanel.tsx` |

Presentational components should not call Tauri IPC except for narrow row actions, such as opening a stack frame in Android Studio. For Logcat UI chrome, follow the ownership map in `DESIGN_SYSTEM.md`.

### IPC Type

`ProcessedEntry.json_body` is `Option<String>`, not a JSON value. The frontend parses it only when the user opens JSON details.

---

## Layout Viewer and UI Automation

### Capture

The layout viewer and MCP UI automation share the same UI Automator capture path (`capture_ui_hierarchy_snapshot`). Keep capture logic centralized in `services/ui_hierarchy.rs`, `ui_hierarchy_parse.rs`, and `ui_automation.rs` so GUI and MCP behavior stay consistent.

A device accepts one UiAutomation client at a time; a second one fails on the device with "UiAutomationService … already registered". So:

- Every UI Automator call takes the device's lock (`ui_automator_lock::acquire`) for the whole capture. The registry is process-wide and keyed by serial, so GUI commands and MCP tools in one process take turns on a device and never wait for another device. An entry lives only while a call holds or waits for it (`MAX_LOCKED_SERIALS`, 64).
- Both build front doors register a connected test run (`begin_instrumentation_for_task`: a task whose name starts with `connected`) until Gradle exits. Its devices are every device, or `ANDROID_SERIAL` when set. Captures on them fail at once with "busy: instrumentation running" and send nothing to the device.
- "already registered" from a dump that returned no XML means another client (an IDE test run, another tool) holds UiAutomation: fail at once with a busy error, do not retry.
- A capture has a total deadline, `CAPTURE_TOTAL_DEADLINE` (60 s), across the lock wait, the shell probes, every dump attempt and fallback (25 s each), and the screenshot. Every subprocess goes through `output_with_timeout`, so a hung `uiautomator dump` or `screencap` is killed, not left holding the device.

### Bounds and Caps

The hierarchy parser and automation tools keep explicit caps:

| Cap | Constant | Value |
|-----|----------|-------|
| Raw XML size | `MAX_XML_BYTES` | 4 MiB |
| Node count | `MAX_NODES` | 8,000 |
| Tree depth | `MAX_DEPTH` | 64 |
| Attribute string length | `MAX_ATTR_LEN` | 2,048 |
| Interactive rows (MCP) | inline clamp | default 80, max 500 |
| Find/match results | `MAX_FIND_RESULTS` | 100 (default 50) |
| Tap/swipe coordinates | `MAX_COORD` | 16,384 |
| Typed text | `MAX_INPUT_TEXT_BYTES` | 1,000 bytes |
| Wait timeouts | `MAX_WAIT_TIMEOUT_MS` | 30 s |
| Scroll attempts | `MAX_SCROLL_ATTEMPTS` | 25 |

Name new caps as constants in the service that enforces them.

### Device Shell Safety

UI actions reach the device through `adb -s <serial> shell <args...>` (`run_adb_shell`). adb joins the arguments with spaces and the device shell re-parses them. Therefore:

- Only allowlisted values (key names, validated permissions, numeric coordinates) may be passed unquoted.
- Free text (typed text, deep links, activity names) must be quoted for the device shell, and `input text` needs its own escaping on top.
- Add injection tests (quotes, `&`, `;`, `$()`, backticks, globs, URLs with query strings) for every tool that sends free text.

### Paths

Tree paths use the same index convention across the layout viewer and MCP tools. If UI presentation collapses boilerplate nodes, preserve enough mapping to reveal or act on the real underlying node. The viewer warns when hidden boilerplate makes its paths differ from MCP paths.

### Screen Hash

Use `screenHash` to protect automation from stale UI state. `ui_tap`, `ui_tap_element`, `ui_type_text`, `ui_fill_input`, and `find_ui_parent` accept `expectScreenHash`. New tools that act on coordinates or tree paths must accept it too.

---

## MCP

`MCP_SERVER.md` documents the tool surface, error model, limits, and security model. The domain rules:

### Tool Definitions

MCP tools live in `services/mcp_server.rs` and are declared with `rmcp` `#[tool_router]` / `#[tool]` macros. Do not describe MCP tools as generated from the frontend action registry. A tool is a thin adapter over the same service function the Tauri command uses.

### Validation

Validate every external string before acting, using the shared validators:

| Input | Validator |
|-------|-----------|
| Gradle task | `utils/validation.rs::validate_gradle_task` (no leading `-`); MCP also applies `check_agent_gradle_task` unless `mcp.allowUnrestrictedGradle` is on |
| Package name | `utils/validation.rs::validate_package_name`; tools that stop an app or change its data or permissions also apply `check_agent_package_scope` unless the call passes `allow_foreign_package: true` |
| Device serial | `utils/validation.rs::validate_device_serial` |
| APK path | `utils/path.rs::validate_apk_within_build_outputs` |
| Activity name | `validate_activity_name` |
| Deep link | `ui_automation::validate_deep_link_uri` |
| Runtime permission | `ui_automation::validate_runtime_permission` |
| UI key name | `resolve_ui_key_code` (allowlist) |
| Coordinates / tree paths | `validate_coordinates`, `validate_tap_coordinate_pair`, `normalize_tree_path` |
| AVD name, system image, device profile | `adb_manager.rs` validators |

### Results and Errors

- Invalid arguments (failed validation, malformed input) return `McpError::invalid_params`.
- Failures while executing a valid request (device offline, build failed, element not found) return `CallToolResult::error(...)` so the model can read the message and recover.
- Use `CallToolResult::structured(json!(...))` for machine-readable results and `CallToolResult::success(...)` for human-readable text.
- Keep payloads bounded and omit noisy command logs unless the tool is explicitly for diagnostics.

### Modes

- Headless mode (`AndroidMcpServer::new_headless`) is the supported mode. It picks the project once at startup (`select_headless_project`): `--project`, then the Gradle root containing the current directory (`find_gradle_root`), then `last_active_project` from settings if it is a directory; otherwise no project. `get_project_info` reports the rule as `selected_by` (`argument`, `working_directory`, `last_active_project`; `app` in GUI mode).
- The MCP server never asks about trust. Build tools refuse an untrusted project with `invalid_params`; every other tool works.
- GUI mode (`AndroidMcpServer::from_app_handle`) shares the GUI's managed state but is served on the GUI's stdio; see [Process Model](#process-model).
- Headless MCP logs to stderr; stdout is reserved for MCP JSON-RPC.

### Activity

MCP lifecycle, tool, prompt, and resource activity is appended to `~/.keynobi/mcp-activity.jsonl` through `services/mcp_activity.rs`. The GUI polls it every 3 s. Each entry records kind, name, duration, status, and a short result summary; never log full arguments or secrets.

---

## Settings

- `settings_manager.rs` loads and saves `~/.keynobi/settings.json` with atomic temp-file writes.
- A settings file that fails to parse is moved to `settings.json.corrupt`, replaced with defaults, and reported through `settings:corrupted`.
- Every settings struct uses `#[serde(default)]`; numeric settings with safe ranges are clamped on load (for example the logcat ring buffer).
- The frontend debounces writes (500 ms). On shutdown the backend waits for a flush acknowledgement from the frontend before exiting.

### JDK Resolution and Health

`services/jdk.rs` is the only place that decides which JDK Gradle uses. `build_env_vars` (GUI builds, MCP builds, variant discovery), the `sdkmanager` calls, GUI Health (`run_health_checks`), and MCP `run_health_check`/`get_project_info` all call it (Health and project info through `check_project_java`, which ignores an untrusted project's `gradle.properties` so the project cannot choose the `java` binary that is probed), so the GUI and a headless MCP process pick the same JDK even when they inherit different environments. Resolution order:

1. `org.gradle.java.home` in `$GRADLE_USER_HOME/gradle.properties` (default `~/.gradle`), then in the Gradle root's `gradle.properties`. This matches Gradle, where the user home file overrides the project file and the daemon runs on this JDK whatever `JAVA_HOME` says.
2. The `java.home` setting (`~/` expanded).
3. Android Studio's bundled runtime: `Android Studio.app`, then `Android Studio Preview.app`, in `/Applications` and `~/Applications`. A runtime whose `release` file reports a version below 17 is skipped.
4. The newest JDK 17 or later under `/Library/Java/JavaVirtualMachines/*/Contents/Home`, by `JAVA_VERSION` in its `release` file (`1.8.0_392` is 8).

`GRADLE_USER_HOME` is read from the process environment because the Gradle child inherits it, so a GUI and an MCP process with different values can still differ, exactly as Gradle would. Sources 1 and 2 are used even when the path is broken, so Health shows the misconfiguration. When nothing resolves, `JAVA_HOME` is not set for Gradle and Health probes `java` on `PATH`.

The probe runs `<home>/bin/java -version` through `output_with_timeout` (`TOOL_PROBE_TIMEOUT`). Java counts as found only when it exits 0 **and** prints a version, so the macOS `/usr/bin/java` stub ("Unable to locate a Java Runtime", non-zero exit) is reported missing. Missing Java is an error; a JDK below 17 is a warning (GUI) or a `warning` field (MCP). Search roots are injected through `JdkSearchRoots`; under `cfg(test)`, `JdkSearchRoots::system()` searches nothing.

## Projects

- Saved projects and `last_active_project` live in settings; `MAX_RECENT_PROJECTS` (20) caps the list.
- Opening a project resolves the Gradle root (`fs_manager::find_gradle_root`), which becomes the effective root for path validation.

### Project Trust

Opening a project must not run its code. Only Gradle runs project code, and only a trusted project may run it.

- `ProjectEntry.trusted`: `true` trusted, `false` Safe Mode (declined or revoked), `null` never asked. A new registry entry starts `null`. The field is always written; an entry saved before it existed has no field and deserializes as `true`, so projects already in the registry keep working.
- `services/project_trust.rs` owns the lookup. `trust_in(settings, project_root)` compares canonical paths: the registry entry for the same folder decides; otherwise the folder is the Gradle root of registered projects, where a Safe Mode entry wins over a trusted one. Unknown folders are untrusted. The trust root is `FsState.project_root` (falling back to the Gradle root).
- `set_project_trust(id, trusted)` writes through `mutate_settings`; the settings UI snapshot cannot change it.
- Frontend: `project.service.ts` asks once, after the registry entry of a newly opened project is known and only if the open is still current (**Trust** or **Open in Safe Mode**; the safe choice is listed last). Dismissing keeps Safe Mode without saving. Trust is derived from the projects store (`isProjectTrusted(root)`), not a separate store. `loadVariants()` skips the Gradle phase for an untrusted root and keys its in-flight coalescing by trust, so a Safe Mode load cannot satisfy one started after trusting.
- Trusting the open project reloads its variants with Gradle. Revoking cancels the open project's running build.
- Health ignores an untrusted project's `org.gradle.java.home` (`jdk::check_project_java`) and its `local.properties` `sdk.dir` (`health_inspector`), so the project cannot choose an executable Keynobi runs.
- Project App Info edits `versionName`/`versionCode` in `app/build.gradle(.kts)`. Edits must report failure when the file or fields are not found.

## Shutdown

On window close the app has a 3 s budget: cancel a running build, stop logcat, stop device polling, and flush settings. New long-running work must register with this shutdown path.

---

## Known Gaps

Places where the code does not yet meet the rules above. Remove an entry when it is fixed.

- **Cross-process builds.** GUI and headless MCP can build at the same time. MCP builds appear in the GUI's history only after the GUI's next build or restart, not live.
- **Persisted history size.** `MAX_PERSISTED_HISTORY` (20) is effectively unused because load trims to `MAX_HISTORY` (10).
- **Build error counts after truncation.** Once `MAX_BUILD_ERRORS` is reached, `errorCount`/`warningCount` count only the retained diagnostics, and the truncation notice itself counts as a warning. True totals would need new `BuildResult`/`BuildCompleteEvent` fields.
- **Duplicate lint diagnostics.** With `abortOnError`, lint prints its first failure from both the report task and the failing task, so that issue is listed twice. The parser is stateless per line, and diagnostics are not de-duplicated.
- **MCP cancel during spawn.** A build cancelled during spawn on the MCP path returns without recording history.
- **Unicode typing.** `ui_type_text_unicode` sets the clipboard with a Clipper broadcast, falling back to `content insert`. `am broadcast` exits 0 even when Clipper is not installed, so the fallback may not run and the paste can insert stale clipboard text. Needs verification on a device.
- **UI Automator across processes.** The device lock and the instrumentation check are per process. A headless MCP server and the GUI (or two headless servers) can still collide on one device, and a connected test run started by one is invisible to the other; the device's "already registered" error is then reported as busy.
- **Screen hash coverage.** `ui_swipe`, `send_ui_key`, `ui_type_text_unicode`, `clear_focused_input`, and `ui_scroll_until_element` do not accept `expectScreenHash`.
- **Logcat clear mid-tick.** The pipeline checks `clear_epoch` at the top of each 100 ms tick but not again when it stores the batch, so lines drained just before a clear can still be stored (and emitted) just after it. They get fresh IDs, so identity is safe; at most one tick of pre-clear lines survives.
- **MCP error model.** Coordinate, permission, and deep-link validation failures return `CallToolResult::error` instead of `McpError::invalid_params`.
- **Validator duplication.** MCP `validate_apk_path` duplicates `validate_apk_within_build_outputs` and hard-codes the `app` module.
- **Activity log.** `mcp-activity.jsonl` is trimmed only at server start (over 1,000 lines → last 500), and summaries are not redacted.
- **APK lookup module.** `find_output_apk` looks only under `app/build/outputs/apk`, so projects whose application module is not named `app` cannot deploy.
- **Project App Info.** When the app module is not named `app`, the root build file is edited and success is reported even if nothing changed.
- **Airplane-mode fallback.** On devices without `cmd connectivity airplane-mode`, the fallback broadcast is a protected broadcast that a non-root shell is normally refused; the setting is then restored and the step reported as failed. Needs verification on a device.
- **Dead code.** `DevicePanel.tsx` (panel/popover modes) is not imported anywhere.
- **Trust is lost with the registry entry.** Removing a project, or eviction past `MAX_RECENT_PROJECTS`, forgets its trust; reopening asks again. Downgrading to a version without trust drops the field, and upgrading again treats those entries as trusted.
- **Revoking does not stop other processes.** Revoking trust cancels only the open project's build in the GUI; a build a headless MCP server already started runs to completion. New builds are refused everywhere.
- **JDK resolution scope.** `-Dorg.gradle.java.home` in `GRADLE_OPTS` or `JAVA_OPTS` and Gradle toolchains are not considered. The Settings **Auto-detect** button (`detect_java_path`) still prefers the process `JAVA_HOME` and a login shell's `JAVA_HOME`, which may be older than 17.
