# Hardening Plan — Solidity, Testability, Reliability

**Status:** COMPLETE (2026-08-18) — all phases landed, see commits 884a8be..HEAD
**Created:** 2026-08-18
**Scope:** all 27 findings + 7 test-coverage gaps from the repo review, plus 2 items found while
writing this plan (6.3a module-side-effect listener in `monitor.store.ts`, 6.3b dead
`buildLogOutput` export). No new features.
**Baseline commit:** `0386fba` (rebased onto 14 dependency bumps on 2026-08-18)

> **Completed.** All 27 findings, the 7 coverage gaps, and the 2 items found while
> writing the plan are addressed across 9 commits. Two additional bugs were found
> _during_ execution and fixed: MCP `clear_logcat` neither bumped `clear_epoch`
> nor emitted `logcat:cleared`, and three pre-existing clippy lints were blocking
> `--all-targets`. Remaining follow-ups are listed at the end of this document.

---

## How to use this document

Each work item is self-contained: **files → change → tests → verify**. Items are grouped into
phases; each phase ends at a green, committable state. Do not batch phases into one commit —
every file touched here is a churn hotspot (84th–99th percentile, bus factor 1), so small diffs
with their own tests are the only way to keep review risk down.

**Golden rule for this plan: write the failing test first.** Three of the top findings (C1, H5,
M9) are currently invisible to a fully green 1121-test suite. A fix without a test that failed
before it is not done.

### Per-phase verification (run all of these before committing)

```bash
npm run test && npm run lint && npx tsc --noEmit
cd src-tauri && cargo test --lib --tests && cargo clippy --lib --tests -- -D warnings && cargo fmt --check
```

Note: `cargo clippy --lib --tests` currently fails on two **pre-existing** lints in
`src-tauri/src/services/ui_automation.rs:1924-1925`. Phase 6.1 fixes those; until then, use
`cargo clippy -- -D warnings` (what CI runs) and treat those two as known.

### Risk profile of the files being touched

| File                            | Hotspot | Risk type   | Note                  |
| ------------------------------- | ------- | ----------- | --------------------- |
| `services/mcp_server.rs`        | 99%     | churn-heavy | 3330 lines, 8 tests   |
| `services/build_runner.rs`      | 98%     | bug-prone   | 26 co-change partners |
| `services/settings_manager.rs`  | 96%     | churn-heavy | 67 downstream callers |
| `commands/build.rs`             | 96%     | bug-prone   |                       |
| `services/logcat.rs`            | 95%     | churn-heavy |                       |
| `src/services/build.service.ts` | 95%     | bug-prone   |                       |
| `src/stores/device.store.ts`    | 86%     | bug-prone   |                       |
| `services/process_manager.rs`   | 84%     | bug-prone   |                       |

`lib.rs`, `App.tsx`, and `tauri-api.ts` are the three highest-churn files in the repo — touch
them only for the exact lines required, never reformat.

### Phase order and why

1. **Phase 0** — characterization tests. Locks current correct behavior before refactors.
2. **Phase 1** — logcat lifecycle. The only CRITICAL; self-contained; unblocks nothing else.
3. **Phase 2** — build path unification. Largest refactor; subsumes 5 findings.
4. **Phase 3** — process manager. Small, independent, can be done any time after Phase 0.
5. **Phase 4** — frontend guards. Independent of all Rust phases.
6. **Phase 5** — shared validation + settings cache. Depends on Phase 2 (touches the same files).
7. **Phase 6** — CI guards, coverage, cleanup.

Phases 3 and 4 are independent of 1 and 2 — parallelize if you want.

---

## Phase 0 — Characterization tests (no production changes)

**Goal:** capture today's correct behavior so the refactors in Phases 1–2 can't silently change
it, and add the two failing tests that prove C1 and H5 are real.

### 0.1 — Build finalization characterization

**File:** `src-tauri/src/commands/build.rs` (test module)

Add tests around `finalize_completed_build` covering behavior that must survive Phase 2:

- cancelled build → `event.cancelled == true`, history record still written, status `Cancelled`
- successful build → `BuildStatus::Success`, `error_count == 0`
- build with warnings only → `success == true`, `warning_count` correct, `error_count == 0`
- history ring respects `MAX_HISTORY`

**Verify:** `cargo test --lib finalize`

### 0.2 — Logcat stream lifecycle characterization

**File:** `src-tauri/src/services/logcat.rs` (test module)

The existing 6 stream tests cover spawn failure and reconnect. Add:

- `stop_then_start_does_not_leave_two_streams` — **must fail today** (this is C1)
- `stop_on_idle_device_terminates_child` — **must fail today** (this is H4)

Use a fake `adb` binary the same way `mock_gradlew_*` tests in `build_runner.rs` do (see the
existing `mock_gradlew_output_parses_correctly` helper for the shell-script fixture pattern).
For the idle case, the fake adb should print one line then `sleep`.

**Verify:** both new tests **fail**. Record the failure output in the commit message.

### 0.3 — Process cancel characterization

**File:** `src-tauri/src/services/process_manager.rs` (test module)

- `cancel_terminates_running_process` — spawn `sleep 30`, cancel, assert `on_exit` fires with
  `ProcessTermination::Cancelled` within ~1s
- `delayed_sigkill_does_not_fire_after_natural_exit` — **must fail today** (this is H5).
  Spawn a short process, cancel it, wait >5s, assert no `SIGKILL` was sent post-exit. Assert via
  an `exited` flag rather than by observing signals.

**Verify:** first passes, second fails.

### 0.4 — Frontend listener idempotency characterization

**File:** `src/services/build.service.test.ts`

- `initBuildService registers exactly one build:complete listener when called twice concurrently`
  — **must fail today** (this is M9). Make the mocked `listen` return a deferred promise so both
  calls are in flight simultaneously, then assert `listen` was called once.

**Verify:** test fails.

**Commit:** `test: add characterization tests for logcat, build, and process lifecycles`
(4 tests intentionally failing — mark with `#[ignore]` / `it.fails` if you need CI green between
phases, and remove the marker in the phase that fixes each.)

---

## Phase 1 — Logcat stream lifecycle

Fixes **C1**, **H4**, **M14**, **M15**, **M18**.

### 1.1 — Stream generation token (C1, H4, M18)

**Files:**

- `src-tauri/src/services/logcat.rs`
- `src-tauri/src/commands/logcat.rs`
- `src-tauri/src/services/mcp_server.rs` (`start_logcat` / `stop_logcat` tools)

**Change:**

1. Add to `LogcatStateInner` (`services/logcat.rs:149`):

   ```rust
   /// Incremented on every start/stop request. A running stream task exits as
   /// soon as this no longer matches the generation it was started with, so a
   /// stop→start sequence can never leave two streams feeding the same store.
   pub stream_generation: u64,
   ```

   Initialize to `0` in `LogcatStateInner::new()`.

2. Change the signature of `start_logcat_stream` to take `generation: u64`.

3. Replace **every** `if !state.streaming { break }` / `if !state.streaming { ... }` check in
   `start_logcat_stream` with a combined check:

   ```rust
   fn is_current(state: &LogcatStateInner, generation: u64) -> bool {
       state.streaming && state.stream_generation == generation
   }
   ```

   Sites: the `'reconnect` loop head (~line 344), the spawn-failure retry (~line 377), the
   pipeline tick (~line 470), and the post-drain `still_streaming` check (~line 528).

4. **Do not** set `state.streaming = false` unconditionally at the end of
   `start_logcat_stream` (currently line 559-560). Only do so if this task still owns the
   generation — otherwise a dying old task clobbers the new stream's flag:

   ```rust
   let mut state = logcat_state.lock().await;
   if state.stream_generation == generation {
       state.streaming = false;
   }
   ```

5. Add an explicit shutdown path so the child dies even on an idle device (**H4**). Give the
   reader task a `tokio::sync::watch` or `CancellationToken` receiver and `select!` it against
   `reader.next_line()`:

   ```rust
   tokio::select! {
       line = reader.next_line() => { /* existing match */ }
       _ = shutdown.changed() => break,
   }
   ```

   The pipeline task signals shutdown when `is_current()` goes false. After
   `reader_handle.await`, call `let _ = child.kill().await;` explicitly rather than relying on
   `kill_on_drop`.

6. `commands/logcat.rs::start_logcat` (line 14):

   ```rust
   let generation = {
       let mut state = logcat_state.lock().await;
       // A restart with a different serial must supersede the running stream (M18).
       if state.streaming && state.device_serial == device_serial {
           return Ok(());
       }
       state.stream_generation = state.stream_generation.wrapping_add(1);
       state.streaming = true;
       state.device_serial = device_serial.clone();
       state.stream_generation
   };
   ```

   Pass `generation` into `start_logcat_stream`.

7. `commands/logcat.rs::stop_logcat` (line 57): bump the generation as well as clearing
   `streaming`, so any in-flight task exits even if `streaming` is flipped back on before its
   next tick:

   ```rust
   state.streaming = false;
   state.stream_generation = state.stream_generation.wrapping_add(1);
   ```

8. Apply steps 6–7 verbatim to the MCP tools at `services/mcp_server.rs:784` and `:845`.
   (Phase 5.1 will deduplicate these into a shared helper; for now keep them identical and add a
   `// KEEP IN SYNC WITH commands/logcat.rs` comment on both.)

**Tests:** the two Phase 0.2 tests must now pass. Add:

- `start_with_different_serial_supersedes_running_stream`
- `stop_bumps_generation_so_stale_task_exits`

**Verify:** `cargo test --lib logcat` — all green, including the previously failing two.

**Risk:** `logcat.rs` is 95th-percentile churn. Keep the diff to the generation checks and the
shutdown select; resist tidying the surrounding pipeline code.

### 1.2 — Frontend restart uses one atomic call (C1, follow-through)

**Files:** `src/components/logcat/LogcatPanel.tsx:734-748`

`handleRestart` currently issues three sequential IPC calls. With 1.1 landed the race is closed
backend-side, but the sequence is still wasteful and reads as risky. Simplify to:

```ts
async function handleRestart() {
  if (restarting()) return;
  setRestarting(true);
  try {
    await stopLogcat();
    await clearLogcat();
    const device = selectedDevice();
    await startLogcat(device?.serial ?? undefined);
    setLogcatStreaming(true);
  } catch (e) {
    /* unchanged */
  } finally {
    setRestarting(false);
  }
}
```

No behavior change — this is a no-op edit unless you prefer to introduce a `restart_logcat`
command. **Recommendation: skip the new command** (CLAUDE.md §2 — an existing command sequence
already does the job now that the backend is correct). Leave `handleRestart` as-is and note in
the commit that the fix is backend-side.

### 1.3 — Bounded reconnect with backoff (M14)

**File:** `src-tauri/src/services/logcat.rs:370-383, 545-560`

**Change:** replace both fixed delays with exponential backoff and a give-up:

```rust
const RECONNECT_BACKOFF_MAX_MS: u64 = 30_000;
const RECONNECT_MAX_ATTEMPTS: u32 = 10;
```

Track `consecutive_failures: u32` across `'reconnect` iterations. Delay is
`min(BASE << consecutive_failures, RECONNECT_BACKOFF_MAX_MS)`. Reset to 0 after any iteration
that successfully streamed at least one line. On exceeding `RECONNECT_MAX_ATTEMPTS`, set
`streaming = false`, emit a new `logcat:stopped` event with a reason string, and break.

Keep the existing `#[cfg(test)]` short-delay constants pattern (lines 218-226) so tests stay fast.

**Frontend:** `src/components/logcat/LogcatPanel.tsx` — add a `listenLogcatStopped` handler
alongside `unlistenReconnecting` (line 635) that calls `setLogcatStreaming(false)` and shows a
toast with the reason. Add the wrapper to `src/lib/tauri-api.ts` next to `listenLogcatReconnecting`.

**Tests:**

- `reconnect_backoff_grows_and_caps`
- `gives_up_after_max_attempts_and_sets_streaming_false`

### 1.4 — Surface dropped logcat lines (M15)

**Files:** `src-tauri/src/models/logcat.rs:135` (`LogStats`), `src-tauri/src/services/logcat.rs:420`

**Change:** add `pub dropped_lines: u64` to `LogStats`. Increment it on
`Err(TrySendError::Full(_))` in the reader task — the reader has no state lock, so use an
`Arc<AtomicU64>` shared with the pipeline task, which folds it into `state.store.stats` on each
tick.

**⚠ Binding regeneration required:** `LogStats` is `#[ts(export)]`. After this change run
`npm run generate:bindings` and commit `src/bindings/LogStats.ts`. CI fails otherwise (the
bindings-staleness check in `.github/workflows/ci.yml`).

**Frontend:** `src/components/logcat/LogcatPanel.tsx` — where ring stats are already rendered,
show `⚠ N dropped` when `dropped_lines > 0`.

**Tests:** `dropped_lines_counted_when_channel_full` — push more than
`RAW_LOG_LINE_CHANNEL_CAPACITY` lines with the pipeline stalled.

**Commit:** `fix(logcat): generation-scoped stream lifecycle, bounded reconnect, drop counter`

---

## Phase 2 — Unify the two build paths

Fixes **H2**, **H3**, **H6**, **M8**, **M17**, **L20**, **L19**.

This is the highest-leverage refactor in the plan. Today `commands/build.rs::run_gradle_task`
(UI) and `build_runner::run_task` (MCP) independently reimplement line parsing, error
accumulation, success detection, cancellation, and finalization — and already disagree on five
behaviors.

### 2.1 — Extract the build-slot reservation (H2)

**File:** `src-tauri/src/services/build_runner.rs`

**Change:** lift the guard currently only in `commands/build.rs:163-171` into build_runner:

```rust
/// Reserve the single build slot. Returns Err if a build is already running.
/// Must be called by every code path that spawns Gradle.
pub async fn try_reserve_build_slot(
    build_state: &BuildState,
    task: &str,
    started_at: &str,
) -> Result<(), String> {
    let mut bs = build_state.inner.lock().await;
    if bs.starting || bs.current_build.is_some()
        || matches!(bs.status, BuildStatus::Running { .. })
    {
        return Err("A Gradle build is already running".to_string());
    }
    bs.starting = true;
    bs.status = BuildStatus::Running {
        task: task.to_owned(),
        started_at: started_at.to_owned(),
    };
    bs.current_errors.clear();
    Ok(())
}
```

Call it from **both** `commands/build.rs::run_gradle_task` (replacing the inline block) and
`build_runner::run_task` (which currently has no guard at all — this is H2).

**Tests:**

- `try_reserve_build_slot_rejects_when_running`
- `try_reserve_build_slot_rejects_when_starting`
- `mcp_run_task_refuses_while_ui_build_is_running` (integration-style, using `BuildState` directly)

### 2.2 — Share the line-accumulator (M8)

**File:** `src-tauri/src/services/build_runner.rs`

Both paths build the same three accumulators and the same `on_line` closure. Extract:

```rust
pub struct BuildAccumulators {
    pub errors: Arc<StdMutex<Vec<BuildError>>>,
    pub duration_ms: Arc<StdMutex<u64>>,
    pub success_flag: Arc<StdMutex<bool>>,
}

impl BuildAccumulators {
    pub fn new() -> Self { /* … */ }

    /// Build the shared `on_line` handler. `extra` runs after the shared
    /// bookkeeping — the UI path uses it to forward the line to its Channel.
    pub fn on_line_handler(
        &self,
        build_log: BuildLog,
        extra: impl Fn(&BuildLine) + Send + Sync + 'static,
    ) -> Box<dyn Fn(ProcessLine) + Send + Sync + 'static> { /* … */ }

    pub fn snapshot(&self) -> (bool, u64, Vec<BuildError>) { /* … */ }
}
```

`commands/build.rs` passes `extra = |line| { let _ = on_line.send(line.clone()); }`;
`build_runner::run_task` passes `extra = |_| {}`.

Delete the duplicated closure bodies from both call sites.

**Tests:** move/extend the existing `build_runner.rs` parse tests to exercise
`on_line_handler` directly (error/warning classification, summary duration, success flag).

### 2.3 — Exit code is authoritative for success (L20)

**File:** `src-tauri/src/commands/build.rs:257-259`

**Change:**

```rust
let success = !cancelled
    && matches!(termination, ProcessTermination::ExitCode(0))
    && flag;
```

Rationale: `||` lets stray `BUILD SUCCESSFUL` text in the output override a non-zero exit.
Making both conditions required is the safe direction. Apply the same rule to
`build_runner::run_task`, which today ignores the exit code entirely — thread the
`ProcessTermination` out of `on_exit` (via a `oneshot` payload instead of `()`).

**Tests:**

- `success_requires_zero_exit_and_summary_line`
- `nonzero_exit_with_success_text_is_not_success`
- `zero_exit_without_summary_line_is_not_success` (documents the stricter rule)

> **Decision to record:** this tightens the rule. If a Gradle task legitimately exits 0 without
> printing a summary line (`clean` may), it will now report failure. **Mitigation:** treat a
> missing summary line as success only when the exit code is 0 _and_ no errors were parsed.
> Encode whichever variant you pick as a decision record (see Phase 6.5).

### 2.4 — Cancel-during-spawn kills the process (H6)

**File:** `src-tauri/src/services/build_runner.rs:663-672`

**Change:** mirror `commands/build.rs:298-306`:

```rust
if matches!(bs.status, BuildStatus::Cancelled) {
    drop(bs);
    let _ = build_state.take_active_process_id();
    process_manager::cancel(&process_manager.0, pid).await;
    return Ok(GradleTaskResult { success: false, timed_out: false, duration_ms: 0, errors: vec![] });
}
```

**Tests:** `run_task_cancelled_during_spawn_kills_process` — set status to `Cancelled` before
the spawn resolves; assert the process manager has no tracked processes afterwards.

### 2.5 — Timed-out builds record history (M17)

**File:** `src-tauri/src/services/build_runner.rs:676-684`

**Change:** before returning the `timed_out` result, call `record_build_result` with a failed
`BuildResult` and one synthetic `BuildError`:

```rust
let timeout_err = BuildError {
    message: format!("Build timed out after {timeout_sec}s and was cancelled"),
    file: None, line: None, col: None,
    severity: BuildErrorSeverity::Error,
};
```

Set `bs.status = BuildStatus::Failed(...)` rather than leaving the `Cancelled` that
`cancel_build` set.

**Tests:** `timed_out_build_is_recorded_as_failed_with_reason`

### 2.6 — MCP builds emit `build:complete` (H3)

**Files:** `src-tauri/src/services/build_runner.rs`, `src-tauri/src/services/mcp_server.rs:307`

**Change:** add `app_handle: Option<&AppHandle>` to `run_task`. After finalization, if present:

```rust
if let Some(app) = app_handle {
    let _ = app.emit("build:complete", event);
}
```

Route the finalization through the existing `commands::build::finalize_completed_build` so both
paths produce an identical `BuildCompleteEvent`. That function is `pub(crate)` — either widen it
or (preferred) **move it into `build_runner`** and have `commands/build.rs` call it from there.
Moving it puts all build finalization in one module and lets `commands/build.rs` shrink.

`AndroidMcpServer` already holds `app_handle: Option<AppHandle>` (used by `start_logcat` at
`mcp_server.rs:807`) — pass `self.app_handle.as_ref()`.

**Tests:**

- `run_task_emits_build_complete_when_app_handle_present`
- `run_task_succeeds_without_app_handle` (headless mode must still work)

**Frontend:** no change needed — `build.service.ts` already listens for `build:complete`. Confirm
manually that an MCP-triggered build now updates the Build panel.

### 2.7 — Drop the dead `setBuildResult` params (L19)

**File:** `src/stores/build.store.ts:181-193`, callers in `src/services/build.service.ts:52, 172, 189`

`errorCount` / `warningCount` are accepted and never used (counts come from streamed lines).
Remove them from the signature and from all three call sites. Pure cleanup, no behavior change.

**Tests:** existing `build.store.test.ts` must stay green; adjust call sites in the test file.

**Commit:** `refactor(build): unify UI and MCP build paths behind shared runner`

---

## Phase 3 — Process manager cancellation safety

Fixes **H5**.

### 3.1 — Don't SIGKILL a possibly-recycled PID

**File:** `src-tauri/src/services/process_manager.rs:34-42, 236-247`

**Change:**

1. Add to `ProcessRecord`, and clone it into the reader task:

   ```rust
   /// Set by the reader task immediately before `on_exit`. The delayed SIGKILL
   /// checks this so we never signal a PID the OS may have recycled.
   pub(crate) exited: Arc<AtomicBool>,
   ```

2. In the reader task, right before `manager_for_cleanup.lock().await.processes.remove(&id);`:

   ```rust
   exited_flag.store(true, Ordering::SeqCst);
   ```

3. In `cancel()`, capture `record.exited` and guard the delayed kill:
   ```rust
   let exited = record.exited.clone();
   tokio::spawn(async move {
       tokio::time::sleep(Duration::from_secs(5)).await;
       if exited.load(Ordering::SeqCst) { return; }
       #[cfg(unix)]
       unsafe { libc::kill(os_pid as libc::pid_t, libc::SIGKILL) };
   });
   ```

**Tests:** the Phase 0.3 `delayed_sigkill_does_not_fire_after_natural_exit` must now pass. Add
`sigkill_still_fires_for_process_that_ignores_sigterm` (spawn a shell that traps SIGTERM) —
mark `#[ignore]` if it's too slow for the default suite, and note it in the commit.

**Note:** the stronger fix is to keep the `Child` handle and use `child.kill()` (tokio refuses to
signal a reaped PID). That requires restructuring ownership between `spawn` and the reader task
and is **out of scope** — the `exited` flag closes the hole with ~10 lines.

**Commit:** `fix(process): guard delayed SIGKILL against recycled PIDs`

---

## Phase 4 — Frontend lifecycle and state guards

Fixes **M9**, **M10**, **M11**, **M12**, **L24**, **L25**. Independent of Phases 1–3.

### 4.1 — Idempotent `initBuildService` (M9)

**File:** `src/services/build.service.ts:42-44`

The guard is checked before the `await`, so concurrent calls both register. Use the pattern
already proven in `src/stores/mcp.store.ts` (set the sentinel **synchronously** before awaiting):

```ts
let buildCompleteUnlisten: (() => void) | null = null;
let buildListenerInit: Promise<void> | null = null;

export function initBuildService(): Promise<void> {
  if (buildListenerInit) return buildListenerInit;
  buildListenerInit = (async () => {
    const unlisten = await listenBuildComplete((e) => {
      /* unchanged */
    });
    if (buildCompleteUnlisten) {
      unlisten();
      return;
    } // superseded by a reset
    buildCompleteUnlisten = unlisten;
  })().catch((err) => {
    buildListenerInit = null; // allow retry after failure
    throw err;
  });
  return buildListenerInit;
}

/** Test-only teardown, mirroring resetMcpListenersForTests. */
export function resetBuildServiceForTests(): void {
  buildCompleteUnlisten?.();
  buildCompleteUnlisten = null;
  buildListenerInit = null;
}
```

**Tests:** the Phase 0.4 test must now pass. Add `initBuildService retries after a failed listen`.

### 4.2 — Clear stale `selectedSerial` on disconnect (M10)

**File:** `src/stores/device.store.ts:98-107`

```ts
export function setDevices(devices: Device[]): void {
  setDeviceState("devices", devices);
  const current = deviceState.selectedSerial;
  const stillOnline =
    current !== null && devices.some((d) => d.serial === current && d.connectionState === "online");
  if (current !== null && !stillOnline) {
    setDeviceState("selectedSerial", null);
  }
  if (!deviceState.selectedSerial) {
    const first = devices.find((d) => d.connectionState === "online");
    if (first) setDeviceState("selectedSerial", first.serial);
  }
}
```

**Tests** (`src/stores/device.store.test.ts`):

- `clears selection when the selected device goes offline`
- `auto-selects a newly connected device after the previous one disconnects`
- `keeps the selection when the device is still online`

**Watch out:** this now fires `selectedSerial` changes from the device-list event. Confirm
`onDeviceChange` / `saveActiveProjectMeta` isn't triggered by it — `setDevices` doesn't call
`_onDeviceChange`, only `pickDevice` does, so this is safe. Add a test asserting that.

### 4.3 — Surface backend selection failures (M11)

**Files:** `src/stores/device.store.ts:127-133`, `src/stores/variant.store.ts:200-207`

Both swallow the error with "in-memory selection is still valid". The backend's selection is what
MCP tools and `None`-serial commands resolve against, so a silent failure means the UI and the
agent target different devices.

```ts
export async function pickDevice(serial: string): Promise<void> {
  const previous = deviceState.selectedSerial;
  setDeviceState("selectedSerial", serial);
  try {
    await selectDeviceApi(serial);
  } catch (err) {
    setDeviceState("selectedSerial", previous); // roll back
    showToast(`Failed to select device: ${formatError(err)}`, "error");
    return; // do not persist a failed selection
  }
  _onDeviceChange?.(serial);
}
```

Same shape for `selectVariant`. **Note the behavior change:** selection now fails visibly instead
of silently diverging. That is the point.

**Tests:** `pickDevice rolls back and toasts when the backend rejects`;
`selectVariant rolls back when set_active_variant fails`.

### 4.4 — Guard concurrent project switches (M12)

**File:** `src/services/project.service.ts:75-103`

Copy the generation pattern from `variant.store.ts` (`isCurrentProject`):

```ts
let openGeneration = 0;

async function doOpenProject(path: string): Promise<OpenProjectResult | null> {
  const generation = ++openGeneration;
  const isCurrent = () => generation === openGeneration;

  setLoading(true);
  try {
    const projectName = await openProject(path);
    if (!isCurrent()) return null;
    const canonicalRoot = (await getProjectRoot().catch(() => null)) ?? path;
    if (!isCurrent()) return null;
    const gradleRoot = await getGradleRoot().catch(() => null);
    if (!isCurrent()) return null;
    setProject(canonicalRoot, projectName, gradleRoot);

    const appId = await getApplicationId().catch(() => null);
    if (!isCurrent()) return null;
    setApplicationId(appId);
    setMinePackage(appId);
    // … unchanged …
    return { root: canonicalRoot, projectName };
  } catch (err) {
    /* unchanged */
  } finally {
    if (isCurrent()) setLoading(false);
  }
}
```

**Tests** (`src/services/project.service.test.ts`):

- `a superseded project open does not write to the store`
- `the winning open sets projectRoot and activeProjectId`

### 4.5 — Async-mount listener leak (L24)

**Files:** `src/App.tsx:353-361`, `src/components/logcat/LogcatPanel.tsx:590-637`

If the component unmounts before the awaited `listen()` resolves, the assignment lands after
`onCleanup` ran and the listener leaks. Low impact today (panels use `display:none` and never
unmount) but it's a landmine and it blocks component-level testing.

```ts
let disposed = false;
onCleanup(() => {
  disposed = true; /* existing unlisten calls */
});

onMount(async () => {
  const un = await listenLogcatEntries(/* … */);
  if (disposed) {
    un();
    return;
  }
  unlistenEntries = un;
});
```

Apply to all four LogcatPanel listeners and to `unlistenClose` in App.tsx. **Do not** restructure
anything else in these two files — they are the #2 and #4 churn hotspots in the repo.

### 4.6 — Keep pinned projects on top after upsert (L25)

**File:** `src/stores/projects.store.ts:32-44`

`upsertProject` `unshift`s without re-sorting, so a freshly opened unpinned project can sit above
pinned ones. Extract the comparator already inlined in `setPinned` and apply it in both:

```ts
function sortProjects(list: ProjectEntry[]): void {
  list.sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return b.lastOpened.localeCompare(a.lastOpened);
  });
}
```

**Tests:** new `src/stores/projects.store.test.ts` (see Phase 6.3) covering upsert ordering,
removal, rename, and meta update.

**Commit:** `fix(frontend): idempotent listeners, selection rollback, project-switch guard`

---

## Phase 5 — Shared validation, settings cache, misc backend

Fixes **M7**, **M13**, **M16**, **L21**, **L22**, **L23**.

### 5.1 — Single source of truth for validators (M7)

**New file:** `src-tauri/src/utils/validation.rs`; register in `src-tauri/src/utils/mod.rs`.

Move the canonical implementations (the stricter `commands/` versions, which have length caps)
and have them return `Result<(), String>`:

```rust
pub fn validate_gradle_task(task: &str) -> Result<(), String>
pub fn validate_device_serial(serial: &str) -> Result<(), String>
pub fn validate_package_name(package: &str) -> Result<(), String>
```

Then:

- `commands/build.rs:46`, `commands/device.rs:39,61` → delete the bodies, keep thin wrappers that
  map `String` → `AppError::InvalidInput`.
- `services/mcp_server.rs:2939,2955,2974` → delete the bodies, map to
  `McpError::invalid_params`.

Move the existing tests (`mcp_server.rs:3275-3320` and any in `commands/`) into
`utils/validation.rs`. Add a length-cap test for serials — the MCP path had no cap before, so
this is a real behavior change on that path (now rejects >64 chars).

**Also fix the `logcat.rs` / `mcp_server.rs` start/stop duplication introduced in Phase 1.1** by
extracting `services/logcat::{request_start, request_stop}` helpers that both callers use.

### 5.2 — Cache settings, stop blocking the runtime (M13)

**File:** `src-tauri/src/services/settings_manager.rs`

`load_settings()` does a sync read + full JSON parse and is called **67 times**, including inside
`run_gradle_task`, `start_logcat`, and `record_build_result` — all on tokio worker threads.

**Change:**

```rust
static SETTINGS_CACHE: LazyLock<RwLock<Option<AppSettings>>> =
    LazyLock::new(|| RwLock::new(None));

/// Cached read. The cache is invalidated by every mutation path.
pub fn load_settings() -> (AppSettings, bool) {
    if let Some(s) = SETTINGS_CACHE.read().ok().and_then(|g| g.clone()) {
        return (s, false);
    }
    let (settings, corrupted) = load_settings_from_path(&settings_file());
    if let Ok(mut g) = SETTINGS_CACHE.write() { *g = Some(settings.clone()); }
    (settings, corrupted)
}

pub fn invalidate_settings_cache() {
    if let Ok(mut g) = SETTINGS_CACHE.write() { *g = None; }
}
```

Call `invalidate_settings_cache()` at the end of `save_settings`, `mutate_settings_at_path*`,
and `reset_settings`. Add `#[cfg(test)] pub fn reset_cache_for_tests()`.

**⚠ Correctness note:** the `was_corrupted` flag must not be cached as `true` — it's a one-shot
signal consumed at startup. Returning `false` on cache hits (as above) is correct because a
corrupt file is repaired on first load.

**Also:** wrap the remaining blocking calls in `spawn_blocking`, following the pattern already at
`commands/file_system.rs:261`. Priority sites: `commands/build.rs` (settings load in
`run_gradle_task`) and `build_runner::record_build_result` — the latter also calls
`save_build_history()` **while holding the `inner` tokio mutex**. Restructure so the mutex guard
is dropped before the disk writes:

```rust
let (record_id, history_snapshot) = { /* lock, mutate, clone, drop */ };
tokio::task::spawn_blocking(move || {
    save_build_history(&history_snapshot);
    save_build_log(record_id, &raw_lines);
    rotate_build_logs(/* … */);
}).await.ok();
```

**Tests:**

- `load_settings_uses_cache_on_second_call` (assert via a temp file mutated behind the cache)
- `mutate_settings_invalidates_cache`
- `record_build_result_does_not_hold_lock_during_disk_io` — assert a concurrent
  `build_state.inner.lock()` acquires promptly during finalization

### 5.3 — Stable persisted project IDs (M16)

**File:** `src-tauri/src/commands/file_system.rs:14-18`

`DefaultHasher` output is explicitly **not** guaranteed stable across Rust releases, but the IDs
are persisted in `settings.json` and used to match `activeProjectId`, pins, and per-project meta.
A toolchain bump silently orphans every stored entry.

**Change:** replace with a stable hash. `sha2 = "0.11"` is **already a dependency**
(`src-tauri/Cargo.toml:45`) — no new crate needed.

```rust
use sha2::{Digest, Sha256};

/// Stable across Rust versions — these IDs are persisted in settings.json,
/// so the hash must never change implementation.
fn project_id(path: &std::path::Path) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    format!("{:016x}", u64::from_be_bytes(digest[..8].try_into().unwrap()))
}
```

Keep the 16-hex-char output width so existing ID formatting and any UI truncation stay valid.

**Migration:** in `upsert_project`, if an existing entry matches by `path` but has a different
`id`, rewrite the `id` in place. Do the same in `list_projects` so stale IDs heal on first read.

**Tests:** `project_id_is_stable_for_known_input` (hardcode the expected hex — that's the point
of the test); `upsert_rewrites_legacy_id_for_same_path`.

### 5.4 — Clear `last_active_project` when the folder is gone (L23)

**File:** `src-tauri/src/commands/file_system.rs:260` or `src/services/project.service.ts:245`

Today a deleted project folder produces a "Failed to open project" toast on **every** launch,
forever. Preferred fix (backend, keeps the frontend dumb): in `get_last_active_project`, return
`None` and clear the setting if the path no longer exists.

```rust
if let Some(ref p) = settings.last_active_project {
    if !std::path::Path::new(p).exists() {
        let _ = settings_manager::mutate_settings(|s| s.last_active_project = None);
        return Ok(None);
    }
}
```

**Tests:** `get_last_active_project_clears_missing_path`.

### 5.5 — Binary search for logcat context (L21)

**File:** `src-tauri/src/services/log_store.rs:174-198`

`context_before` / `context_after` linear-scan up to 200k entries per call. Entry `id` is
monotonically increasing and the deque is ordered, so:

```rust
let anchor_index = match self.entries.binary_search_by_key(&anchor_id, |e| e.id) {
    Ok(i) => i,
    Err(_) => return Vec::new(),
};
```

The existing tests (`log_store.rs:395-424`) already cover the semantics — they must stay green
unchanged. Add `context_lookup_returns_empty_for_evicted_anchor`.

### 5.6 — Hoist per-call regexes (L22)

**File:** `src-tauri/src/services/variant_manager.rs:203, 228, 229, 394, 412, 598`

Move to `LazyLock<Regex>` statics, matching the pattern already used in
`services/build_parser.rs:48-66`. Pure refactor; existing variant tests must stay green.

**Commit:** `refactor(backend): shared validators, settings cache, stable project ids`

---

## Phase 6 — Tests, CI guards, cleanup

Fixes **L26**, **L27**, and the seven coverage gaps.

### 6.1 — Fix pre-existing clippy lints and widen CI

**Files:** `src-tauri/src/services/ui_automation.rs:1924-1925`, `src-tauri/benches/fs_benchmarks.rs`,
`.github/workflows/ci.yml`

1. `ui_automation.rs:1924` → `assert!(...)` instead of `assert_eq!(x, true)`;
   `:1925` → `!m[0].tree_path.is_empty()`.
2. `benches/fs_benchmarks.rs` → `std::hint::black_box` instead of the deprecated
   `criterion::black_box`.
3. CI `rust` job: change `cargo clippy -- -D warnings` to
   `cargo clippy --all-targets -- -D warnings`, and add `cargo fmt --check`.
4. CI `frontend` job: add `npx prettier --check "src/**/*.{ts,tsx,css}"`.

**⚠ Blocker:** step 4 fails immediately — **57 files** currently have prettier drift. Run
`npm run format` as its own commit _first_ (a pure-formatting commit, no logic), then enable the
check. Do not mix that reformat into any other commit.

### 6.2 — IPC contract drift guard

**New file:** `src/test/ipc-contract.test.ts`

I verified the contract is currently **perfectly aligned** — 67 commands registered in
`generate_handler!`, 67 invoked from `src/`, zero drift either way, and all 67 covered by the
mock backend. Nothing to fix; this test exists so it stays that way. The `finalize_build` removal
in `7266133` required coordinated edits in four files with nothing to catch a miss.

```ts
// Parse generate_handler![…] from lib.rs (strip // comments before splitting on commas —
// a naive split swallows the first entry after each comment line).
// Parse invoke("…") / invoke<T>("…") from src/**, excluding tests and mock-backend.
// Parse the handler keys from src/test/mock-backend/**.
// Assert: invoked ⊆ registered, and invoked ⊆ mocked.
```

Do **not** assert `registered ⊆ invoked` — MCP-only commands may legitimately have no frontend
caller.

### 6.3 — Tests for the four untested stores

| File                                | Cover                                                                                                                           |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `src/stores/projects.store.test.ts` | upsert ordering (L25), remove, rename, pin re-sort, meta update                                                                 |
| `src/stores/health.store.test.ts`   | `refreshHealthChecks` concurrency guard, error path clears `isRunning`, `healthChecks()` status matrix for sdk/adb permutations |
| `src/stores/log.store.test.ts`      | `createLogStore` cap/eviction, `pushEntries` batching, `clearEntries`                                                           |
| `src/stores/monitor.store.test.ts`  | signal updates from a `monitor://stats` payload — **needs 6.3a first**                                                          |

**6.3a — `monitor.store.ts` registers a listener as a module side effect.**
`src/stores/monitor.store.ts:27-33` calls `listen("monitor://stats", …)` at import time, guarded
only by `typeof window !== "undefined"`, with no unlisten. Merely importing the module starts a
listener, which is why it has no tests — and it's the same lifecycle class as M9.

Refactor to an explicit init, matching `initMcpListeners`:

```ts
let monitorUnlisten: UnlistenFn | null = null;
let monitorInit: Promise<void> | null = null;

export function initMonitorListeners(): Promise<void> {
  /* same shape as 4.1 */
}
export function resetMonitorListenersForTests(): void {
  /* … */
}
```

Call `initMonitorListeners()` from `App.tsx`'s `onMount`, next to the existing
`initMcpListeners()` call (`App.tsx:135-140`). Then the store is testable.

**6.3b — remove the dead `buildLogOutput` export.** `src/stores/log.store.ts:62` exports
`createLogStore({ maxEntries: 2000 })` as `buildLogOutput`; I confirmed it has **zero**
references anywhere in `src/`. `build.store.ts` creates its own `buildLogStore`. Delete it.

### 6.4 — MCP server test coverage

**File:** `src-tauri/src/services/mcp_server.rs` — 8 tests / 3330 lines, and only the validators
are covered. After Phase 5.1 moves those out, it will have ~4.

Add tests for the tools that mutate shared state (the ones that can desync the UI):
`run_gradle_task` (slot reservation, emit), `start_logcat` / `stop_logcat` (generation),
`set_active_variant`, `cancel_build`. Construct `AndroidMcpServer` with a `None` app handle and
assert against `BuildState` / `LogcatState` directly.

This is the largest remaining gap after the fixes land. Timebox it — full coverage of a
3330-line file is a separate project. Target the ~6 state-mutating tools.

### 6.5 — Coverage reporting (L27)

**File:** `vite.config.ts:68`

```ts
test: {
  // … existing …
  coverage: {
    provider: "v8",
    reporter: ["text", "html"],
    include: ["src/stores/**", "src/services/**", "src/lib/**"],
  },
}
```

Add `"test:coverage": "vitest run --coverage"` to package.json. **Report only — do not add
thresholds yet.** Set them in a follow-up once you've seen the real numbers; a threshold picked
blind either fails CI on day one or is set so low it's meaningless.

### 6.6 — Decision records and docs

Per CLAUDE.md, after the code changes:

1. `update_decision_records(action="list")` to review existing records.
2. Create records for:
   - **Generation-scoped logcat stream lifecycle** — why a generation token beats a boolean flag
     (a flag can be flipped back on before the old task observes it).
   - **Single build execution path shared by UI and MCP** — why both front doors must go through
     one runner, and that MCP builds now emit `build:complete`.
   - **Exit code is authoritative for build success** — the 2.3 rule and its rationale.
   - **Stable hashing for persisted project IDs** — why `DefaultHasher` is unsafe to persist.
   - **Settings are cached in-process** — invalidation contract for future contributors.
3. Update `docs/DOMAIN_PATTERNS.md`: the build-lifecycle section now describes one path, not two.
4. Update `docs/MCP_SERVER.md` if it documents MCP build behavior that changed.

**Commit:** `test: add IPC contract guard, store tests, and coverage reporting`

---

## Commit sequence

| #   | Commit                                                                             | Phase                           |
| --- | ---------------------------------------------------------------------------------- | ------------------------------- |
| 1   | `test: add characterization tests for logcat, build, and process lifecycles`       | 0                               |
| 2   | `fix(logcat): generation-scoped stream lifecycle, bounded reconnect, drop counter` | 1                               |
| 3   | `refactor(build): unify UI and MCP build paths behind shared runner`               | 2                               |
| 4   | `fix(process): guard delayed SIGKILL against recycled PIDs`                        | 3                               |
| 5   | `fix(frontend): idempotent listeners, selection rollback, project-switch guard`    | 4                               |
| 6   | `refactor(backend): shared validators, settings cache, stable project ids`         | 5                               |
| 7   | `style: apply prettier to all source files`                                        | 6.1 (pure formatting, no logic) |
| 8   | `ci: widen clippy to all targets, add fmt and prettier checks`                     | 6.1                             |
| 9   | `test: add IPC contract guard, store tests, and coverage reporting`                | 6.2–6.5                         |
| 10  | `docs: record build and logcat lifecycle decisions`                                | 6.6                             |

Commit 3 is the big one — consider splitting it along the 2.1–2.7 sub-item boundaries if the
diff exceeds ~400 lines.

---

## Manual verification (after all phases)

The automated suite can't reach these. Run against a real device or emulator:

1. **Logcat restart** — start logcat, hit Restart 5× rapidly. Confirm no duplicate entries and
   exactly one `adb logcat` in `ps aux | grep logcat`.
2. **Logcat stop on an idle device** — start logcat on an idle emulator, stop, confirm the
   `adb logcat` process is gone within ~1s.
3. **Device switch while streaming** — switch devices mid-stream; confirm the stream follows.
4. **MCP build visibility** — with the app open, run `run_gradle_task` from Claude Code; confirm
   the Build panel shows progress and history refreshes.
5. **Concurrent build rejection** — start a UI build, then trigger an MCP build; confirm a clear
   "already running" error and that cancelling still kills the right process.
6. **Device disconnect** — unplug the selected device mid-session, plug in another; confirm
   auto-selection without a picker dialog.
7. **Corrupt settings recovery** — already covered by the test added in `7266133`; spot-check
   that `~/.keynobi/settings.json.corrupt` appears and the app starts with defaults.

---

## Explicitly out of scope

- Rewriting `process_manager::cancel` to hold the `Child` handle (noted in 3.1) — larger
  ownership refactor; the `exited` flag closes the actual hole.
- Splitting `mcp_server.rs` (3330 lines) into modules — worth doing, but it is a refactor
  project of its own and would collide with every phase here.
- Comprehensive `mcp_server.rs` test coverage beyond the ~6 state-mutating tools (6.4).
- Any UI/UX change. Findings that surface new information to the user (dropped-line counter,
  selection-failure toast, logcat give-up notice) reuse existing surfaces.

---

## Execution record (2026-08-18)

| Commit    | Phase   | Scope                                                                                |
| --------- | ------- | ------------------------------------------------------------------------------------ |
| `884a8be` | 3       | Delayed-SIGKILL guard against recycled PIDs (H5)                                     |
| `d4baf45` | 0, 1    | Logcat generation lifecycle, bounded reconnect, drop counter (C1, H4, M14, M15, M18) |
| `a5fecab` | 2       | Unified build path (H2, H3, H6, M8, M17, L19, L20)                                   |
| `11c95fd` | 0, 4    | Frontend listeners, selection rollback, project-switch guard (M9–M12, L24, L25)      |
| `34f0ab6` | 5       | Shared validators, settings cache, stable project ids (M7, M13, M16, L21–L23)        |
| `b7ab2f4` | 6.1     | Pre-existing clippy lints cleared                                                    |
| `7736dae` | 6.1     | Repo-wide prettier (formatting only)                                                 |
| `f89c45d` | 6.2–6.5 | IPC contract guard, MCP + store tests, coverage, CI widening                         |

**Test totals:** Rust 386 → 451 (lib+tests). Frontend 1121 → 1165.

### Verified-red-first

Four tests were confirmed to fail before their fix, and three were re-verified by
temporarily reverting the fix afterwards:

- `stop_then_start_does_not_leave_two_streams` — measured **4** live adb
  processes; **2** with the generation guard removed; passes with it.
- `stop_on_idle_device_terminates_child` — hung past a 3 s timeout; still fails
  if the reader's shutdown select arm is removed.
- `delayed_sigkill_does_not_fire_after_natural_exit` — did not compile (the
  `exited` guard it asserts on did not exist).
- `initBuildService registers exactly one listener…` — `resetBuildServiceForTests
is not a function`, then a genuine double registration.
- IPC contract guard — verified by deleting a `generate_handler!` entry; the test
  fails and names the calling file.

### Found during execution (not in the original review)

1. **MCP `clear_logcat` diverged from the command version** — it neither bumped
   `clear_epoch` (so buffered lines reappeared right after an agent's clear) nor
   emitted `logcat:cleared` (so the UI kept rendering cleared entries). Caught by
   the new MCP state-mutation tests. Same class as H3.
2. **Three pre-existing clippy lints** blocked `cargo clippy --all-targets`,
   which is why CI only ran clippy on default targets and test-code lints went
   unenforced. Cleared in `b7ab2f4`; CI now runs `--all-targets`.

### Deliberate deviations from the plan

- **1.2 skipped as planned.** `handleRestart`'s three-call sequence was left
  alone: the race is closed backend-side, and adding a `restart_logcat` command
  would violate "can this be done with existing commands?"
- **M10 narrowed.** The plan cleared a stale `selectedSerial` whenever the device
  was absent. Implemented as "absent from a **non-empty** list" — ADB reports zero
  devices during a server restart, and dropping the user's choice over a transient
  blip is worse than briefly holding a stale serial. Two existing tests that
  documented the old behavior were updated with this reasoning.
- **IPC contract test lives in `scripts/`, not `src/test/`.** `src/` has no node
  type definitions (`types: ["vitest/globals"]`), and adding `@types/node`
  repo-wide for one test was the larger change. `scripts/**/*.test.mjs` is an
  existing, already-included convention.
- **Validator tests left in `mcp_server.rs`.** The plan moved them wholesale; the
  originals now exercise the error-type mapping of the thin wrappers, which is
  worth keeping, and deleting them would add churn to a 99th-percentile hotspot.
- **No coverage thresholds set**, as planned — reporting only.

### Remaining follow-ups

- **`mcp_server.rs` coverage is still thin** (14 tests / 3330 lines). The ~6
  state-mutating tools are now covered; the UI-automation and device tools are
  not. A dedicated pass is warranted.
- **Start/stop/clear logic is still duplicated** between `commands/logcat.rs` and
  `mcp_server.rs`, now with `KEEP IN SYNC` comments on both. Extracting
  `services::logcat::{request_start, request_stop, request_clear}` would close it
  for good — deferred to keep this change reviewable.
- **`process_manager::cancel` still signals a raw PID.** The `exited` flag closes
  the hole; holding the `Child` handle and using `child.kill()` would be
  structurally safer but needs an ownership refactor.
- **Splitting `mcp_server.rs` into modules** remains out of scope.

### Self-review findings (commit `6329fa9`)

Re-reading the full diff surfaced **6 defects introduced by the hardening work
itself**, one critical. This is the strongest argument for the manual gate below:
the automated suite was fully green while all six were present.

| Severity | Defect                                                                                                                                      |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| CRITICAL | `run_task` reserved the build slot then returned via `?` on spawn failure — `starting` stayed true, wedging every later build until restart |
| HIGH     | `consecutive_failures` never reset, so a long session died after 10 _cumulative_ ADB restarts that each recovered fine                      |
| MEDIUM   | drop counter was task-local: reset on reconnect, and `clear` let the pre-clear value reappear one tick later                                |
| MEDIUM   | settings cache went stale across processes (headless `--mcp` is a separate process) and broke hand-edited settings.json                     |
| LOW      | `spawn_blocking` result discarded — build-history persistence could fail completely silently                                                |
| LOW      | `give_up` was inserted between `start_logcat_stream`'s doc block and the function, reattaching the docs to the wrong item                   |

Three were verified to fail without their fix. One test (the reconnect budget)
initially passed _with and without_ the fix because its window was too short —
caught only by revert-verifying. Tests that cannot fail are worse than no tests.

### Manual verification still outstanding

The 7 device-dependent checks listed above have **not** been run — they need a
real device or emulator. The automated suite cannot reach them.
