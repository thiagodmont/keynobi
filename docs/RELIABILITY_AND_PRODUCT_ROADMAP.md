# Reliability and Product Roadmap

**Review date:** September 23, 2026  
**Revision:** 2 — verification pass, same day. Every R1–R12 claim re-checked against source; new findings, market update, and resequenced phases added.  
**Reviewed baseline:** Keynobi 0.1.28, commit `d28eed3`  
**Status:** Plan reviewed. All product and technical decisions recorded on 2026-09-24 (see [Decisions](#decisions-recorded-2026-09-24)). **Implementation has not started.** Each phase begins only when the maintainer explicitly starts it.  
**Scope:** Reliability, testability, security boundaries, architecture, user workflows, verification, releases, and market-informed feature suggestions.

Source references below describe the reviewed baseline. File contents and line numbers may change as implementation proceeds. Test results are historical review results, not a claim that a future checkout passes.

> ⚠️ **Do not run `cargo test` (or `npm run generate:bindings` / `check:bindings`, which run it) until R5's test isolation lands.** The Rust suite writes the real `~/.keynobi` data directory. This has already happened: see [R5](#r5-isolate-tests-from-user-data-and-make-persistence-transactional).

## What Changed in Revision 2

- **R5 moved to P0.** The original claim that "no evidence was found that this run changed that data" was wrong. The live `~/.keynobi/build-history.json` contains only test fixtures, and real build logs were deleted by rotation.
- **Ten new findings (R14–R23).** They were not in revision 1. The most severe:
  - device-side shell injection through `adb shell`;
  - an unrestricted `run_gradle_task`;
  - build code executed on project open, with no trust prompt;
  - unreliable JDK detection.
- **Severity adjustments.**
  - Down: R3 to P2 (it needs a hostile repository); AVD `--force` to P3 (the UI already guards it); R2's AVD matching to P2 (the real failure is a false negative).
  - Up: R9's "wait for EOF before `wait()`" hang to P1.
- **A minimal first fix for each item.** Several of revision 1's implementations (policy layer, `ArtifactDescriptor`, `OperationId` state machine, transactional repositories, revision hashes, a separate coordinator) are larger than the confirmed bugs require. They are kept as *later* options, not first steps.
- **Hardening Plan relationship made explicit.** Two defects here were introduced by that plan, one of its "done" items is broken, and its open follow-ups are folded into R13.
- **Market section updated.** Google's Android CLI 1.0 and LogcatOn v1.4.0 materially change positioning. F1 is reframed as *build provenance*, and F7–F16 are added.
- **Phases resequenced** around a P0 "stop active harm" phase and a Phase 0.5 quick-wins batch. Added per-item status tracking and reliability metrics.
- **Decisions D1–D15 recorded** on 2026-09-24 (see [Decisions](#decisions-recorded-2026-09-24)).

## Recommendation

Prioritize a reliability-focused release before adding substantial new functionality. Keynobi already has useful architecture and extensive tests, but several important workflows can still act on the wrong target, accept stale state, execute untrusted input, or report an outcome that does not match what happened.

The highest priorities are:

1. **Stop active harm (P0):**
   - isolate tests from user data;
   - make restart non-destructive by default;
   - quote every `adb shell` argument;
   - restrict what agents can pass to Gradle;
   - remove unused webview filesystem permissions.
2. Make build, deploy, project switching, and cancellation operate on explicit identities.
3. Resolve the separation between GUI state and the normal headless MCP connection.
4. Make logcat continuity, recovery, and filtering deterministic.
5. Bound every subprocess and buffer.
6. Strengthen verification at the actual desktop and device boundaries.

Prefer the **smallest fix that closes the confirmed bug**, with a regression test that fails without it. Introduce new abstractions only when a second caller or a failing test demands them.

No application code was changed during the review. This document records recommendations for approval; it is not an instruction to execute every phase automatically.

## Priority and Status Index

Update the **Status** column as work proceeds. Valid values: Not started, In progress, Done (commit), Won't do (reason).

| ID | Title | Priority | Status |
| --- | --- | --- | --- |
| R5 | Test isolation (user-data writes) | **P0** | Not started |
| R14 | Quote `adb shell` arguments; fix Unicode typing | **P0** | Not started |
| R15 | Restrict `run_gradle_task`; annotate MCP tools | **P0** | Not started |
| R1 | Non-destructive restart; explicit clear-data | **P0** | Not started |
| R3a | Remove unused webview filesystem permissions/scopes | **P0** (quick) | Not started |
| R2 | Exact APK/variant resolution (AVD part: P2) | P1 | Not started |
| R4 | Build/deploy/project operation ownership | P1 | Not started |
| R5b | Cross-process persistence safety | P1 | Not started |
| R6 | GUI/MCP shared state | P1 | Not started |
| R7 | Logcat identity and reconciliation | P1 | Not started |
| R8 | Telemetry allowlist and immediate opt-out | P1 | Not started |
| R9a | Process EOF hang and adb timeouts | P1 | Not started |
| R16 | Project trust before executing Gradle | P1 | Not started |
| R17 | JDK detection and unified health probes | P1 | Not started |
| R18 | Build diagnostic parser for modern toolchains | P1 (verify first) | Not started |
| R20 | Device-affecting MCP tool scoping and truthful results | P1 | Not started |
| R3 | Remaining filesystem boundary / App Info safety | P2 | Not started |
| R9 | Remaining subprocess supervision and device reconciliation | P2 | Not started |
| R10 | Resource budgets and performance measurement | P2 | Not started |
| R11 | Contracts, native tests, release verification | P2 | Not started |
| R12 | UI truthfulness, accessibility, diagnostics | P2 | Not started |
| R19 | UI Automator process hygiene and serialization | P2 | Not started |
| R21 | Agent ergonomics (screenshots, progress, cancellation) | P2 | Not started |
| R22 | MCP setup, distribution, and updates | P2 | Not started |
| R23 | Small correctness cleanups | P3 | Not started |
| R13 | Extract seams where fixes need them | P3 (supporting) | Not started |

## Review Basis and Confidence

The review covered:

- project documentation, Rust services and commands, frontend stores and workflows, the MCP implementation, test infrastructure, CI, release workflows, and competitor documentation;
- in revision 2, a claim-by-claim verification of R1–R12, a separate gap search, and refreshed market research.

The following checks passed during the original review:

| Check                         | Result                                      |
| ----------------------------- | ------------------------------------------- |
| Frontend/unit tests           | 1,186 tests across 116 files                |
| Rust tests                    | 415 passed; 47 binding-export tests skipped |
| Browser E2E                   | 30 passed                                   |
| Storybook smoke/accessibility | 9 passed                                    |
| TypeScript, lint, formatting  | Passed                                      |
| Version synchronization       | Passed                                      |

These results are a useful baseline. They do **not** establish that packaged macOS behavior, real ADB failures, physical devices, or extended streaming sessions work correctly. The Rust run itself modified user data (see R5).

Most findings below are **confirmed from code paths**. Timing-dependent consequences still need deterministic regression tests. Items marked *needs verification* have not yet been demonstrated.

Severity and confidence are separate: a high-impact code path can warrant P1 work even when the adverse timing has not yet been reproduced on a real device.

## Relationship to the Hardening Plan

[`HARDENING_PLAN.md`](HARDENING_PLAN.md) was executed on 2026-08-18 (commits `884a8be`…`6329fa9`). This roadmap builds on it:

- **Defects introduced by the hardening work:**
  - **Selection rollback race** (R9): commit `11c95fd` captures `previous` before the await. A late failure can therefore roll back a newer, successful selection.
  - **Logcat binary search on non-monotonic IDs** (R7): HARDENING 5.5 added `index_of` binary search on the premise that IDs are monotonic. Reconnects, stop→start, and device switches break that premise.
  - **Lesson:** rollback-on-failure and ordered-index optimizations need an explicit revision/ordering invariant *and* a test that violates it.
- **Item marked done but broken:** HARDENING 6.5 configured V8 coverage, but `@vitest/coverage-v8` is not installed, so `npm run test:coverage` fails (R11).
- **Open follow-ups folded into this roadmap:**
  - logcat start/stop/clear duplicated between `commands/logcat.rs` and `mcp_server.rs` ("KEEP IN SYNC") → R7/R13;
  - raw-PID signalling in `process_manager::cancel` → R9/R13;
  - `mcp_server.rs` split (now 3,504 lines, about 15 tests) → R13.
- **Manual verification still outstanding:** the plan's 7 device-dependent checks (logcat restart ×5, stop on idle device, device switch mid-stream, MCP build visibility, concurrent build rejection, unplug/replug, corrupt settings) have never been run. They are the first Phase 0 deliverable.

## Existing Strengths to Preserve

Build on the existing work:

- Rust-owned state and generated IPC models.
- Bounded build history and raw log buffers.
- Build success checks that consider the process exit code.
- Logcat generation guards, reconnect limits, batching, and dropped-line counters.
- Shared input validators and existing canonical-path checks.
- UI hierarchy size/depth limits and stale-screen protection.
- Settings recovery and atomic replacement.
- Broad parser, store, component, and browser tests.
- Signed/notarized release configuration tied to a CI-validated commit.

Keep **Tauri, Rust, SolidJS, and the existing domain structure**. The problems found do not justify changing frameworks or rewriting the application.

---

## Verified Findings from Revision 1 (R1–R13)

### R1. Make Destructive Actions Explicit, Especially Application Restart

**Priority:** P0 (restart default); P3 (AVD `--force`). **Verification:** Confirmed.

The MCP `restart_app` tool defaults `cold` to `true`, and that path executes `pm clear`. A generic restart therefore wipes application data and runtime permissions unless the caller overrides the default. The tool description does not mention this; only the parameter description does.

Every MCP tool with an optional serial also silently picks the first online device when the serial is omitted.

**Evidence:**
- [MCP restart default](../src-tauri/src/services/mcp_server.rs#L742)
- [data-clearing implementation](../src-tauri/src/services/app_inspector.rs#L83)
- [implicit first-device selection](../src-tauri/src/services/adb_manager.rs#L1310)
- [AVD `--force`](../src-tauri/src/services/adb_manager.rs#L955)

**Minimal first fix**

- Replace `cold` with `clear_data: bool`, defaulting to `false`. Ordinary restart becomes force-stop plus launch.
- Require `device_serial` for `restart_app` and any tool that clears data. Return an ambiguity error when it is omitted and several devices are online.
- Add rmcp `destructive_hint` / `read_only_hint` annotations to all tools (see R15).
- Drop `--force` from AVD creation. `CreateDeviceDialog` already rejects exact name matches, so the remaining risk is a stale AVD list (an AVD created in Android Studio after the last refresh). That makes it P3.

**Later, if needed:** a backend policy layer distinguishing inspection, execution, and destructive actions, shared by the UI toolbox (F4) and MCP.

**Acceptance criteria:**
- A normal restart preserves a test application's saved data.
- Clearing data requires the explicit parameter.
- Ambiguous device selection produces no side effect.
- An existing AVD cannot be overwritten.

### R2. Guarantee That Deployment and Emulator Actions Use the Intended Target

**Priority:** P1 (APK resolution); P2 (AVD association, emulator stop, wipe correlation). **Verification:** APK resolution confirmed. AVD matching only partly true, as detailed below. `stop_app`'s exit status was already fixed in `ffbe85e`.

**APK resolution.**
- The resolver compares the whole variant name, lowercased, against *individual* parent directory names. For `paidDebug`, `"paiddebug"` matches neither `paid` nor `debug`.
- The fallback passes then return the first APK in `read_dir` order. With a stale `apk/free/debug/` present, Run can install the wrong flavor with no signal.
- The resolver also hard-codes the `app` module.
- When aapt2 fails, deploy falls back to the project `applicationId`. That ignores `applicationIdSuffix`, so launch targets the wrong package.

**AVD association.**
- The frontend uses bidirectional substring matching. However, the compared value is usually the `model` from `adb devices -l` (for Google images, `sdk_gphone64_arm64`), not the AVD name.
- The common failure is therefore a **false negative**: a running AVD is not shown as running, and Stop silently does nothing. Prefix collisions (`Pixel_7` vs. `Pixel_7_Pro`) need non-standard images.

**Emulator operations.**
- `stop_emulator` maps any completed run to `Ok`, so MCP `stop_avd` reports "stopped" on failure.
- `wipe_avd_data` returns success if *any* emulator comes online. Wiping X while an unrelated emulator Y runs, or while X is already running (the `-wipe-data` launch fails on its lock), reports success.
- Concurrent `launch_emulator` calls can both return the same newly-online serial.

**Evidence:**
- [APK resolver](../src-tauri/src/services/build_runner.rs#L333)
- [applicationId fallback](../src/services/build.service.ts#L351)
- [AVD-to-serial matching](../src/stores/device.store.ts#L83)
- [emulator stop](../src-tauri/src/services/adb_manager.rs#L672)
- [wipe](../src-tauri/src/services/adb_manager.rs#L1009)

**Minimal first fix**

- Resolve outputs from AGP's `output-metadata.json` (`variantName`, `outputFile`). If it is absent, match the concatenated path segments below `apk/` against the variant. **Remove the cross-variant fallback passes when a variant is given.** Missing or ambiguous output returns an actionable error.
- If aapt2 fails, error rather than guessing the package (or apply the suffix from the variant model).
- Resolve AVD names in the backend (`adb -s <serial> emu avd name`, or `getprop ro.boot.qemu.avd_name`) during `list_devices`, and match exactly. Display names remain presentation only.
- Check the exit status in `stop_emulator`. Correlate wipe/startup readiness using the existing before/after diff (`newly_online_emulator_serial`) plus the AVD name.
- Initially support one clearly identified APK output. Add split-APK installation deliberately, with tests.

**Later, if needed:** an `ArtifactDescriptor` (project, module, variant, producing build ID, canonical paths, package identity), once F1/F2 need to persist artifact identity.

**Acceptance criteria:**
- Tests cover multiple flavors, stale outputs, missing variants, multiple application modules, `applicationIdSuffix`, ambiguous outputs, a running AVD whose model differs from its name, simultaneous emulator launches, and a failed emulator stop.
- Every successful operation identifies the exact artifact and device it affected.

### R3. Complete Filesystem Boundary Enforcement and Make Source Edits Conflict-Safe

**Priority:** P0 for R3a (webview permissions, a quick fix); P2 for the rest. **Verification:** Confirmed. The remaining items need a hostile or unusual repository to trigger, and their impact is bounded.

**R3a — webview filesystem permissions (underplayed in revision 1).**
- `capabilities/default.json` grants `fs:allow-remove`, `fs:allow-rename`, `fs:allow-mkdir`, `fs:allow-read-dir` and `fs:allow-read-file`.
- The backend adds **recursive** scopes for every opened project and for the SDK directory.
- The frontend only uses `writeTextFile` (logcat export).
- A webview compromise (for example a rendering bug in untrusted log content) could therefore delete a project or the SDK.
- `tauri-plugin-shell` with `shell:default`/`shell:allow-open` is also granted, but no frontend code imports the shell plugin.

**Remaining boundary gaps.**
- **APK validator.** It checks containment inside canonical outputs without first verifying that canonical outputs remain inside the canonical project. If `app/build` is a symlink to `~`, any `.apk` under home passes. The duplicate MCP validator has the same gap, then passes the *non-canonical* path to adb (check-then-use).
- **MCP resources.** They read four fixed project files without symlink checks or size caps.
- **App Info writes.**
  - They use first-match `Regex::replace`, so a commented `// versionCode 1` can be edited instead of the real assignment.
  - Catalog references (`versionCode = libs.versions…`) do not match at all, yet the save returns `Ok` with nothing changed.
  - They use a fixed temporary filename, and the rename drops the original file mode.

**Evidence:**
- [capabilities](../src-tauri/capabilities/default.json)
- [project scope grant](../src-tauri/src/commands/file_system.rs#L140)
- [SDK scope grant](../src-tauri/src/commands/settings.rs#L44)
- [APK boundary](../src-tauri/src/utils/path.rs#L63)
- [MCP resource reads](../src-tauri/src/services/mcp_server.rs#L2893)
- [MCP APK validator](../src-tauri/src/services/mcp_server.rs#L2937)
- [Project App Info writes](../src-tauri/src/commands/file_system.rs#L441)

**Minimal first fix**

- **R3a:**
  - reduce `fs` permissions to `allow-write-text-file` scoped to the export destination (or route export through a backend command);
  - remove the `allow_directory` calls;
  - remove `tauri-plugin-shell` if nothing uses it.
- **APK paths:** add `canonical_outputs.starts_with(canonical_root)` to the shared validator. Make MCP call the shared validator and use its returned canonical path.
- **Resources:** canonicalize, check containment, and cap the size of resource reads.
- **App Info:** error when there are zero or several non-comment matches, and when the file would not change. Use a unique temporary file and preserve permissions.

**Later, if needed:**
- file revision/hash checks and a reviewable change preview;
- explicitly approved external build roots.

The current save re-reads the file and replaces only two fields, so concurrent Android Studio edits elsewhere in the file are already preserved.

**Acceptance criteria:**
- Tests cover symlinks at each ancestor, traversal, internal symlinks, changed targets, duplicate or commented assignments, unsupported expressions, and write failures.
- A rejected save leaves the original file intact.
- The webview cannot delete or rename files.

**Uncertainty:** No exploit was executed against a user project, and the review does not claim one occurred.

### R4. Give Builds, Deployments, and Project Changes Explicit Operation Ownership

**Priority:** P1. **Verification:** Confirmed, and **worse than originally stated**.

**Backend.**
- `cancel_build` clears `current_build`/`starting` and returns immediately after SIGTERM; SIGKILL follows 5 s later.
- When cancelled build A finalizes late, `record_build_result` calls `take_active_process_id()`, which **takes build B's process ID**. It then sets `current_build = None` and `starting = false`, and writes A's outcome into the shared status.
- Consequences:
  - B can no longer be cancelled;
  - a third concurrent Gradle build can start;
  - B's status shows A's result.
- Likely trigger: an MCP agent calling `cancel_build` then `run_gradle_task`.

**Recording.**
- `record_build_result` writes `Failed` whenever `!success`, overwriting `Cancelled`, so history cannot distinguish a cancel from a failure.
- B clears the shared `build_log`, and A's late finalization snapshots B's lines into A's persisted log.

**Frontend.**
- Cancellation clears its completion resolver before awaiting the cancel IPC. On rejection the original promise stays pending, and every later build throws "already running". This is low likelihood, because the Rust command always returns `Ok`.
- `BuildCompleteEvent` has no build identity, so A's late `cancelled` event resolves B's waiter and flips B's UI to cancelled.

**Project and deploy.**
- `selectProject`/`openProjectFolder` run history, variant, and device restoration after `doOpenProject` without a generation check. On a rapid A→B switch, A's saved variant can land while B is open and then be saved into B's registry entry.
- Deploy resolves `findApkPath` and `installApkOnDevice` against backend `FsState` at call time, so switching projects mid-build installs from the new project's outputs.
- Cancel remains visible during install/launch, but `cancelBuild` returns early unless the phase is `running`, so it is a silent no-op.

**Evidence:**
- [backend cancellation](../src-tauri/src/services/build_runner.rs#L547)
- [finalization takes the active process ID](../src-tauri/src/services/build_runner.rs#L603)
- [completion event](../src-tauri/src/services/build_runner.rs#L439)
- [frontend cancellation](../src/services/build.service.ts#L381)
- [completion handling](../src/services/build.service.ts#L67)
- [project restoration](../src/services/project.service.ts#L187)

**Minimal first fix**

- Use the existing `ProcessId` (already returned by `run_gradle_task`) as the build run ID. Add it to `BuildCompleteEvent` and to the output batches.
- In `record_build_result`:
  - change `status`, `current_build`, `active_process_id`, and `build_log` only when they still belong to this run;
  - append to history unconditionally;
  - record `Cancelled` when the run was cancelled, and `TimedOut` when it timed out.
- Keep each run's log buffer per run, or snapshot it at cancel time.
- In the frontend:
  - match completion events by ID;
  - wrap the cancel IPC in `try/finally` so every waiter settles exactly once;
  - hide Cancel during install/launch until deploy cancellation exists.
- Carry the project-open generation (`isCurrent`) through `selectProject` and `reloadVariantsAndRestoreMeta`. Capture the project root at deploy start and pass it explicitly, or block project switching while a deploy is in flight.

**Later, if needed:**
- a `ProjectSessionId`;
- an explicit state machine (starting, running, cancelling, succeeded, failed, cancelled, timed out);
- a separate deploy lifecycle with in-flight install cancellation.

**Acceptance criteria.** The regression suite should deliberately:

- Cancel A, start B, then deliver A's output and completion.
- Reject cancellation IPC.
- Complete a build before its start reply arrives.
- Switch projects at every awaited restoration/deployment boundary.
- Cancel while selecting a device, building, installing, and launching.
- Deliver duplicate completion events.

No scenario may mix projects, strand a promise, corrupt the next run, or produce two terminal history entries. A cancelled build is recorded as cancelled.

### R5. Isolate Tests from User Data and Make Persistence Transactional

**Priority:** **P0** (test isolation); P1 (cross-process persistence, R5b). **Verification:** Confirmed, **and it has already caused data loss.**

**Tests write the real data directory.**
- `BuildState::new()` calls `load_build_history()` against the real data directory.
- It is used in `tests/build_integration.rs` and in about 20 unit tests in `build_runner.rs` and `mcp_server.rs`.
- `record_build_result` writes history and logs and runs retention. `clear_history_empties_the_deque` writes an empty history to disk.

**Observed on the reviewer's machine (2026-09-23 11:20):**
- `~/.keynobi/build-history.json` contained only fixtures from `record_build_result_respects_history_limit` (`task_2`…`task_11`, `startedAt 2024-01-01`, IDs 43–52).
- `build-logs/` held only 0-byte `build-43..52.jsonl` files, with 45 missing from a parallel-test race.
- The user's real history was overwritten, and rotation deleted the real logs as orphans.

**Cross-process persistence (R5b).**
- GUI and headless MCP share settings and history files, but `SETTINGS_MUTATION_LOCK` is a process-local static and history has no lock.
- Temporary filenames are fixed (`settings.json.tmp`, `build-history.json.tmp`).
- `next_id` is `max(history) + 1`, computed once per process, so build IDs collide across processes.
- **Worse than stated:** `rotate_build_logs` deletes every `build-N.jsonl` whose ID is not in *its own process's* history. One process's completion therefore deletes the other's logs.
- **Lost updates within one process:** the frontend saves the whole `settingsState` snapshot, including `recentProjects`, so backend `mutate_settings` edits (`recent_projects`, `last_active_project`, MCP's `last_build_variant`) are reverted by the next GUI save.

**Evidence:**
- [default data directory](../src-tauri/src/services/settings_manager.rs#L88)
- [integration test using persisted history](../src-tauri/tests/build_integration.rs#L178)
- [completion persistence](../src-tauri/src/services/build_runner.rs#L641)
- [log rotation](../src-tauri/src/services/build_runner.rs#L204)
- [settings mutation](../src-tauri/src/services/settings_manager.rs#L226)

**Minimal first fix**

- **P0:**
  - add a data-directory override (an injected root, or `KEYNOBI_DATA_DIR` read once) plus a `BuildState::in_memory()` constructor;
  - point every test at a unique temporary directory;
  - add a CI guard that fails if a test touches `$HOME/.keynobi` (for example, run tests with `HOME` set to a temporary directory).
- **Separate binding generation from the full test suite,** so `generate:bindings` does not run persistence tests.
- **R5b:**
  - add an advisory `flock` on `~/.keynobi/.lock` around history and settings writes;
  - re-read history before appending;
  - allocate IDs under the lock;
  - scope log rotation to IDs from the merged on-disk history;
  - use unique temporary files;
  - make `save_settings` ignore backend-owned fields and re-read them from disk under the lock.
- Return persistence errors explicitly; do not discard `spawn_blocking` results.
- Inject a clock into retention logic.

**Later, if needed:** injected `AppPaths` with settings/history repositories and revisioned settings patches. R6 routing MCP through the GUI reduces, but does not remove, the need; standalone headless mode remains.

**Acceptance criteria:**
- Repeated and parallel test runs leave `$HOME/.keynobi` untouched, and a CI check enforces it.
- Two processes updating different settings fields preserve both changes.
- Concurrent build completions never reuse IDs or delete each other's logs.
- Crash-mid-write and permission failures produce recoverable, visible outcomes.

**Recovery note:** Users affected by past test runs cannot recover the overwritten history. Consider mentioning this in the release notes of the fix.

### R6. Make GUI and MCP State Ownership Match the Product Promise

**Priority:** P1. **Verification:** Confirmed.

**Current behavior.**
- The standard setup command launches `keynobi --mcp`, which creates fresh `FsState`, `BuildState`, `DeviceState`, `LogcatState` and `ProcessManager`. It does not attach to the running GUI.
- Headless mode reads `last_active_project` once at launch, so later GUI project switches are invisible to the agent. This contradicts PITCH.md ("using the same project you have open in Keynobi").
- `MCP_SERVER.md` and `DOMAIN_PATTERNS.md` describe a "GUI mode with shared state" that users cannot actually reach. Test comments claiming "UI and MCP share one build slot" hold only in that unreachable mode.

**Newly found.**
- **GUI-mode MCP is effectively dead code.** `start_mcp_server` serves `rmcp::transport::stdio()` on the GUI process's own stdin/stdout, and `settings.mcp.auto_start` runs it automatically. A GUI launched from Finder has no client on stdio.
- **The MCP PID file is single-slot.** With two clients connected (for example Claude Code and Codex), the first to exit removes the file, and the GUI then reports MCP as not running. Stale PIDs can also be reused.
- **Activity-log rotation rewrites the file non-atomically** while other processes append to it.

**Evidence:**
- [headless entry point](../src-tauri/src/main.rs#L9)
- [independent state creation](../src-tauri/src/services/mcp_server.rs#L3199)
- [GUI stdio transport](../src-tauri/src/services/mcp_server.rs#L3174)
- [PID file](../src-tauri/src/services/mcp_activity.rs#L154)

**Recommended design (replaces revision 1's "local coordinator")**

1. The GUI listens on a Unix domain socket at `~/.keynobi/mcp.sock` (socket 0600, directory 0700).
   - Each connection is served by `AndroidMcpServer::from_app_handle` on the GUI's real managed state.
   - rmcp's async read/write transport (already enabled through `transport-io`) serves each connection as its own session, so multi-client support comes without extra work.
2. `keynobi --mcp` first tries to connect to the socket.
   - **If that works,** it becomes a byte pipe (`tokio::io::copy_bidirectional` between stdio and the socket).
   - **If not,** it runs standalone as today, but says so explicitly in `serverInfo`/`instructions`, the activity log, and `get_project_info`.
   - An `--attach-only` flag provides a hard failure for users who want it.
3. Existing client registrations keep working unchanged. The GUI's lifetime is the coordinator's lifetime, so no separate daemon is needed.
4. Delete the stdio GUI mode and the `mcp.auto_start` setting. Replace the PID file with per-session liveness records.
5. In standalone mode, use R5b's lock for build-slot exclusion, and prefer the agent's working directory over a stale `last_active_project` when it contains a Gradle root (see R16).

**Alternatives considered:**

| Alternative | Why not |
| --- | --- |
| Streamable HTTP hosted by the GUI | Needs different client configuration, fails when the GUI is closed, and adds localhost-authentication concerns. |
| A lock/lease alone | Prevents conflicting builds, but cannot make an MCP build appear in the GUI. |

**Acceptance criteria:**
- A build started from an MCP client appears in the GUI with the same ID, progress, outcome, and artifact.
- Competing starts receive a consistent busy response.
- Connecting two clients and disconnecting one does not invalidate the other.
- Standalone mode is always labelled as such.
- Project identity is always visible and unambiguous.

**Decided:** see [D8](#blocking-phase-2). The socket bridge is approved, with the policies recorded there.

### R7. Make Logcat Identity and Snapshot/Stream Reconciliation Reliable

**Priority:** P1. **Verification:** Confirmed, and broader than stated.

**Duplicate entry IDs.**
- `PipelineContext` initializes `next_id: 1` and is rebuilt inside the reconnect loop.
- The store is **not** cleared on reconnect, on stop→start, or on device switch (`start_logcat` never calls `store.clear()`).
- All three paths therefore produce duplicate IDs in one ring. `index_of` binary-searches keys that are not sorted, and the crash-group counter also restarts.
- The frontend deduplicates context expansion by ID and selects rows by ID, so rows are dropped or mis-selected.
- This is a regression from HARDENING 5.5 (see above).

**Stale backfill after Clear.**
- `syncBackendFilter` guards only against newer filter syncs.
- Neither the Clear handler nor the `logcat:cleared` listener invalidates the guard, so an in-flight backfill can restore entries after Clear.
- The `onMount` backfill has the same hole.

**Wrong device on auto-start.** Auto-start chooses the first online device instead of the selected one; manual start uses `selectedDevice()`.

**Evidence:**
- [ID initialization](../src-tauri/src/services/log_pipeline.rs#L53)
- [reconnect context](../src-tauri/src/services/logcat.rs#L573)
- [ordered-ID assumption](../src-tauri/src/services/log_store.rs#L178)
- [`logcat:cleared` listener](../src/components/logcat/LogcatPanel.tsx#L612)
- [Clear handler](../src/components/logcat/LogcatPanel.tsx#L746)
- [auto-start device choice](../src/components/logcat/LogcatPanel.tsx#L633)

**Minimal first fix** (this closes every confirmed bug):

- Move `next_id` (and the crash-group counter) into `LogcatStateInner` or `LogStore`, so IDs survive reconnects, new contexts, and device switches.
- Invalidate `filterSyncGuard` on Clear and on `logcat:cleared`, and guard the mount backfill.
- Make auto-start prefer `selectedDevice()`.
- Apply the ID fix to both the Tauri command and MCP `start_logcat` paths. Better still, extract `services::logcat::{request_start, request_stop, request_clear}` to remove the "KEEP IN SYNC" duplication.

**Later, after a demonstrated missed-row reproduction:**
- stream envelopes carrying device, session, sequence, and clear epoch;
- filter revisions;
- snapshot cursors with subscribe-before-snapshot reconciliation;
- explicit starting/live/reconnecting/stopped/failed states;
- bounded PID enrichment;
- a testable logcat-session controller.

**Acceptance criteria:**
- Reconnect, stop/start, and device switch keep IDs unique.
- Context expansion and row selection remain correct.
- Clear cannot resurrect entries.
- Events arriving during backfill are neither lost nor duplicated.
- Switching devices cannot present old-device logs as the new device's stream.

### R8. Enforce the Telemetry Privacy Promise with an Explicit Allowed Payload

**Priority:** P1. **Verification:** Confirmed, and wider than cited. This is a control weakness, **not evidence of an actual leak**.

**Browser.**
- The scrubber preserves free-form messages, exception values, tags, and most context.
- Sentry's default GlobalHandlers integration captures unhandled promise rejections. Tauri `invoke` rejections are strings such as `"Failed to … /Users/…/project"`, and the codebase has many fire-and-forget `void asyncFn()` calls. Those strings reach `exception.value` unscrubbed.
- Release builds ship a browser DSN.

**Native.**
- The scrubber replaces only the home path and `/var/folders` temporary paths.
- The panic integration is enabled. Panic messages can embed arbitrary data (slice contents, `unwrap` error text), including logcat text, package names, serials, and paths outside home.
- Opt-out requires a restart: `before_send` never re-checks consent, so panics keep being reported until then.

**Newly found.** After an off → on → off consent sequence in one session, web telemetry silently stays disabled. `Sentry.close()` disables the client, but `isInitialized()` stays true, so it is never re-initialized. This fails safe (no data is sent), but it is a functional bug.

**Evidence:**
- [browser scrubber](../src/lib/telemetry/sentry-web.ts#L55)
- [re-initialization guard](../src/lib/telemetry/sentry-web.ts#L120)
- [error capture](../src/components/common/ErrorBoundary.tsx#L53)
- [native telemetry](../src-tauri/src/services/telemetry_sentry.rs)

**Minimal first fix**

- Make browser `beforeSend` keep only an allowed set of fields: error type, release, platform, known error code, and sanitized in-app frames. Drop `message`, `exception.value`, tags, breadcrumbs, and contexts by default.
- Make native `before_send` check an `AtomicBool` that `save_settings` updates, so opt-out takes effect immediately. Reduce panic payloads to their type or location.
- Fix re-initialization after close, and serialize init/close under rapid toggling.
- Test serialized envelopes using synthetic secrets, paths, URLs, package names, serials, and log contents.

**Later, if needed:** a local fake collector for transport tests.

**Acceptance criteria:**
- Synthetic sensitive strings cannot survive outbound serialization.
- Opt-out stops subsequent sends, native and web, without a restart.
- Rapid toggling is deterministic.

**Uncertainty:** Production telemetry payloads were not inspected. Native `capture_internal_error` has no production caller.

### R9. Supervise Subprocesses and Reconcile Device State Correctly

**Priority:** P1 for R9a (EOF hang and adb timeouts); P2 for the rest. **Verification:** Confirmed.

**R9a — hangs.**
- `process_manager` waits for stdout/stderr EOF before calling `child.wait()`. A descendant that keeps the pipe open (for example a Gradle daemon) prevents the exit from ever being observed. The build then never finalizes, and the build slot stays stuck. This is the same class as the CRITICAL self-review defect in the Hardening Plan.
- `adb_manager.rs` has **no timeouts at all**. Affected calls include:
  - `list_devices`, which runs every 3 s in the poll loop, so a hung adb freezes polling;
  - getprop enrichment, `install_apk`, `launch_app`, `stop_app`, `stop_emulator`, `resolve_device_serial`, aapt2, `avdmanager`, `sdkmanager`.
- `device_inspector.rs`, `app_inspector.rs`, `health_inspector.rs`, and `commands/health.rs` also have no timeouts.

**Device reconciliation.**
- **Polling compares serial lists only,** so a device changing from unauthorized to online with the same serial emits nothing.
- **Selection rollback race** (introduced by `11c95fd`): `pickDevice` and `selectVariant` capture `previous` before the await, so a late failure can roll back a newer, successful selection.
- **Duplicate poll loops:** `stop_device_polling` followed by `start_device_polling` within 3 s leaves two loops running, because both check only a `polling` bool.
- **Stale adb path:** the polling task captures the `adb` path once, so SDK setting changes do not reach it. Most other commands reload settings on every call.
- **Raw PIDs:** `process_manager::cancel` still signals a raw PID (a Hardening Plan follow-up).

**Evidence:**
- [device comparison](../src-tauri/src/commands/device.rs#L243)
- [process lifecycle](../src-tauri/src/services/process_manager.rs#L156)
- [device selection rollback](../src/stores/device.store.ts#L142)
- [variant selection rollback](../src/stores/variant.store.ts#L253)

**Minimal first fix**

- **R9a:**
  - observe child exit independently of output EOF, and cap post-exit draining (for example 2 s);
  - add a single `run_with_timeout(cmd, duration)` helper with `kill_on_drop(true)`, and route every `adb_manager`, inspector, and health call through it with per-operation deadlines.
- Compare `(serial, state)` snapshots when polling.
- Use a revision counter in `pickDevice`/`selectVariant`: only the latest request may roll back.
- Give the polling task a generation token, so a stale loop exits.

**Later, if needed:**
- an injectable process supervisor (termination acknowledgement, descendant-tree policy separate from shared Gradle daemons, a bounded shutdown routine for GUI exit and MCP disconnect);
- holding `Child` handles instead of raw PIDs;
- separating device discovery from slower metadata enrichment.

**Acceptance criteria:** Tests cover:
- hung ADB;
- ignored termination;
- descendants retaining pipes;
- app close during a build;
- MCP disconnect during work;
- USB authorization without unplugging;
- rapid polling restart;
- out-of-order selection replies.

### R10. Finish Resource Budgets and Establish Trustworthy Performance Measurements

**Priority:** P2. **Verification:** Confirmed; package/PID map growth is low severity.

**Unbounded buffers and work.**
- `BufReader::lines()` has no per-line byte cap in either process output or logcat ingestion; `MAX_BUILD_LOG` counts lines only.
- Structured build errors (`errors_buf`) are uncapped.
- Package/PID maps are cleared only on logcat clear, not on process death.
- The pipeline drains `while try_recv()` with no row or time budget. The channel is capped, but the producer refills it concurrently.
- MCP activity reads load and parse the whole file before `take(limit)`, and rotation runs only at startup.

**Untrustworthy metrics.** The collector reuses an existing `dist/`, stale criterion output, and whichever release/debug binary exists. It then labels the results with `git rev-parse HEAD` without checking for a dirty tree.

**Evidence:**
- [process line ingestion](../src-tauri/src/services/process_manager.rs#L156)
- [build error buffer](../src-tauri/src/services/build_runner.rs#L750)
- [pipeline draining](../src-tauri/src/services/log_pipeline.rs#L174)
- [activity reads](../src-tauri/src/services/mcp_activity.rs#L89)
- [metrics collector](../scripts/collect-metrics.mjs#L117)

**Minimal first fix**

- Cap line bytes (with a truncation marker), `errors_buf`, and per-tick drain (rows or elapsed time).
- Read MCP activity from a bounded tail and rotate it continuously.
- Record commit and dirty-tree state in metrics; rebuild or reject stale artifacts.

**Later:**
- define item/byte/time budgets for every long-lived buffer;
- record full benchmark provenance (profile, architecture, toolchain, hardware, fixture, artifact hash);
- add sustained and burst workloads;
- measure RSS, event latency, frame delays, and dropped counts.

Establish baselines before enforcing thresholds.

**Acceptance criteria:**
- A 30–60 minute soak at 1,000 lines/second keeps RSS below a threshold set from the Phase 0 baseline, and logcat drop counts are reported.
- A 10 MB single line is truncated visibly.
- Reports identify exactly which artifacts were measured.

### R11. Strengthen Contracts, Native Tests, and Release Verification

**Priority:** P2 (the CI and release quick fixes belong in Phase 0.5). **Verification:** Confirmed; dependency scanning only partly true.

**Tests and contracts.**
- Browser E2E runs in Chromium against the mock backend.
- The IPC contract guard only checks command names (registered ⊇ invoked, mocked ⊇ invoked). It does not check arguments, responses, channels, or events.
- `@vitest/coverage-v8` is not installed, although `vite.config.ts` configures it, so `npm run test:coverage` fails.
- Several tests hand-copy production effects instead of exercising them (`LogcatPanel.auto-filter.test.ts`, `LogcatPanel.mine-filter.test.ts`, `BuildPanel.test.ts`).

**CI and release.**
- **Binding verification swallows errors:** `cargo test --lib 2>/dev/null | grep -q "test result" || true`. A failing test run passes the `git diff` check on stale bindings, and the release gate depends on this CI result.
- **The release never verifies its artifact.** The workflow goes build → upload → `gh release create`, with no `hdiutil attach`, `codesign --verify`, `spctl`, `stapler validate`, or launch smoke test.
- **Tag publishing is not retry-safe.** A re-run after a successful push fails.
- **No published checksums or provenance.**
- **Partial dependency scanning.** Dependabot covers version updates, but there is no `cargo audit`/`cargo deny`, `npm audit`, or CodeQL.

**Evidence:**
- [browser configuration](../playwright.config.ts#L24)
- [native IPC harness](../src-tauri/tests/ipc/tauri_commands.rs#L39)
- [contract guard](../scripts/ipc-contract.test.mjs#L87)
- [coverage configuration](../vite.config.ts#L79)
- [binding check](../.github/workflows/ci.yml#L67)
- [release build and publication](../.github/workflows/release.yml#L183)

**Minimal first fix (Phase 0.5):**
- install `@vitest/coverage-v8`;
- remove `|| true` and generate bindings with a dedicated, non-persisting test target (see R5);
- make tag creation idempotent (skip if the tag exists at the validated SHA; fail if it exists elsewhere);
- add `codesign --verify --deep --strict`, `spctl -a`, `stapler validate`, and SHA-256 checksums to the release job;
- add `cargo audit` and `npm audit --omit=dev` with reviewed exceptions.

**Tests and contracts (later):**
- Make unexpected IPC calls fail in unit tests unless explicitly stubbed.
- Add scenario fixtures for delayed replies, rejected cancellation, disconnects, malformed output, and duplicate or out-of-order events.
- Generate representative serialized fixtures from Rust and consume them in TypeScript contract tests.
- Replace copied-effect tests with tests of the production controllers.
- **Priority native harness:** launch the real headless binary against fake `adb`/`gradlew` executables for protocol and lifecycle tests. This gives more value per hour than GUI automation.
- Introduce a measured coverage ratchet for critical modules once reporting works.
- **GUI automation trial:** `tauri-driver` has historically not supported macOS (no WKWebView driver). Revision 1's claim that a WebdriverIO embedded driver works on macOS is **unverified**; confirm it against current [Tauri testing documentation](https://v2.tauri.app/develop/tests/webdriver/) before planning around it. Keep any test-driver capability out of distributable builds. **Decided (D12):** defer GUI automation; use the headless harness plus a scripted manual checklist.

**Releases (later):**
- a DMG mount + launch + MCP handshake smoke test;
- a documented recovery path for a faulty release (see R22 for the updater).

**Acceptance criteria:**
- A deliberately broken serializer or stale event fails tests.
- A failing Rust test fails the binding check.
- Native smoke covers startup, settings persistence, project selection, cancellation, shutdown, and reopen.
- A failed release publication can be retried without deleting or moving tags.

**Uncertainty:** Repository branch-protection settings, Dependabot security alerts, signing credentials, and the supported macOS/architecture matrix were not verified.

### R12. Make UI Promises, Accessibility, and Diagnostics Consistent

**Priority:** P2 (several items are Phase 0.5 quick wins). **Verification:** Confirmed.

- **Unused setting:** "Auto Install on Build" is read only by the settings panel; the Rust default is `true` and nothing consumes it.
- **Wrong Problems for historical builds:** selecting a historical build still shows the current build's Problems (`buildState.errors`).
- **Failures shown as "missing":** a failed historical-log load falls through to an empty list and shows "No log saved for this build".
- **Keyboard gaps:** project/device rows and hover-only actions are not consistently keyboard accessible.
- **Dialog:** the shared dialog has no keydown handler, autofocus, focus restore, or focus trap.
- **Stale health:** the health presentation does not fully reflect the available backend checks, and GUI and MCP health probes disagree (see R17).
- **Log rotation and shutdown:**
  - rotation sorts all `app.log*` files, including the active one, and subtracts sizes even when removal fails;
  - the logging guard is `mem::forget`-ed, so the non-blocking buffer is never flushed on exit.

**Evidence:**
- [settings option](../src/components/settings/SettingsPanel.tsx#L550)
- [historical Problems view](../src/components/build/BuildPanel.tsx#L330)
- [history load failure](../src/components/build/BuildPanel.tsx#L68)
- [dialog](../src/components/ui/Dialog/Dialog.tsx#L56)
- [log rotation](../src-tauri/src/services/monitor.rs#L38)
- [forgotten logging guard](../src-tauri/src/lib.rs#L165)

**Implementation**

- **Phase 0.5:**
  - remove or migrate the unused automatic-install option;
  - add Escape, autofocus, focus trap, and restore to the shared dialog;
  - show a load error distinct from "no log";
  - exclude the active log from rotation, and count only successful removals;
  - hold the logging guard and flush it during shutdown.
- Make history selection switch logs, problems, status, timing, project, variant, and device together.
- Distinguish loading, missing, expired, and failed artifacts.
- Provide semantic, focusable project/device selection, and actions available by keyboard.
- Scope global shortcuts so background actions do not fire through dialogs.
- Version health reports by project/environment context, and refresh the latest requested context.
- Use stable operation/error codes and actionable recovery messages.

**Acceptance criteria.** Scripted Playwright keyboard flows (not only manual checks) show that a keyboard-only user can:
- select projects and devices;
- run and cancel;
- inspect history;
- manage dialogs.

Focus returns correctly. Historical errors belong to the selected build. Health cannot report a healthy current project using a stale result. Shutdown diagnostics remain readable after reopening.

### R13. Extract Production Seams Where They Enable the Missing Tests

**Priority:** P3, supporting work only.

Large files alone are not defects. The useful extractions are the ones that make ownership and adverse ordering testable. With a single maintainer, **extract only when a fix in R1–R22 needs the seam**; do not pre-build the full table.

| Boundary | Responsibility | Triggered by |
| --- | --- | --- |
| Logcat request functions | Shared start/stop/clear for Tauri and MCP (Hardening follow-up) | R7 |
| `run_with_timeout` / process helper | Deadlines, `kill_on_drop`, exit status | R9a, R19 |
| `adb_shell_argv` helper | Quoted device-shell invocation | R14 |
| Artifact resolver | Exact module/variant/output identity | R2 |
| Settings/history persistence | Lock, re-read, unique temporary files, retention | R5b |
| Build run ownership | Run ID checks in finalization | R4 |
| Tauri/MCP adapters | Validation, transport, error translation | R14, R17, R20 |

The Tauri/MCP adapter duplication is a **correctness** issue, not only maintainability, because the two paths already diverge in safety-relevant ways: `launch_app` activity validation (R14), health probes (R17), and `clear_logcat` (fixed during hardening). Splitting `mcp_server.rs` (3,504 lines) should follow these extractions, not precede them.

Each extraction should include characterization tests, one migrated caller at a time, documented invariants, and removal of the duplicated path once both adapters use it. Update the project instructions and manuals where they describe obsolete behavior.

---

## New Findings in Revision 2 (R14–R23)

### R14. Quote Every `adb shell` Argument

**Priority:** P0. **Verification:** Confirmed from code. The adb client joins everything after `shell` with spaces and does no escaping ("just like ssh(1)"), and the device's `/system/bin/sh` parses the resulting string again.

`validation.rs` states that arguments are "never [passed] through a shell". That is true for the host, but not for anything sent through `adb shell`. `MCP_SERVER.md` repeats the claim.

**`ui_type_text` / `ui_fill_input`.** `encode_adb_input_text` rewrites only `%` and space. It passes `` ; & | $ ( ) ` ' " < > * `` through unchanged:

| Input | Result on the device |
| --- | --- |
| `it's` | Fails with an unterminated quote. |
| `P@ss&word` | Truncated at `&`. |
| `*` | Glob-expanded against the device's current directory. |
| `x;reboot` | Runs `reboot`. |

**Other paths.**
- **`open_deep_link`:** `validate_deep_link_uri` checks only the scheme. Any URL with a query string (`https://x/?a=1&b=2`) is cut off at `&`, which breaks the most common deep-link form, and `;cmd` injects.
- **MCP `launch_app`:** the `activity` parameter is not validated before interpolation into `am start`. The GUI path does validate it, so the two front doors diverge.
- **`ui_type_text_unicode` is effectively broken.** It passes `["sh", "-c", clip_cmd]`, which the joining turns into `sh -c am …`. The intended clipboard write does not run as written, and the paste step can insert whatever was already on the clipboard. The `content://…clipboard/primary` fallback provider may not exist on stock AOSP.

**Evidence:**
- [validator claim](../src-tauri/src/utils/validation.rs#L3)
- [input text encoding](../src-tauri/src/services/ui_automation.rs#L1306)
- [deep-link validation](../src-tauri/src/services/ui_automation.rs#L1489)
- [Unicode typing](../src-tauri/src/services/ui_automation.rs#L1803)
- [MCP `launch_app`](../src-tauri/src/services/mcp_server.rs#L2427)

**Minimal first fix**

- Add a single `adb_shell_argv` helper that single-quotes every argument (`'` → `'\''`) before invoking adb. Route every `adb shell` call through it.
- For `input text`, additionally escape the characters that `input` itself interprets.
- Validate `activity` in the MCP path with the shared validator.
- Rewrite Unicode typing to send one pre-quoted command. Verify the clipboard by reading it back, and fail loudly if it does not match.
- Correct the comment in `validation.rs` and the claim in `MCP_SERVER.md`.

**Acceptance criteria:**
- Injection tests for every shell-reaching tool, covering quotes, `&`, `;`, `$()`, backticks, globs, and URLs with query strings.
- The typed text on an emulator equals the input exactly.
- Unicode typing succeeds, or fails with an explicit error.

### R15. Restrict `run_gradle_task` and Annotate MCP Tools

**Priority:** P0. **Verification:** Confirmed from code.

**Flags accepted as tasks.** `validate_gradle_task` permits a leading `-`, and the value is passed as argv to `gradlew`. An agent can therefore pass options such as:
- `-Iinit.gradle` (an init script in the project directory);
- `--offline`, `--refresh-dependencies`, `--write-locks`;
- `--stop`, `-Pkey=value`, `--scan`.

**No task allowlist.** Tasks with external side effects are accepted, for example `publish`/`publishReleaseBundle` (Maven, Play Publisher), `uninstallAll` (uninstalls from every connected device), or custom deploy tasks.

**No annotations.** No MCP tool declares `readOnlyHint`/`destructiveHint`, although rmcp 3.1.2 supports `#[tool(annotations(...))]`. Clients therefore cannot apply their own confirmation policies.

**Evidence:**
- [Gradle task validator](../src-tauri/src/utils/validation.rs#L21)
- [Gradle invocation](../src-tauri/src/services/build_runner.rs#L747)

**Minimal first fix**

- Reject a leading `-` in task names.
- Deny `publish*`, `upload*`, `uninstall*`, and `closeAndRelease*` by default, overridable by an explicit setting (for example `mcp.allowUnrestrictedGradle`, off by default).
- Consider adding a narrower `build_variant` tool (assemble/bundle/test/lint for a variant), and keep `run_gradle_task` for advanced use.
- Annotate every MCP tool:

  | Annotation | Tools |
  | --- | --- |
  | Read-only | Inspection tools, `get_*`, `list_*`, `find_*` |
  | Destructive | `restart_app` with `clear_data`, `stop_avd`, `revoke_runtime_permission`, `set_network_state`, `install_apk`, `clear_logcat` |
  | Open-world | `run_gradle_task` |

**Acceptance criteria:**
- Flag-shaped and denied tasks return a clear error with no process spawned.
- Tool listings include annotations, and a test asserts every tool is annotated.

### R16. Require Project Trust Before Executing Project Build Code

**Priority:** P1. **Verification:** Confirmed from code.

**Build code runs on open.** Every project open triggers variant loading, which runs `chmod +x gradlew` followed by `./gradlew :app:tasks --all`. A freshly cloned repository's `gradlew` and build scripts therefore execute automatically. Android Studio asks "Trust project?" first.

**Headless MCP chooses a stale project.** It picks its project as `--project` → `last_active_project` → current directory. It therefore prefers a stale GUI project over the agent's working directory.

**Evidence:**
- [variant task listing](../src-tauri/src/commands/variant.rs#L113)
- [headless project choice](../src-tauri/src/services/mcp_server.rs#L3212)

**Minimal first fix**

- Keep a trusted-projects list, and prompt on first open.
- Until a project is trusted, use static variant parsing only (`build_inspector` already parses flavors and build types), and do not run `gradlew`.
- In headless mode, prefer the current directory when it contains a Gradle root; otherwise use `last_active_project`, and report which was chosen.

**Acceptance criteria:**
- Opening an untrusted project spawns no process.
- Trust is persisted per canonical root.
- The MCP `get_project_info` result states how the project was selected.

### R17. Make JDK Detection Reliable and Unify Health Probes

**Priority:** P1 (first-run build failures). **Verification:** Confirmed from code.

**JDK choice.**
- `detect_java_home` takes the *first* `read_dir` entry in `/Library/Java/JavaVirtualMachines`, which could be JDK 8 or 11 (AGP 8 needs 17 or newer).
- It never considers Android Studio's bundled JBR (`/Applications/Android Studio.app/Contents/jbr`), which is what many Android developers actually use, or `org.gradle.java.home`.

**Health probes disagree.**
- The GUI health check treats `!stderr.is_empty()` as "Java found". The macOS `/usr/bin/java` stub prints "Unable to locate a Java Runtime" to stderr, so it reports Java as present.
- The MCP health check uses the exit status instead, so the two report differently.
- Neither checks the Java major version.

**Different environments.** When `java.home` is unset, GUI and headless MCP can inherit different environments and run different JDKs. That means separate Gradle daemons (double memory) and different results.

**Evidence:**
- [Java detection](../src-tauri/src/services/settings_manager.rs#L365)
- [GUI health probe](../src-tauri/src/commands/health.rs#L49)
- [MCP health probe](../src-tauri/src/services/health_inspector.rs#L48)

**Minimal first fix**

- Detection order:
  1. `org.gradle.java.home`;
  2. the configured setting;
  3. Android Studio's JBR;
  4. the highest installed JDK at version 17 or newer.
- Parse the major version, and use one shared probe for GUI and MCP.
- Surface the chosen JDK in Health and in `get_project_info`.

**Acceptance criteria:**
- With only the `/usr/bin/java` stub present, Health reports Java as missing.
- With JDK 11 and the JBR installed, the JBR is chosen.
- GUI and MCP report identical results.

### R18. Cover Modern Toolchain Diagnostics in the Build Parser

**Priority:** P1, pending verification. **Verification:** *Needs verification against real builds.*

`KOTLIN_DIAG_PATTERN` requires `path:L:C: message`, with a colon after the column, and every fixture uses K1 message wording ("Unresolved reference: foo"). The Kotlin 2.x (K2) compiler is believed to print `e: file:///…/File.kt:10:5 Unresolved reference 'foo'.`, with no colon after the column. If so, `get_build_errors`, a headline agent feature, misses most Kotlin errors on current toolchains.

These formats also likely do not match:
- KSP: `e: [ksp] path:L: msg`;
- R8: `ERROR: R8: …`;
- Android lint: `File.kt:10: Error: … [Id]`.

**Evidence:** [Kotlin pattern](../src-tauri/src/services/build_parser.rs#L14).

**Implementation**

- Capture real fixtures from Kotlin 2.x (K2), KSP, R8, lint, AAPT2, and configuration-cache failures in Phase 0.
- Relax or extend the patterns, with one fixture test per toolchain format.
- Keep unmatched error lines visible, for example as "unparsed error lines" with a count, rather than reporting "no errors".

**Acceptance criteria:**
- Each captured fixture yields the correct file, line, column, and severity.
- A failing build never reports zero errors when the log contains `e:` or `ERROR:` lines.

### R19. UI Automator Process Hygiene and Serialization

**Priority:** P2. **Verification:** Confirmed from code.

**Timed-out processes survive.** `ui_hierarchy` and `ui_automation` wrap `Command::output()` in `timeout()` without `kill_on_drop(true)`. A hung `uiautomator dump` or `screencap` outlives the timeout and keeps the device's UiAutomation registration.

**Calls are not serialized.** There is no per-device lock. Parallel agent calls, GUI plus MCP, or an instrumentation run (`run_tests`) fail with "UiAutomationService already registered".

**Retries have no overall deadline.** One hierarchy fetch can take about 8 attempts × 25 s, roughly 200 s.

**Implementation:**
- set `kill_on_drop(true)` everywhere (through R9a's helper);
- add a per-serial async lock for UI Automator operations;
- enforce a total deadline per tool call;
- return a clear "device busy (instrumentation running)" error.

**Acceptance criteria:**
- A simulated hung dump leaves no orphan process.
- Concurrent hierarchy requests serialize and succeed.
- No tool call exceeds its total deadline.

### R20. Scope Device-Affecting MCP Tools and Report Truthful Results

**Priority:** P1. **Verification:** Confirmed from code.

- **Any package accepted.** `restart_app` (with data clearing), `revoke_runtime_permission`, and `stop_app` work on any package, including `com.google.android.gms` on a personal phone.
- **Wireless ADB can be cut.** `set_network_state` with Wi-Fi off or airplane mode on a wireless-ADB device (`ip:port` or `_adb-tls-connect` serials) drops the ADB connection it runs over, and nothing can restore it.
- **Airplane-mode fallback always runs.** The `settings put` step runs even when `cmd` succeeded, without a broadcast, which can leave the setting and the radios inconsistent.
- **False success reports.** `am start` usually exits 0 even with "Error: Activity not started, unable to resolve Intent". `adb_manager::launch_app` checks for this, but `open_deep_link` and `open_app_settings` report `opened: true` regardless.
- **Prefix match on packages.** `discover_effective_package` uses `starts_with`, so `com.example.app` matches `com.example.apple`.

**Implementation:**
- Restrict destructive and permission tools to the project's `applicationId`, plus its variant suffixes, unless an explicit `allow_foreign_package: true` is passed.
- Refuse Wi-Fi-off and airplane-mode changes on wireless serials, run the airplane fallback only when `cmd` fails, and return the previous state so the change can be reverted.
- Apply the `am start` stdout error check to every `am start` path.
- Match packages exactly, or with `.`/`:` boundaries.

**Acceptance criteria:**
- Destructive tools reject foreign packages by default.
- Network changes are refused on wireless serials.
- A deep link to an unresolvable intent returns an error.

### R21. Agent Ergonomics: Screenshots, Progress, Cancellation, Tool Surface

**Priority:** P2. **Verification:** Confirmed from code.

- **Screenshot coordinates.** `screenshot` returns a full-resolution PNG with no dimensions. MCP clients commonly downscale large images, so coordinates the agent reads from the image are off by a factor of 2–3 when passed to `ui_tap`. Full-resolution images are also costly in tokens.
- **No progress or cancellation.** `run_gradle_task` and `run_tests` block for up to 600 s without progress notifications and without honoring MCP cancellation. Clients with shorter request timeouts give up while Gradle keeps running.
- **Tool surface cost.** About 60 tool descriptions take a large amount of agent context on every session.

**Implementation:**
- Return `{deviceWidth, deviceHeight, imageWidth, imageHeight, scale}` with screenshots, offer an optional downscaled image, and steer agents to element or tree-path taps.
- Emit MCP progress notifications from build and test runs, and map MCP cancellation to build cancellation. This depends on R4's run ownership.
- Consider optional toolsets (core / UI automation / device admin) enabled by setting or server argument.

**Acceptance criteria:**
- A tap at coordinates read from a downscaled screenshot hits the intended element.
- A cancelled MCP request cancels its build.
- Long builds report progress.

### R22. MCP Setup, Distribution, and Updates

**Priority:** P2. **Verification:** Confirmed from code.

- **Temporary install paths.** The setup command registers `current_exe()`. When the app runs from a mounted DMG (`/Volumes/…`) or an AppTranslocation path, the registered path disappears after eject or reboot.
- **Wrong registration scope.** The generated `claude mcp add` has no `--scope user`, so it registers only for the directory where the user pastes it. The GUI's `claude mcp get` check, run from its own working directory, then misses local-scoped entries and wrongly reports "not configured".
- **No in-place updates.**
  - `update.service.ts` only checks the GitHub API and opens the releases page; there is no `tauri-plugin-updater` and no signature-verified update.
  - Users stuck on a faulty release have no in-app path to a fix.
  - MCP clients keep running the old binary until restarted.

**Evidence:** [setup path](../src-tauri/src/commands/mcp.rs#L52), [update check](../src/services/update.service.ts).

**Implementation:**
- Refuse or warn when running from a translocated or `/Volumes` path, and ask the user to move the app to `/Applications` first.
- Generate the setup command with `--scope user`, and detect registration with the matching scope.
- Adopt `tauri-plugin-updater` with a signed `latest.json`, paired with R11's checksums and provenance.
- Show the running MCP binary's version in the GUI, and warn when it differs from the app version.

**Acceptance criteria:**
- After ejecting the DMG, the registered MCP command still works, or setup refused to register it.
- An update installs with signature verification.
- A version mismatch between GUI and MCP is visible.

### R23. Small Correctness Cleanups

**Priority:** P3 (batch with nearby work).

- `clear_history` resets `next_id = 1`, so new log filenames can collide with retained or orphaned `build-N.jsonl` files. Keep IDs monotonic.
- `mcp_activity` rotation rewrites the file non-atomically while other processes append. Use rename-based rotation.
- `USER_MANUAL.md` does not document the UI automation tools. `MCP_SERVER.md` overstates the no-shell guarantee (R14).
- There is no `adb pair`/`adb connect` support, a prerequisite for the wireless workflows in F4.

---

## Market Comparison and Feature Direction

The competitive picture as of September 2026 confirms revision 1's thesis that **MCP availability alone is no longer a differentiator**, and sharpens it:

- Google's **Android CLI 1.0** (May 2026) now provides the default *stateless* agent toolchain for devices, emulators, screenshots, layout, SDK management, and skills. It has **no logcat or crash commands**.
- **LogcatOn v1.4.0** (September 9, 2026) is free and runs on macOS, Windows, and Linux. It already ships Session Capsules (close to F1), R8 Retrace, ANR detection, cold-start comparison across builds, and an MCP "Agent Port". **It has no build runner.**

**Keynobi's defensible position is therefore stateful build-to-runtime provenance.** That means linking Gradle task → artifact → install → device → logs → crash in one place that both the developer and their agent can read. No surveyed tool connects all of these. Evidence bundles alone are no longer novel; provenance is.

This is a product hypothesis derived from offerings researched on the review date, not validated market demand. Recheck external capabilities before implementation.

| Application/tool | Relevant capability at review time | Implication for Keynobi |
| --- | --- | --- |
| Android Studio + Gemini Agent Mode | Agent device interaction, screenshots, Logcat/build assistance, App Quality Insights "Fix with AI", device UI shortcuts (API 33+), logcat tabs/splits. Acts as an MCP *client* only. [Gemini](https://developer.android.com/studio/gemini/features), [Agent Mode](https://developer.android.com/studio/gemini/agent-mode), [Logcat](https://developer.android.com/studio/debug/logcat) | Win on focused workflow, portability between agents, and provenance. |
| **Android CLI 1.0** (Google) | `run`, `emulator`, `screen capture/resolve`, `layout`, `sdk`, `skills`, `docs`, `studio render-compose-preview`. No logcat or crash commands. [Docs](https://developer.android.com/tools/agents/android-cli) | **Complement, don't duplicate.** Position the MCP as the stateful layer; publish a Keynobi skill; point agents to Android CLI for stateless tasks. |
| Journeys (Studio Labs) | Natural-language tests with per-step screenshot, action, and reasoning. [Docs](https://developer.android.com/studio/gemini/journeys) | Step-level evidence is becoming expected. |
| **LogcatOn** | Session Capsules, Retrace, 32 crash/ANR/native signals, cold-start comparison, Agent Port MCP, device control. Free, cross-platform. [Releases](https://qwerfunch.github.io/logcat-on-releases/) | **Closest direct rival.** Differentiate on build integration and provenance, not logcat features alone. |
| Maestro / Maestro Studio | Visual test authoring, repeatable YAML flows, MCP. [Maestro](https://maestro.dev/), [MCP](https://docs.maestro.dev/get-started/maestro-mcp) | Export to Maestro rather than building an automation language. |
| Mobile Next mobile-mcp | Device/app control, screenshots, UI interaction, screen recording, logs, crashes; iOS too. [Repository](https://github.com/mobile-next/mobile-mcp) | Screen recording is now a baseline agent capability. |
| Flocon | Kotlin Multiplatform desktop app plus in-app SDK: network inspection and mocking, Room/SQLite, SharedPreferences/DataStore, deep links, files. No MCP. [Repository](https://github.com/openflocon/Flocon) | Where Flipper users went. In-app inspection needs an SDK; Keynobi can cover the adb-reachable subset. |
| Proxyman / HTTP Toolkit | One-click Android proxy and certificate setup; Proxyman paid tiers include MCP. [Proxyman](https://proxyman.com/pricing), [HTTP Toolkit](https://httptoolkit.com/android/) | Integrate; do not build a proxy. |
| scrcpy | Android mirroring, control, and recording. [Repository](https://github.com/Genymobile/scrcpy) | Integrate for live interaction; start with `adb screenrecord`. |
| ADB Idea | Fast start/stop/restart and explicitly separated data-clearing commands. [Repository](https://github.com/pbreault/adb-idea) | A focused, keyboard-accessible device toolbox is useful, with clear-data kept separate. |
| Develocity Build Scan | Free per-build scans; trend dashboards are enterprise-only; 2026.1 adds MCP. [Scans](https://develocity.ai/scans/gradle/), [2026.1](https://gradle.com/develocity/releases/2026.1/) | Local build-time trends are unserved for free. |
| Sentry Size Analysis / diffuse | Paid size-regression alerts (February 2026); open-source APK/AAB diff. [Sentry](https://sentry.io/about/press-releases/sentry-introduces-size-analysis/), [diffuse](https://github.com/JakeWharton/diffuse) | Build-to-build artifact diffs are valued. |
| Perfetto | Android system-trace capture and analysis. [Documentation](https://perfetto.dev/docs/getting-started/system-tracing) | Capture and hand off traces; do not build a viewer. |

Prioritize the following features **after the relevant reliability gates pass**. Feature ranking represents product priority, not defect severity.

### F1. Build Provenance and Reproducible Debug Sessions

**Priority:** Highest product priority. **Reframed in revision 2** from "evidence bundles" to provenance, because capsule/bundle export already exists in LogcatOn.

Keynobi already has most of the ingredients: builds, errors, artifacts, devices, logs, crashes, hierarchy snapshots, and MCP activity. Connect them into one bounded session keyed by build identity.

**Implementation**

- Add a versioned `DebugSession` manifest referencing project/build/artifact/device identities. This depends on R2 and R4.
- Record a timeline of build, install, launch, crash, reconnect, and user bookmarks.
- Attach crashes, ANRs, and launch timings to the build that produced the running artifact (F7, F8).
- Capture a chosen log window, selected diagnostics, optional screenshot/hierarchy/recording, and toolchain health.
- Offer a preview and redaction step before export.
- Support local import and offline inspection.
- Expose compact session summaries and bounded evidence retrieval through MCP, for example "what changed between the last passing and the first crashing build?".
- Define retention by size and age; avoid duplicating every live buffer.

**Acceptance criteria:**
- A developer can reproduce the context of a failed run without screenshots scattered across tools.
- Every crash in a session names the build and artifact it came from.
- Imported bundles work offline, cannot escape the import directory, enforce size limits, and clearly identify omitted or redacted evidence.

**Success measure:** Time required to assemble and understand a useful bug report.

### F2. Named Run Configurations and Proper Application-Module Support

This extends the artifact correctness work (R2 removes the hard-coded `app` module) into a useful workflow feature.

**Implementation**

- Store named configurations containing application module, variant, task, target preference, launch activity/deep link, and optional post-launch log filter.
- Separate portable project configuration from machine-specific paths and device serials.
- Validate a configuration before execution and show the resolved target.
- Make configurations available in the command palette and MCP.
- Preserve Build and Run as explicit actions.
- Treat unsupported dynamic Gradle configuration as a visible limitation.

**Acceptance criteria:**
- A multi-application repository can run the intended module repeatedly.
- Switching configuration cannot reuse another configuration's artifact.
- Exported configuration contains no machine secrets.

**Success measure:** Fewer manual steps and fewer target-selection mistakes in repeated development loops.

### F3. Test Center with Structured Results and Maestro Export

**Re-scoped in revision 2:** the most distinctive piece is turning successful agent or manual UI sessions into deterministic Maestro flows (F12), because LLM-driven runs are expensive and non-deterministic.

**Implementation**

- Add typed `TestRun` and `TestCaseResult` models linked to operation/build/device identities.
- Execute selected Gradle unit/instrumentation test tasks through the supervised runner (R9), serialized with UI Automator use (R19).
- Parse JUnit XML reports into passed, failed, skipped, and incomplete results.
- Show failure output, duration, related logs, and rerun actions.
- Add an optional Maestro adapter for existing project flows.
- Expose the same results through GUI and MCP.

**Acceptance criteria:**
- A failing test links to its own report and evidence.
- Cancellation produces an incomplete/cancelled run.
- Unsupported report formats retain raw artifacts without inventing results.
- A missing optional tool produces a setup hint.

**Success measure:** Time from failed test to useful diagnosis and verified rerun.

### F4. Device Toolbox That Exposes Existing Backend Capabilities

**Prerequisites:** R14 (argument quoting), R20 (package scoping, truthful results), and R1 (destructive separation). The toolbox puts these same commands in front of users.

**Implementation**

- Add a target-labelled toolbox for:
  - restart, force-stop, deep links, and app settings;
  - permissions, orientation, and screenshots;
  - relevant runtime information;
  - configuration toggles (F10).
- Reuse the same domain services as MCP.
- Separate clearing data and other destructive operations visually and semantically.
- Register actions in the command palette.
- Add wireless pairing/connection (`adb pair`/`connect`) as a separately tested extension with explicit connection state and timeout behavior.

**Acceptance criteria:**
- Every action names its device and package.
- UI and MCP return consistent outcomes.
- Changing selection during an action cannot retarget it.

**Success measure:** Reduced terminal switching for routine device tasks.

### F5. Multiple Log Sessions, Comparison, and Offline Investigation

**Priority lowered in revision 2:** Android Studio already provides log tabs and splits. Pursue this mainly for comparing *sessions* (F1), such as before and after a build, rather than as a general logcat feature.

**Implementation**

- Build on the corrected session/cursor model (R7).
- Allow a small bounded number of sessions, each pinned to a device or imported capture.
- Keep filter, selection, follow-tail, and bookmarks per session.
- Add side-by-side views and linked navigation using explicit timestamps.
- Define an overall memory budget across sessions.
- Display clock differences and discontinuities rather than implying exact synchronization.

**Acceptance criteria:**
- Two devices cannot mix entries or filters.
- Closing a session releases its listeners and processes.
- Memory remains bounded at the session cap.

### F6. Optional Screen Interaction and Performance Capture

**Priority:** Later integrations. **Revision 2:** start with `adb screenrecord` (F11), which needs no external dependency, before scrcpy.

**Implementation**

- Launch an installed scrcpy instance under supervision, for the exact selected serial.
- Track its process and recording output in the debug session.
- Keep missing-tool setup and version compatibility explicit.
- Add bounded performance-capture presets that produce a trace artifact, and open traces in Perfetto.
- Distinguish Keynobi process health from the Android application's performance.

**Acceptance criteria:**
- The selected device is used consistently.
- Capture stops and cleans up correctly.
- Missing tools fail visibly.
- Artifacts are attached to the correct session.

### New Feature Candidates (Revision 2)

These are ranked. Effort: S = days, M = 1–3 weeks, L = more. Each item builds on existing Keynobi capabilities.

| ID | Feature | Effort | Why users want it | Builds on |
| --- | --- | --- | --- | --- |
| **F7** | **Exit reasons, ANR and native crash collection, and automatic R8 retrace.** Read `dumpsys activity exit-info <pkg>`; deobfuscate with the `mapping.txt` from the exact build that produced the installed APK; MCP `get_exit_reasons`. | S/M | Play vitals penalize user-perceived ANR rates above 0.47% and crash rates above 1.09%. LogcatOn ships retrace. | Crash inspector, build history, artifact identity (R2) |
| **F8** | **Launch time per build.** Parse `am start -W` TotalTime and the logcat `Displayed` line; store it in build history; show the change from the previous build. | S | Play vitals slow-start metrics; LogcatOn's cold-start comparison. | `restart_app` timing, build history |
| **F9** | **APK/AAB inspection and build-to-build diff.** Size, dex count, manifest permissions, exported components, SDK levels. MCP: "did my change add a permission?" | M | Sentry launched paid Size Analysis (February 2026); diffuse is widely used. | aapt2, retained artifacts |
| **F10** | **Device configuration toggles and a configuration-matrix screenshot.** Dark mode, font scale, density, locale, animations, layout bounds; one action captures the same screen under N configurations. | S/M | Studio only offers device UI shortcuts inside the IDE on API 33+; Radon charges for advanced device settings. | MCP screenshot and hierarchy tools |
| **F11** | **Screen recording to MP4/GIF, aligned with the log window.** `adb screenrecord`, attached to the debug session. | S/M | mobile-mcp exposes recording; Radon and Revyl charge for replays. | F1 timeline |
| **F12** | **Export a successful UI session as a Maestro flow.** Keynobi's `ui_tap_element`/`ui_fill_input` calls are selector-based and map cleanly to YAML. | M | Maestro's recorder only bootstraps flows; Journeys and Arbigent persist scenarios. | MCP activity log, UI automation |
| **F13** | **Local Gradle build-performance trends.** Per-task timings from `--profile`, configuration-cache hit/miss, and a "why was this build slower?" MCP tool. | M | Develocity's trend dashboards are enterprise-only. | Build history (`duration_ms`) |
| **F14** | **Deep link / App Links verifier and intent composer.** Intent filters from the merged manifest, `pm get-app-links` state, saved presets with extras. | S/M | Flocon's deep-link launcher; Studio's App Links Assistant. | `open_deep_link` (after R14) |
| **F15** | **Read-only app data browser with snapshot/restore.** `run-as` file tree, SharedPreferences XML, SQLite pull; restore a known state for reproducible runs; agents can verify state after a flow. | M | Flocon, Studio's Database Inspector, Studio backup/restore. | Device services |
| **F16** | **One-click proxy integration.** Set and unset `http_proxy` with automatic revert; detect Proxyman or HTTP Toolkit. | S | Studio's Network Inspector supports only OkHttp/HttpURLConnection, and its rules do not persist. | Device services |
| **F17** | **Android CLI interop and a Keynobi agent skill.** Detect Android CLI; publish a skill describing when to use Keynobi (stateful: logs, crashes, builds) versus Android CLI (stateless). | S | Android CLI is positioned as the default agent toolchain. | MCP server |

A later candidate, **matching local crashes to Play vitals or Crashlytics issues** (Play Developer Reporting API), is deprioritized because Studio's App Quality Insights and the Firebase MCP already cover production crash triage.

## Phased Implementation Order

Severity determines importance; dependencies determine execution order. With a single maintainer, **each phase should be timeboxed**, and **feature work should be frozen** until Phase 1's gate passes, except for S-sized items that directly reuse a fix.

| Phase | Deliverables | Completion gate |
| --- | --- | --- |
| **P0 — Stop active harm** (days) | R5 test isolation, with a CI guard against writes to `$HOME/.keynobi`; R1 non-destructive restart; R14 `adb_shell_argv` quoting; R15 Gradle flag/task restriction and tool annotations; R3a webview permission trimming. | Tests cannot touch user data; no shell metacharacter reaches the device unquoted; restart preserves data; agents cannot pass Gradle flags or publish by default. |
| **0 — Trustworthy baseline** | Run the 7 outstanding Hardening Plan manual device checks. Record the support matrix. Capture parser fixtures (R18). Build the headless-binary harness with fake adb/gradle. Write deterministic failing tests for the P1 findings. Measure a logcat soak baseline. | The important defects are demonstrated before they are fixed; baselines exist. |
| **0.5 — Quick wins** (days) | Dialog Escape/focus handling; remove the unused auto-install setting; install `@vitest/coverage-v8`; remove `\|\| true` from CI; idempotent tags; release `codesign`/`spctl`/`stapler` verification and checksums; `cargo audit`/`npm audit`; `(serial, state)` polling; selection revision counter; line-byte and `errors_buf` caps; flush logging on shutdown; active-log rotation exclusion; history-load error state. | Each item ships with its own test. |
| **1 — Correct targets and ownership** | R2 APK resolution; R4 run-ID ownership; R7 three minimal fixes plus shared logcat request functions; R9a EOF hang and adb timeouts; R5b cross-process persistence; R16 project trust; R17 JDK/health; R18 parser; R20 tool scoping. | No wrong-target action, stale-finalizer corruption, stuck build slot, or stranded operation in the regression matrix. |
| **2 — Unify runtime state** | R6 socket bridge; R8 telemetry allowlist and immediate opt-out; R9 remainder; R19 UI Automator serialization; R21 agent ergonomics. | GUI and MCP agree on operations; reconnect, cancel, disconnect, and shutdown scenarios recover deterministically. |
| **3 — Release confidence** | R10 budgets and soak tests; R11 contracts and native smoke tests; R12 accessibility and truthful UI; R22 updater and setup fixes. | A release candidate passes native/device smoke, bounded-load checks, privacy tests, and retry-safe publishing. |
| **4 — Strongest product value** | F1 build provenance with F7 (exit reasons and retrace) and F8 (launch time); F2 named run configurations; F17 Android CLI interop. | Users can reproduce and share a failure with exact build-to-crash context. |
| **5 — Extend validated workflows** | F9, F10, F12 and F4 by demand; then F3, F11, F13–F16, F5, F6. | Each feature demonstrates a measurable workflow improvement and preserves the reliability gates. |

R13 is supporting work within the relevant phases, not a separate rewrite phase.

Phases 1–3 are a substantial engineering effort for one maintainer, likely several months. Estimate them after Phase 0 establishes reproductions and resolves the R6 design and native-testing choices.

Each implementation slice should be independently reviewable:

1. A failing behavior test or documented reproduction, **revert-verified**: the test must fail without the fix. The Hardening Plan found a test that passed with and without its fix.
2. The smallest ownership/service change needed.
3. The fix and adverse-ordering tests.
4. Binding/contract updates where required (Rust handler and `tauri-api.ts` together).
5. Relevant documentation, focused native verification, and a Status update in the index above.

## Reliability Metrics

Track these from Phase 0 onward, so "materially more reliable" is measurable rather than asserted:

| Metric | Source | Target (set after baseline) |
| --- | --- | --- |
| Crash-free session rate | Opt-in telemetry (after R8) | Establish in Phase 0; must not regress per release |
| Reported vs. actual build outcome mismatches | Regression matrix (R4) | Zero |
| Logcat drop rate at 1,000 lines/s | Soak test (R10) | Baseline, then a threshold |
| RSS after a 30-minute soak | Soak test (R10) | Baseline, then a threshold |
| Stuck-operation count (slot never released) | Regression matrix (R4, R9a) | Zero |
| Time to ship a fix for a faulty release | Release records (R11, R22) | Documented and rehearsed |

## Release Approval Criteria

Before calling the application materially more reliable, require evidence that:

- Tests cannot write production application data, and CI enforces it.
- No device-shell argument is interpreted by the device shell; injection tests pass for every shell-reaching tool.
- Destructive agent actions are opt-in, annotated, and scoped to the project's package.
- No operation can silently change project, artifact, package, or device after starting.
- Every started operation reaches one truthful terminal outcome; cancellations are recorded as cancellations.
- Cancellation failure remains visible and recoverable.
- GUI and MCP state ownership is explicit and consistent; standalone mode is labelled.
- Persistence survives concurrent writers and interrupted writes.
- Log reconnects, stop/start, device switches, and clears preserve identity and reject stale data.
- Sustained ingestion has bounded memory and disk behavior, measured against the Phase 0 baseline.
- Telemetry obeys a tested data-minimization and consent contract, with immediate opt-out.
- Keyboard workflows and native shutdown/reopen work in the packaged application.
- Published artifacts are verified, checksummed, and updatable in place; publication can recover from interruption.
- The Hardening Plan's 7 manual device checks, and this roadmap's device checks, have been run and recorded.

## Decisions (Recorded 2026-09-24)

The maintainer approved the following decisions on 2026-09-24. They are binding for implementation. Changing one requires updating this section, with the date and reason.

### Blocking P0

**D1 — Gradle task policy for agents (R15)**
- Always reject task names starting with `-`; agents never pass Gradle flags.
- Deny by default: `publish*`, `upload*`, `uninstall*`, `closeAndRelease*`, `*ToMavenCentral`, `*PlayStore*`.
- Allow all other tasks, including custom ones.
- Escape hatch: the `mcp.allowUnrestrictedGradle` setting, off by default and **toggleable only in the GUI, never through MCP**.

**D2 — `restart_app` breaking change (R1)**
- Remove `cold` and add `clear_data: bool`, default `false`.
- A request that still sends `cold` gets an explicit error pointing to `clear_data`. Do not silently map or ignore it.
- Document the change in the changelog.

**D3 — Package scoping for destructive tools (R20)**
- Destructive and permission-changing tools (`restart_app` with `clear_data`, `revoke_runtime_permission`, `stop_app`) are scoped by default to the project's `applicationId` plus its variant suffixes.
- Other packages require an explicit per-call `allow_foreign_package: true`.
- Read-only tools stay unrestricted.

**D4 — Affected-user disclosure (R5)**
- Add one factual line to the fix's release notes: "Running the developer test suite could overwrite local build history; fixed."

### Blocking Phase 1

**D5 — Project trust (R16)**
- Prompt once, on first open: "This project will run its Gradle build scripts. Trust it?"
- Choices: **Trust** or **Open in Safe Mode**. Safe Mode uses static variant parsing, and builds are disabled until the project is trusted.
- Persist trust per canonical root in settings.
- Grandfather every project already in the registry as trusted at migration.
- Offer "Revoke trust" in the project context menu.
- Headless MCP never prompts: an untrusted project returns an error asking the user to open it in the GUI first.

**D6 — Artifact metadata (R2)**
- The primary source is AGP `output-metadata.json`.
- The fallback is concatenated path-segment matching below `apk/`.
- There is never a cross-variant "any APK" fallback.
- The declared floor is AGP 7.0 or newer.
- No Gradle init scripts or tooling-API integration; they conflict with D5's trust model.

**D7 — External build directories (R3)**
- Not supported for now. Outputs outside the canonical project root are rejected with an error naming the path.
- Add an approved-external-roots setting only when a real user reports the need.

### Blocking Phase 2

**D8 — GUI/MCP state sharing (R6):** the socket-bridge design is approved.

| Policy | Decision |
| --- | --- |
| Transport | The GUI listens on `~/.keynobi/mcp.sock` (socket 0600, directory 0700). `keynobi --mcp` forwards stdio to it when the GUI is running. Existing client registrations are unchanged. |
| GUI not running | Run standalone automatically, **clearly labelled** in `serverInfo`, `get_project_info`, and every build result. `--attach-only` gives a hard failure. Never auto-launch the GUI from an agent session. |
| Cancellation authority | Any client or the GUI may cancel any build. History records who cancelled. |
| Client disconnect | A build started by a disconnected client keeps running; the GUI shows its result. |
| GUI quits with clients attached | Cancel the running build and return a clean error to clients. They fall back to standalone on their next call. |
| Legacy | Delete the stdio GUI mode and the `mcp.auto_start` setting. Replace the PID file with per-session liveness records. |

**D9 — Telemetry scope (R8)**
- Keep telemetry opt-in, sending only allowlisted fields: error type, release, OS, architecture, in-app stack frames, and known error code.
- Drop messages, exception values, breadcrumbs, and tags.
- Native panics report their location only, never the message.
- Opt-out takes effect immediately.

### Blocking Phase 3

**D10 — Auto-updater (R22)**
- Adopt `tauri-plugin-updater` with a signed `latest.json` published by the release job.
- Keep the private key in GitHub Actions secrets, plus an **offline backup** (password manager and an encrypted copy). The backup is mandatory.
- Check at startup, notify, and install only on user confirmation; never silently.
- Warn when the MCP binary's version differs from the GUI's.

**D11 — Support matrix**

| Area | Decision |
| --- | --- |
| macOS | 13 Ventura and newer, official. 12 Monterey, best effort (the configured minimum stays `12.0`). |
| Architecture | Keep shipping `aarch64` and `x86_64`. The native smoke test runs on Apple Silicon only. Revisit Intel when Apple drops it. |
| JDK | 17 and newer; Android Studio's bundled JBR is preferred (R17). |
| AGP / Gradle | AGP 8.x and newer, official. AGP 7.x, best effort. |
| Android | API 26 and newer. One current Google emulator image in automated checks, plus the maintainer's physical device for manual checks. |

**D12 — Native GUI automation (R11)**
- Do not invest now. Build the headless-binary harness with fake `adb`/`gradlew` first.
- Cover the packaged GUI with a scripted manual checklist (about 10 minutes per release candidate), with results recorded in the release PR.
- Revisit only if macOS WebDriver support is confirmed working.

**D13 — Performance budgets (R10)**
- Set no thresholds before Phase 0 measures a baseline: 30 minutes at 1,000 lines/second on the maintainer's machine.
- After that, the thresholds are baseline + 20% RSS and a 0% logcat drop rate at 1,000 lines/second.
- Run nightly or before each release, not on every PR.

### Process and Product

**D14 — Feature freeze and timeboxes**
- No new features until Phase 1's gate passes. The only exception is S-sized items that directly reuse a fix (for example F8, launch time per build, from the restart work).

  | Phase | Timebox |
  | --- | --- |
  | P0 + Phase 0.5 | 2 weeks |
  | Phase 0 | 1 week |
  | Phase 1 | 4–6 weeks |

- A phase that overruns its timebox by 50% stops and is re-scoped.
- Keep shipping patch releases throughout.

**D15 — Feature priority and Android CLI**
- The first feature phase is F1 (build provenance), F7 (exit reasons and retrace), F8 (launch time), and F2 (named run configurations).
- Publish a Keynobi agent skill that works *alongside* Google's Android CLI (F17).
- Validate demand before Phase 5 through a README/release-notes poll, GitHub issues and discussions, and (with opt-in telemetry) counts of which MCP tools are actually called.

### Still Open

- None blocking. Revisit D7, D11 (Intel), and D12 when their stated triggers occur.

**Recommended initial approval scope:** P0, Phase 0, and Phase 0.5, followed by Phase 1. Treat Phases 2–3 as the next approval, and build provenance (F1 + F7 + F8) with named run configurations (F2) as the first feature phase.

## Resuming This Plan Later

Before implementing an approved phase:

1. Read current project instructions, [`HARDENING_PLAN.md`](HARDENING_PLAN.md), and the governing architecture/domain documents.
2. **Do not run `cargo test` until R5's isolation has landed.** Afterwards, confirm that the CI guard against writes to `$HOME/.keynobi` is active.
3. Compare the current checkout with the reviewed baseline; some findings may already be fixed. Update the Status index.
4. Reproduce the selected findings without touching real user data or devices unintentionally.
5. Confirm the phase's unresolved product decisions, and refresh external integration documentation (Android CLI, LogcatOn, and Tauri testing change quickly).
6. Implement in reviewable, revert-verified slices, recording validation evidence and updating this roadmap as work is completed.

Do not treat historical test totals or this saved proposal as proof of current behavior or as approval for later phases.
