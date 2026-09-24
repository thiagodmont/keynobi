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
- Raw build output retained for MCP/history: `MAX_BUILD_LOG` (5,000 lines).
- Per-build logs: `~/.keynobi/build-logs/build-{id}.jsonl`, pruned after each build by `rotate_build_logs` in two passes: first, files older than `build.buildLogRetentionDays` (default 7; 0 disables) or not in history; then oldest-first until the folder is under `build.buildLogMaxFolderMb` (default 100).

### Execution Paths

Both front doors reserve the same slot and share finalization:

| Front door | Reserve | Finalize | Output |
|------------|---------|----------|--------|
| Tauri `run_gradle_task` (`commands/build.rs`) | `try_reserve_build_slot` | `finalize_completed_build`, then emits `build:complete` | Streams through a Tauri `Channel<BuildLine>` |
| MCP `build_runner::run_task` | `try_reserve_build_slot` | `emit_build_complete` (wraps `finalize_completed_build`; emits only when an `AppHandle` exists) | Not streamed; the result returns when the build ends |

Gradle runs with `--console=plain`. Every path that spawns Gradle must call `try_reserve_build_slot` first.

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

### Invariants

- `runBuild()` resolves only after completion/cancellation state is known.
- Cancellation must clear pending build-completion waiters and process IDs.
- Parsed build errors are persisted by backend finalization before `build:complete` is emitted.
- **Only one build at a time per process.** Every path that spawns Gradle must call `try_reserve_build_slot` first.
- **Success requires exit code 0 AND a `BUILD SUCCESSFUL` summary line.** The exit code is authoritative; the summary line alone is not sufficient.
- A cancelled or timed-out build still records a history entry.
- Build history IDs must stay unique across restarts and clears so log filenames never collide.

---

## Device Management

### Backend State

`DeviceState` (`services/adb_manager.rs`) owns connected devices, the selected serial, and the polling guard. Device polling runs as a detached task every 3 s and emits `device:list_changed` when the set of serials changes.

### AVDs

AVD lifecycle commands go through Android SDK tools. `create_avd_device` and `delete_avd_device` return the refreshed AVD list so the frontend updates in one round-trip; `launch_avd` returns the new serial and emits `device:list_changed`. Validate AVD names, system image IDs, and device profile IDs with the validators in `adb_manager.rs`.

### Frontend

- `DeviceSidebar` is the device management surface; `DevicePickerDialog` handles "choose a device" during run flows.
- Device-picking flows must validate that `selectedSerial` is still online before using it (`resolveDevice` in `build.service.ts`).
- Activity names passed to `am start` must be validated (`validate_activity_name`).

---

## Logcat

### Pipeline

Logcat is a backend-first streaming pipeline:

```text
adb logcat -T 1                       (starts at "now"; no earlier history)
  -> raw line ingestion               (bounded channel, RAW_LOG_LINE_CHANNEL_CAPACITY = 10,000)
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

`commands/logcat.rs` and the MCP tools must apply identical start/stop/clear handling. Until they share one service function, changes to either copy must be mirrored (they are marked `KEEP IN SYNC`).

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
| Gradle task | `utils/validation.rs::validate_gradle_task` |
| Package name | `utils/validation.rs::validate_package_name` |
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

- Headless mode (`AndroidMcpServer::new_headless`) is the supported mode. It picks the project once at startup: `--project`, then `last_active_project` from settings (if it is a directory), then the current directory.
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

## Projects

- Saved projects and `last_active_project` live in settings; `MAX_RECENT_PROJECTS` (20) caps the list.
- Opening a project resolves the Gradle root (`fs_manager::find_gradle_root`), which becomes the effective root for path validation.
- Project App Info edits `versionName`/`versionCode` in `app/build.gradle(.kts)`. Edits must report failure when the file or fields are not found.

## Shutdown

On window close the app has a 3 s budget: cancel a running build, stop logcat, stop device polling, and flush settings. New long-running work must register with this shutdown path.

---

## Known Gaps

Places where the code does not yet meet the rules above. Remove an entry when it is fixed.

- **Cross-process builds.** GUI and headless MCP can build at the same time; `build-history.json` is last-writer-wins. MCP builds do not appear live in the GUI.
- **Build history IDs.** `clear_history` resets `next_id` to 1, so new log filenames can collide with retained files. `MAX_PERSISTED_HISTORY` (20) is effectively unused because load trims to 10.
- **MCP cancel during spawn.** A build cancelled during spawn on the MCP path returns without recording history.
- **Device polling.** `device:list_changed` fires only when the serial set changes, not when a device's state changes (for example unauthorized → device).
- **Unicode typing.** `ui_type_text_unicode` sets the clipboard with a Clipper broadcast, falling back to `content insert`. `am broadcast` exits 0 even when Clipper is not installed, so the fallback may not run and the paste can insert stale clipboard text. Needs verification on a device.
- **Screen hash coverage.** `ui_swipe`, `send_ui_key`, `ui_type_text_unicode`, `clear_focused_input`, and `ui_scroll_until_element` do not accept `expectScreenHash`.
- **Logcat duplication.** Start/stop/clear is duplicated between `commands/logcat.rs` and `mcp_server.rs`. Shutdown sets `streaming = false` without bumping the generation.
- **MCP error model.** Coordinate, permission, and deep-link validation failures return `CallToolResult::error` instead of `McpError::invalid_params`.
- **Validator duplication.** MCP `validate_apk_path` duplicates `validate_apk_within_build_outputs` and hard-codes the `app` module.
- **Activity log.** `mcp-activity.jsonl` is trimmed only at server start (over 1,000 lines → last 500), and summaries are not redacted.
- **Project App Info.** When the app module is not named `app`, the root build file is edited and success is reported even if nothing changed.
- **Dead code.** `DevicePanel.tsx` (panel/popover modes) is not imported anywhere.
