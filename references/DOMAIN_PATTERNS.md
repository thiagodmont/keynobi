# Domain Patterns

Domain-specific rules for Keynobi. These supplement `CODE_PATTERN.md`; keep this file focused on invariants that are not obvious from local code.

Update this file when a domain workflow, boundary, or safety rule changes. When the code does not yet meet a rule, keep the rule and record the gap under [Known Gaps](#known-gaps).

---

## Process Model

Every domain below runs in two independent contexts (see `BEST_PRACTICES.md` § Process Model):

- **GUI**: Tauri commands operating on the app's managed state. MCP sessions **attached** to the app (see [MCP](#mcp)) run in this process on the same state.
- **Standalone MCP**: `keynobi --mcp` that could not attach to the app, with its own `FsState`, `BuildState`, `DeviceState`, `LogcatState`, and `ProcessManager`.

Invariants that say "one at a time" or "the GUI sees it" hold **within one process**, so they cover the app and its attached sessions but not standalone servers. Across processes, only files in `~/.keynobi/` are shared; the per-project build lock in `build-locks/` is the one cross-process guard.

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

The app and agents start builds through one service, `build_runner::start_build(state, pm, app_handle, BuildRequest { .., origin })`:

| Front door | Origin | Waits for |
|------------|--------|-----------|
| Tauri `run_gradle_task` (`commands/build.rs`) | `BuildActor::App` | Nothing: returns the run ID once Gradle spawned; the frontend follows the events. |
| MCP `run_gradle_task` / `run_tests` (`AndroidMcpServer::run_build`) | `BuildActor::Agent` (session id, client name, standalone) | `BuildHandle::wait()`, bounded by `mcp.buildTimeoutSec`; then `time_out_build`. Meanwhile it reports progress (`BuildHandle::current_task`) when the client asked, and a cancelled request stops this run only (`cancel_run`). |

`start_build`, in order: takes the project's cross-process lock (`build_lock::try_acquire` on `<data dir>/build-locks/<hash of the canonical Gradle root>.lock`, reused by later builds in the same process), reserves the slot (`try_reserve_build_slot`), starts the run's log buffer, spawns Gradle, and emits `build:started`. A detached task then streams output (`build:lines`, batched every `BUILD_LINES_FLUSH_INTERVAL` of 50 ms, at most `MAX_LINES_PER_BATCH` (500) per event and `MAX_PENDING_BUILD_LINES` (10,000) held), and on exit records history and emits `build:complete`. Finalization does not depend on the caller: a client that disconnects, or a GUI call that returns early, still gets its build recorded. Events are emitted only when an `AppHandle` exists (the app and its attached sessions).

Gradle runs with `--console=plain`. Every path that spawns Gradle must go through `start_build`, or call `try_reserve_build_slot` first.

Three paths run the project's `gradlew`: Tauri `run_gradle_task`, MCP `run_gradle_task`/`run_tests` (both through `start_build`), and Tauri `get_variants_from_gradle`. Each gets its Gradle environment only from `build_runner::trusted_gradle_env`, which refuses an untrusted project before making `gradlew` executable or spawning anything (see [Project Trust](#project-trust)). `build_env_vars` is private so no path can skip the check.

### Frontend Flow

`build.service.ts` owns build orchestration:

```text
initBuildService()
  -> listen to build:started, build:lines, build:complete

runBuild()
  -> runGradleTask (returns the run ID; build:started with origin "app" names it first)
  -> build:lines for that run stream into build.store, flushed every 50 ms
  -> wait for build:complete (timeout: settings.mcp.buildTimeoutSec, clamped 60–3600 s)

build:started from another origin (an agent)
  -> shown in the Build panel with "Started by an agent (<client>)", once this
     window's own build or deploy is not using it
  -> its build:lines stream in; build:complete shows the outcome (never deployed)

runAndDeploy()
  -> resolve target device (prompt with the device picker if none is online)
  -> runBuild()            (its build:complete names the history record: recordId)
  -> install -> launch     (launch_app_on_device with buildId = that recordId)
  -> finally: clear deployPhase
```

### Launch Time

Run App records how long the app took to launch on the build whose APK it installed (`BuildRecord.launch`, a `LaunchTiming`).

- **Measured by Android.** `adb_manager::launch_app` starts a known component with `am start -W` and `parse_am_start_timing` reads `TotalTime`, `WaitTime`, and `LaunchState` (`COLD`, `WARM`, `HOT`, `RELAUNCH`; Android 10+). Missing fields are `None`, never 0: no `TotalTime` (the intent went to the activity already on top) or `Status: timeout` means no timing, and an absent or `UNKNOWN` launch state is `None`. The monkey and MAIN-intent fallbacks report no timing.
- **The right record.** `build:complete` carries `recordId`, the history ID the run was saved as. `runAndDeploy` passes its own run's `recordId` to `launch_app_on_device` as `buildId`, never "the latest build", so a build another client finished meanwhile does not take the time. A failed or cancelled build never reaches launch, so nothing is attached.
- **Persistence.** `build_runner::attach_launch_timing` sets the field under the data lock on the re-read history file (then merges it into memory), like every history write; only a successful build takes a launch time. A record this process holds only in memory (its save failed) is updated there. Failing to record does not fail the launch.
- **Device identity.** The timing keeps the serial and, from the polled device list, the AVD name and model.
- **Comparison.** `compareLaunch` (`lib/launch-timing.ts`) compares with the most recent earlier record of the same project and task, the same launch state, and the same device: the same AVD name when either launch has one, else the same serial. Otherwise there is no comparison. The Builds list and the past-build bar show it as text with a sign (`+54 ms vs #41`), not colour alone.
- **MCP.** `launch_app` and `restart_app` report the timing and state in their result and never write build history.

### Viewing a Past Build

The Build panel describes one build at a time: the live build, or a past build picked in the Builds list. `buildState.viewedHistoryId` holds the choice (`viewHistoryBuild`, `viewLiveBuild`), and `viewedBuild()` in `build.store.ts` is the only source the panel reads for task, status, duration, start time, errors, warnings, origin, and cancelledBy. Never read `buildState.errors` or `phase` directly for something the panel shows next to a past build's log.

- **Arriving builds.** `startBuild` with origin `app` (a build this window started) returns the panel to the live build. Any other origin leaves a past build on screen; the panel says a build is running and offers **Show running build**. Install and launch progress belongs to the live build only.
- **Resets.** A project switch (`resetBuildState`, and the panel's own project effect) and **Clear build history** return to the live build. A viewed build that leaves the history (`MAX_HISTORY`) is reported as missing, not replaced by another.
- **Saved logs.** `get_build_log_entries` returns the lines of the saved log, empty when Gradle printed nothing, and `AppError::NotFound` when rotation removed the file (every recorded build writes one). `createHistoricalLog` (`components/build/build-history-log.ts`) turns that into `loading`, `loaded`, `expired`, or `failed`, drops responses for a build no longer selected, and reloads only when the viewed ID changes.

### Application Module

Nothing assumes the application module is `:app`. `services/gradle_modules.rs` finds the modules the settings file `include`s (Groovy or Kotlin, `project(":x").projectDir = file("…")` honoured, directories confined to the Gradle root, at most `MAX_GRADLE_MODULES`), or the root project when it includes none. A module is an application when its build file applies `com.android.application` by id, `apply plugin`, a version-catalog alias (`alias(libs.plugins.…)`, resolved through `gradle/libs.versions.toml`), or a convention plugin whose id ends in `android.application`. A library named `app` is not an application module. When no module is recognised, an `app` directory that applies no other Android plugin is used. Settings, build, and catalog files are read only when their canonical path is inside the canonical Gradle root, and at most 1 MiB each.

- **One application module**: APK lookup (`find_output_apk`), APK install validation, the variant preview and `gradlew <module>:tasks`, default-variant inference, `get_application_id`, Project App Info, and MCP `get_build_config`, `list_build_variants`, and resources all use it.
- **Several**: code that needs one module (`resolve_application_module`) uses the module a caller names, as a module path or a task in it (`:mobile:assembleDebug`, MCP `find_apk_path` `module`), and otherwise returns an error listing the modules. Code that does not need to choose covers all of them: the MCP package scope, built application IDs, and APK install validation.

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
- **Only one build at a time per process, and per project across processes.** Every path that spawns Gradle must call `try_reserve_build_slot` first; `start_build` also holds the project's build lock for the build's whole run, so a second Keynobi process building the same project is refused with the holder's pid.
- **Success requires exit code 0 AND a `BUILD SUCCESSFUL` summary line.** The exit code is authoritative; the summary line alone is not sufficient.
- A cancelled or timed-out build still records a history entry. A cancelled build is recorded as `cancelled`, not `failed`.
- **A run is identified by its Gradle process ID** (`BuildFinalization.run_id`, `build:complete` `runId`). Once the process has spawned, both front doors set `latest_run`. Finalization always appends history, but only the latest run may update the shared status, errors, current build, and cancellable process, so a cancelled build that finishes after its replacement started cannot take the replacement over.
- **The frontend waits only on its own run.** `build.service.ts` takes the run ID from `build:started` (origin `app`) or `run_gradle_task` and completes `runBuild()` only on that run's `build:complete`; output and a completion that arrive before the ID is known are held (capped) until it is. Events from cancelled, timed-out, or replaced runs refresh history only.
- **Builds the window did not start are shown, never deployed.** An agent's build is observed by run ID: it is shown when the window's own build or deploy is idle, dropped if the project changed before then, and after a project switch its completion does not bring it back. While it runs, Build is disabled with a tooltip naming the agent and new builds are refused with the same reason.
- **Who started and who cancelled a build are recorded** (`BuildRecord.origin`, `cancelled_by`; `BuildActor`: `app`, `appQuit`, or `agent`). Records saved before these fields existed load with both `null`. A build cancelled while Gradle is still spawning is recorded as cancelled, and one stopped by the MCP timeout is recorded as failed with the reason.
- `cancelBuild()` cancels any running build, whoever started it, and releases the waiting `runBuild()` even when the cancel request fails. Cancel is offered only while Gradle runs; install and launch cannot be cancelled.
- **Untrusted projects never build.** The backend refuses (GUI: `AppError::PermissionDenied`; MCP: `invalid_params`), and `runBuild()`/`runAndDeploy()` reject early in Safe Mode with the same instruction.
- **Project opens are generation-counted** (`beginProjectOpen()` in `project.store.ts`). Every open, select, and restore stops after an `await` once a newer open started, so it cannot write another project's registry entry, variant, or device. Deploy stops before APK lookup and before install when the generation changed.
- **Each run has its own output buffer** (`BuildLogSlot::start_run`). `get_build_log` reads the latest run's; a run's history entry saves its own lines even when it finishes late.
- Build history IDs must stay unique across restarts and clears so log filenames never collide. `persist_build_record_in` allocates them under the data lock, above every ID in the persisted history and in `build-logs/`.
- A finished build is appended to the history as re-read from disk, so builds another process recorded are kept, and log rotation checks against that merged history.
- `save_settings` (the settings UI's full snapshot) keeps `recentProjects` and `lastActiveProject` from disk; the backend owns them, including each project's trust.
- **Deploy installs only the requested variant's APK.** `find_output_apk` looks in the application module's `build/outputs/apk`, reads AGP's `output-metadata.json` (else the directory path under `apk/`), and returns an error when no APK or more than one APK matches. It never falls back to another variant's APK or another module's. It ignores APKs that resolve outside the application module's `build/outputs` (a symlinked module, `build`, or `outputs` directory, or an `outputFile` that points elsewhere), and install uses the canonical path the validator returns.
- **Launch uses the installed APK's package name** (aapt2, else `output-metadata.json`). If neither works, deploy installs but does not launch; it never guesses from the project's `applicationId`.

---

## Device Management

### Backend State

`DeviceState` (`services/adb_manager.rs`) owns connected devices, the selected serial, and the polling state. Device polling runs as a detached task every 3 s and emits `device:list_changed` when any device's serial or connection state changes (for example unauthorized → online).

- **One polling loop.** Start and stop go through `DeviceStateInner::begin_polling` / `stop_polling`, which bump a generation. A loop runs only while its generation is current (`is_current_polling`), checks it again before writing the device list, and is woken by stop instead of finishing its sleep, so a stop followed by a quick start (or shutdown) never leaves two loops.
- **Current SDK path.** Each tick resolves `adb` from settings again, so changing the Android SDK path takes effect on the next poll. The loop (`poll_devices` in `commands/device.rs`) takes the path resolver and the event emitter as parameters and is tested without Tauri.

adb and SDK tool calls have per-operation deadlines (`utils/process.rs`): 10 s for queries (`devices`, `getprop`, `pm`, `dumpsys`, `am force-stop`), 30 s for launches and screenshots, 5 min for `adb install`, 15 s for `emu kill`, 30 s for aapt2, 60 s for avdmanager, and 2 min for `sdkmanager --list`. When `adb devices` times out, `list_devices` logs it and returns an empty list, so polling continues on the next tick; enrichment skips a device whose `getprop` times out.

### AVDs

AVD lifecycle commands go through Android SDK tools. `create_avd_device` and `delete_avd_device` return the refreshed AVD list so the frontend updates in one round-trip; `launch_avd` returns the emulator's serial and emits `device:list_changed`. Validate AVD names, system image IDs, and device profile IDs with the validators in `adb_manager.rs`.

**An emulator is identified by its AVD name, never by its model or display name.** The `model` in `adb devices -l` is the system image's (`sdk_gphone64_arm64` for every Google image), and name prefixes collide (`Pixel_7`, `Pixel_7_Pro`). `enrich_device_props` sets `Device.avd_name` for each online emulator with `adb_manager::resolve_avd_name` (`adb -s <serial> emu avd name`, else `getprop ro.boot.qemu.avd_name` / `ro.kernel.qemu.avd_name`). The poll loop asks again for an emulator whose name did not resolve (console still starting) for up to `AVD_NAME_RETRY_POLLS` (10) polls while the list is unchanged. The frontend (`runningAvdNames`, `serialForAvd` in `device.store.ts`) matches `avdName` exactly; an emulator without one is not treated as any AVD.

Emulator operations report what happened:

- **Launch** (`launch_emulator`, Tauri `launch_avd`, MCP `launch_avd`) returns the serial of the emulator whose resolved AVD name matches, among emulators that came online after the launch started, so simultaneous launches each get their own serial. An AVD already running is not started again; its serial is returned (`already_running`). A second request for an AVD this process is already starting waits for that emulator instead of starting another (`StartingAvd`). An emulator process that exits with an error before its AVD is online (for example on the AVD's lock) fails the launch at once.
- **Stop** (`stop_emulator`) fails when `adb emu kill` exits non-zero or prints an error (`error: …`, or the console's `KO: …`, which can come with exit status 0), and succeeds only once `adb devices` no longer lists the serial, within `STOP_WAIT` (30 s).
- **Wipe** (`wipe_avd_data`) is refused while the AVD is running (any listed emulator whose AVD name matches) or being started, since the `-wipe-data` relaunch would fail on the AVD's lock. It then waits for that AVD's emulator by name, like launch.

### Frontend

- `DeviceSidebar` is the device management surface; `DevicePickerDialog` handles "choose a device" during run flows. Connected devices are a `Listbox`: offline devices take focus but cannot be selected, and a running emulator's row menu (Shift+F10) offers **Stop Emulator**.
- Device-picking flows must validate that `selectedSerial` is still online before using it (`resolveDevice` in `build.service.ts`).
- `pickDevice` and `selectVariant` update the selection before the backend confirms it and roll back if the backend rejects it. Each call takes a revision number; a response from a call that is no longer the latest neither rolls back nor persists project meta, so it cannot undo a newer selection.
- Activity names passed to `am start` must be validated (`validate_activity_name`).

### Device Commands

- `am start` usually exits 0 even when nothing started ("Error: Activity not started, unable to resolve Intent"). Every `am start` path (launch, restart, deep links, app settings) checks the output with `adb_manager::am_start_failure` and reports a failure.
- Launch and restart start a component with `am start -W`, which returns only once the activity has drawn, so they use the launch deadline (30 s), not the query deadline. See [Launch Time](#launch-time).
- Resolving an installed variant from a base `applicationId` matches the id exactly or at a `.`/`:` boundary: `com.example.app` covers `com.example.app.debug`, never `com.example.apple`.
- A wireless-ADB device (`adb_manager::is_wireless_adb_serial`: `host:port` or an `._adb-tls-connect._tcp` / `._adb._tcp` mDNS name) is reached over its own network. Never turn its Wi-Fi off or airplane mode on; nothing could restore the connection.
- Network toggles read the previous state first and return it. Airplane mode falls back to `settings put global airplane_mode_on` plus the `AIRPLANE_MODE` broadcast only when `cmd connectivity airplane-mode` fails.

### App Exit Reasons

`services/app_exit_info.rs` reads a package's process exit history (`ApplicationExitInfo`, Android 11 / API 30+) with `adb shell dumpsys activity exit-info <package>`. The Tauri command `get_exit_reasons` (the **Show App Exit Reasons** dialog, `components/device/ExitReasonsDialog.tsx`) and the MCP tool `get_exit_reasons` both call `read_exit_reasons`.

- **Validate, then quote.** The serial and a given package are validated before any adb call; the package is also quoted with `quote_device_shell_arg`. A default package comes from the project's build files, which are untrusted too, so it is validated before `pm list packages` runs.
- **Default package.** Without a package, the project must name exactly one application id (`build_inspector::project_package_scope`); several ids, or no project, is `InvalidInput` asking for the package. The history is kept per installed package, so the one installed build of that id (`adb_manager::installed_variant_packages`: the id or the id extended at a `.`/`:` boundary) is read; several installed builds is `InvalidInput` listing them; none installed reads the id itself.
- **Old devices are a result, not an error.** Below API 30 (`ro.build.version.sdk`) the result has `supported: false` and a message, and `dumpsys` is not run. When the API level cannot be read, `dumpsys` runs, and "Bad activity command" / "Unknown command" output is reported the same way.
- **Tolerant parser.** `parse_exit_info` never fails. A record starts at `ApplicationExitInfo #N:`; it reads `key=value` pairs wherever they appear, ignores unknown keys, and leaves missing or unparseable fields `None`. The description is free text: inside it only `state=` and `trace=` end it, and a line without a leading key continues it. Reason codes map to stable names (`crash`, `crashNative`, `anr`, … `unknown` for a code it does not know); the device's own labels are kept. `pss`/`rss` are read as `DebugUtils.sizeValueToString` prints them. `timestampLocal` is set only for `yyyy-MM-dd HH:mm:ss[.SSS]`; a locale-formatted date is ambiguous and stays unparsed.
- **Caps.** At most `MAX_EXIT_INFO_OUTPUT_BYTES` (1 MiB) of output is parsed and `MAX_PARSED_EXIT_RECORDS` (2,000) records kept; records are sorted newest first (by `timestampLocal`, else in dump order) and cut to `MAX_EXIT_RECORDS` (100), with `totalRecords` saying how many the device reported. Descriptions keep `MAX_EXIT_DESCRIPTION_CHARS` (500) characters.
- Fixtures are in `src-tauri/tests/fixtures/exit_info/` (see its README).

---

## Logcat

### Pipeline

Logcat is a backend-first streaming pipeline:

```text
adb logcat -T 1                       (starts at "now"; no earlier history)
  -> raw line ingestion               (lines capped at MAX_LINE_BYTES = 64 KiB; bounded channel, RAW_LOG_LINE_CHANNEL_CAPACITY = 10,000)
  -> processor chain                  (PackageResolver -> CrashAnalyzer -> JsonExtractor -> CategoryClassifier;
                                       batches of at most PIPELINE_BATCH_MAX_ROWS = 5,000 lines or 20 ms)
  -> bounded LogStore                 (ring buffer: setting, default 50,000, 1,000–100,000)
  -> backend filter                   (log_stream.rs)
  -> batched IPC emit                 (logcat:entries every 100 ms, up to MAX_BATCH_SIZE = 500)
  -> frontend render/filter refinements
```

Backend processing owns package resolution, crash detection, JSON detection, category classification, stats, and ring-buffer storage.

- **Bounded batches.** The reader refills the channel while the pipeline drains it, so "drain until empty" has no bound under a flood. `run_batch_into` stops at its `DrainBudget`; the batch is stored and emitted, the task yields, and the next batch runs at once instead of waiting for the 100 ms tick. Lines are never skipped or reordered. What is still queued is `LogStats.backlog_lines`; what the full channel refused is `LogStats.dropped_lines`.
- **PID → package map.** Seeded from `adb shell ps`, extended by ActivityManager `Start proc`, and pruned by `Process <pkg> (pid <n>) has died` and `Killing <n>:<pkg>/...`, so a PID the kernel reuses (often for a native process with no `Start proc` line) is not attributed to the dead app. A death report only unmaps the PID if it still names the dead package; a `Start proc` on a reused PID always replaces the old mapping. `MAX_TRACKED_PIDS` (oldest evicted) and `MAX_TRACKED_PACKAGES` are the backstop for deaths the stream never saw.

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

### Soak Baseline

`npm run perf:soak` builds `src-tauri/examples/logcat_soak.rs` in release mode and streams synthetic logcat through `request_start` and the real reader, pipeline, and store, with the binary itself acting as a fake `adb` (no device, no data directory). Flags: `--rate` (lines/s, default 1,000), `--duration-secs` (600), `--sample-secs` (10), `--giant-line-mb` (10). The traffic includes one app restart per second, JSON bodies, crash bursts, and one oversized line. The report (`perf-metrics/soak_<time>.json`) has RSS start/peak/end and samples, entries ingested and dropped, backlog, batch latency percentiles (from the `keynobi::logcat_batch` trace event), tracked PIDs, whether the oversized line was truncated visibly, and build provenance. It measures the backend only, not IPC or rendering. There are no thresholds yet.

Baseline (commit 71bab15, release, Apple M4 Pro `Mac16,8`, arm64, rustc 1.97.1; machine shared with concurrent builds, so latency tails are pessimistic):

| Run | RSS start / peak / end | Ingested | Dropped | Batch latency p50 / p99 / p99.9 / max | Tracked PIDs |
|-----|------------------------|----------|---------|---------------------------------------|--------------|
| 1,000 lines/s, 30 min | 10.1 / 22.2 / 15.3 MiB | 1,808,294 | 0 | 0.1 / 6.3 / 20.8 / 420 ms | 20 (1,800 restarts) |
| 100,000 lines/s, 30 s | 10.1 / 42.1 / 41.7 MiB | 2,972,497 | 29,458 | 1.4 / 2.4 / 2.8 / 2.9 ms | 20 |

RSS stays flat once the 50,000-entry ring is full. The 10 MiB line was stored as 65,522 bytes ending in `… [truncated 10420268 bytes]`. At 100,000 lines/s every full batch stopped at 5,000 rows; the drops come from the channel filling (10,000 lines) during the 100 ms wait after a batch that emptied it.

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

- `keynobi --mcp` (`mcp_server::run_mcp`) first picks the requested project (`select_headless_project`): `--project`, else the Gradle root containing the current directory (`find_gradle_root`). It then tries to **attach** to the running app over `<data dir>/mcp.sock` (`services/mcp_attach.rs`): one JSON line each way, then the process only relays stdio to the socket. It never launches the app.
- The app (`mcp_attach::start_app_listener`, started in `lib.rs` setup) serves each connection as its own session with `AndroidMcpServer::from_app_handle(..).attached(..)`, so attached sessions share the app's build slot, logcat, devices, and project. The accept/reject rules are `mcp_attach::decide_attach`: no project → accepted and follows the app; the app's open project → accepted and pinned to it; another project, or no project open → refused. The app never changes its open project for an agent.
- A pinned session whose project the app has since closed refuses every tool not in `PROJECT_INDEPENDENT_TOOLS` with a tool error naming both projects (`check_session_project`, applied in `LoggingMcpServer`). New tools are project-scoped unless added to that list.
- When attaching fails, the process runs **standalone** (`AndroidMcpServer::new_headless` + `SessionMode::Standalone { reason }`), with `last_active_project` as the last project fallback. `initialize`, `get_project_info`, the build tools, and the activity log say it is standalone and why. `--attach-only` exits non-zero instead.
- `get_project_info` reports `selected_by` (`argument`, `working_directory`, `last_active_project`; `app` for a session that follows the app) and `mode`.
- The MCP server never asks about trust. Build tools refuse an untrusted project with `invalid_params`; every other tool works.
- `keynobi --mcp` logs to stderr; stdout is reserved for MCP JSON-RPC.

### Activity

MCP lifecycle, tool, prompt, and resource activity is appended to `~/.keynobi/mcp-activity.jsonl` through `services/mcp_activity.rs`, under the data lock, which also covers rotation. The GUI polls it every 3 s while the MCP panel is open. Each entry records kind, name, duration, status, and a short result summary; never log full arguments or secrets.

Live sessions come from `services/mcp_sessions.rs`: the app's in-memory registry of attached sessions (`MAX_ATTACHED_SESSIONS`, pushed to the frontend as `mcp:sessions_changed`) and one `mcp-sessions/<pid>.json` record per standalone server (readers drop records whose process is gone or whose PID now runs another binary). `get_mcp_server_status` returns both.

---

## Settings

- `settings_manager.rs` loads and saves `~/.keynobi/settings.json` with atomic temp-file writes.
- A settings file that fails to parse is moved to `settings.json.corrupt`, replaced with defaults, and reported through `settings:corrupted`.
- Every settings struct uses `#[serde(default)]`; numeric settings with safe ranges are clamped on load (for example the logcat ring buffer).
- The frontend debounces writes (500 ms). On shutdown the backend waits for a flush acknowledgement from the frontend before exiting.

### JDK Resolution and Health

`services/jdk.rs` is the only place that decides which JDK Gradle uses. `build_env_vars` (GUI builds, MCP builds, variant discovery), the `sdkmanager` calls, GUI Health (`run_health_checks`), and MCP `run_health_check`/`get_project_info` all call it (Health and project info through `check_project_java`, which ignores an untrusted project's `gradle.properties` so the project cannot choose the `java` binary that is probed), so the GUI and a standalone MCP process pick the same JDK even when they inherit different environments. Resolution order:

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
- Project App Info (`services/project_app_info.rs`) reads and edits `versionName`/`versionCode` in the first of the application module's `build.gradle.kts` and `build.gradle` (see [Application Module](#application-module)), then the root project's; with several application modules it reports them instead. Only real assignments count: the scanner skips `//` and `/* */` comments (nested in Kotlin) and string literals, and ignores reads, comparisons, and `val`/`var`/`def` declarations.
  - A field is editable only when it has exactly one literal assignment. Otherwise `ProjectAppInfo.versionNameUnavailable`/`versionCodeUnavailable` says why (missing, set N times with the line numbers, or set by an expression, with the line and where the value is probably defined), the value is `null`, and a save of that field is refused with the same message. Several assignments (for example one per product flavor) are refused rather than guessed.
  - `save_project_app_info` takes each field as optional and leaves a `null` field alone. A save that would not change the file is refused. Version codes must be 1 to `MAX_VERSION_CODE` (2100000000), checked in the editor and the command.
  - Both commands resolve the build file with `validate_within_root` and read and write only its canonical path, which must be a regular file inside the canonical project root and at most `MAX_BUILD_FILE_BYTES` (1 MB). A symlinked module directory or build file that leads outside the project is refused; one that stays inside is followed, and the link itself is kept.
  - The file is replaced through a unique temporary file next to the canonical target that gets the original permissions, then renamed over it. A refused or failed save leaves the file byte-identical.

## Shutdown

On window close the app has a 3 s budget: cancel a running build (recorded as cancelled because Keynobi quit) and wait for it to be recorded, answer attached MCP requests still in flight and close the sessions (`mcp_attach::quit_sessions`), stop logcat, stop device polling, and flush settings. New long-running work must register with this shutdown path.

Then, on every app exit (`RunEvent::Exit`, including quit from the menu) and when a standalone `keynobi --mcp` exits (after it waited for its build), `ProcessManager::shutdown_all(SHUTDOWN_GRACE)` stops every process the process manager still runs and starts no new one: SIGTERM, SIGKILL for whatever still runs after the grace (2 s), then at most `FORCE_KILL_WAIT` (1 s) for the exits to be reported. It returns a `ShutdownReport` (stopped, killed, unresponsive) and logs what it had to kill.

### Stopping Processes

- `process_manager` stops a child only through the task that owns its `tokio::process::Child`: `cancel` and `shutdown_all` send it a request, and it signals the child while it has not reaped it (`Child::id()` is `None` afterwards). An exited but unreaped child keeps its PID, so no signal can reach a process that reused it. Never store a child's PID to signal it later.
- `cancel` sends SIGTERM (Gradle needs it to stop the build in the daemon cleanly) and SIGKILL after `CANCEL_GRACE` (5 s). A process that had already exited when the cancel arrived keeps its own exit status; one that was still running is reported as `ProcessTermination::Cancelled`. A cancelled process stays tracked until its exit is reported, and `on_exit` runs exactly once.
- Each child leads its own process group, and signals go to the group: they reach what a wrapper script started in it (as Ctrl-C in a terminal would), but not the Gradle daemon, which moves itself into a new session and is shared with other builds and the IDE.

---

## Known Gaps

Places where the code does not yet meet the rules above. Remove an entry when it is fixed.

- **Cross-process builds.** The app and a standalone MCP server can still build different projects at the same time (the build lock is per project). Standalone builds are not streamed to the app and appear in its history only after its next build or restart. The build lock is best effort: when its file cannot be created or read, the build runs without it.
- **Persisted history size.** `MAX_PERSISTED_HISTORY` (20) is effectively unused because load trims to `MAX_HISTORY` (10).
- **Past builds lack variant and device.** `BuildRecord` does not store the variant, and stores the target device only inside a launch time, so a past build is described by its task (which names the variant), and a past cancelled build shows no duration. The live build's install step, and a launch that reported no time, are not recorded.
- **Launch time scope.** Only `am start -W`'s `TotalTime` is recorded: the time to the first frame. The logcat `Displayed` line (`restart_app` reads it, Run App does not) and `reportFullyDrawn` (time until the app says it is usable) are not recorded on builds. A standalone MCP server never records launch times, and a launch time recorded after another process rewrote the history file is kept only if the record is still among the last `MAX_HISTORY` builds.
- **Build error counts after truncation.** Once `MAX_BUILD_ERRORS` is reached, `errorCount`/`warningCount` count only the retained diagnostics, and the truncation notice itself counts as a warning. True totals would need new `BuildResult`/`BuildCompleteEvent` fields.
- **Duplicate lint diagnostics.** With `abortOnError`, lint prints its first failure from both the report task and the failing task, so that issue is listed twice. The parser is stateless per line, and diagnostics are not de-duplicated.
- **Unicode typing.** `ui_type_text_unicode` sets the clipboard with a Clipper broadcast, falling back to `content insert`. `am broadcast` exits 0 even when Clipper is not installed, so the fallback may not run and the paste can insert stale clipboard text. Needs verification on a device.
- **UI Automator across processes.** The device lock and the instrumentation check are per process. A headless MCP server and the GUI (or two headless servers) can still collide on one device, and a connected test run started by one is invisible to the other; the device's "already registered" error is then reported as busy.
- **Screen hash coverage.** `ui_swipe`, `send_ui_key`, `ui_type_text_unicode`, `clear_focused_input`, and `ui_scroll_until_element` do not accept `expectScreenHash`.
- **Logcat PID attribution without death lines.** Eviction relies on ActivityManager death lines in the system buffer. When they are missed (dropped lines, a buffer not streamed), a dead app's PID stays mapped until the `MAX_TRACKED_PIDS` backstop evicts it, or a `Start proc` reuses it.
- **Logcat sustained floods.** After a batch that empties the channel the pipeline waits for the next 100 ms tick, so input sustained above about `RAW_LOG_LINE_CHANNEL_CAPACITY` lines per tick (roughly 100,000 lines/s) overflows the channel and is dropped (and counted).
- **Logcat clear mid-tick.** The pipeline checks `clear_epoch` at the top of each batch but not again when it stores the batch, so lines drained just before a clear can still be stored (and emitted) just after it. They get fresh IDs, so identity is safe; at most one batch of pre-clear lines survives.
- **MCP error model.** Coordinate, permission, and deep-link validation failures return `CallToolResult::error` instead of `McpError::invalid_params`.
- **Activity log.** Summaries are not redacted.
- **Project App Info `applicationId`.** `applicationId` is read with a first-match pattern, so a commented-out `applicationId` above the real one is shown (and used for `package:mine`).
- **Several application modules.** There is no way to choose the module in the app: Run and deploy, the variant preview, and App Info fail with the list of modules, and variant discovery with Gradle lists every module's tasks. MCP `find_apk_path` takes a `module`; `get_build_config` takes a directory name, not a nested module path.
- **Module detection limits.** Only string-literal `include`s are read (no computed lists or `includeFlat`), `projectDir` only in the `file("…")` and `File(rootDir, "…")` forms, only the `gradle/libs.versions.toml` catalog, and a convention plugin only when its id ends in `android.application`.
- **Airplane-mode fallback.** On devices without `cmd connectivity airplane-mode`, the fallback broadcast is a protected broadcast that a non-root shell is normally refused; the setting is then restored and the step reported as failed. Needs verification on a device.
- **Starting an AVD across processes.** The guard against starting one AVD twice is per process. If the app and a standalone MCP server launch the same AVD at the same moment, the second emulator fails on the AVD's lock and that launch reports the failure even though the AVD comes up. An emulator whose AVD name never resolves (no console answer and neither property) is shown as a device but not as its AVD, so Stop is offered only from the connected list.
- **Dead code.** `DevicePanel.tsx` (panel/popover modes) is not imported anywhere.
- **Exit reason fixtures are reconstructed.** The `exit_info` fixtures follow the framework's dump code, not captures from devices, and the timestamp format (`yyyy-MM-dd HH:mm:ss.SSS`) is unverified on real Android 11–15 devices. If a device prints another format, records still parse but `timestampLocal` is `null` and the order across users' sections is the dump's.
- **Exit reasons are not linked to stacks.** An exit record is not matched to a logcat crash group, and ANR traces (`/data/anr`) and tombstones are not read. Times are the device's local time with no offset. Records of every user (work profile) are listed together, without the user.
- **Trust is lost with the registry entry.** Removing a project, or eviction past `MAX_RECENT_PROJECTS`, forgets its trust; reopening asks again. Downgrading to a version without trust drops the field, and upgrading again treats those entries as trusted.
- **Revoking does not stop other processes.** Revoking trust cancels only the open project's build in the app (including one an attached agent started); a build a standalone MCP server already started runs to completion. New builds are refused everywhere.
- **Processes outside the process manager.** Only Gradle builds run under `process_manager`, so only they are stopped by `shutdown_all`. Logcat's `adb logcat` is stopped by `logcat::request_stop` on window close, and a standalone `keynobi --mcp` exits without stopping its logcat stream (the `adb logcat` child ends when it next writes to the closed pipe).
- **Daemon detachment is assumed.** Stop signals go to the build's process group. A Gradle daemon that did not move itself into its own session (for example with Gradle's native integration disabled) would be stopped with the build, as it would by Ctrl-C in a terminal.
- **JDK resolution scope.** `-Dorg.gradle.java.home` in `GRADLE_OPTS` or `JAVA_OPTS` and Gradle toolchains are not considered. The Settings **Auto-detect** button (`detect_java_path`) still prefers the process `JAVA_HOME` and a login shell's `JAVA_HOME`, which may be older than 17.
