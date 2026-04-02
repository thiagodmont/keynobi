# Domain Patterns

Domain-specific implementation details. These supplement the foundational rules in `CODE_PATTERN.md` with per-domain context needed when working in a specific area.

---

## Table of Contents

1. [Build System](#build-system)
2. [Device Management](#device-management)
3. [Logcat Pipeline](#logcat-pipeline)
4. [MCP Server](#mcp-server)

---

## Build System

### Build State

Build state follows the per-concern Mutex pattern. `BuildState` holds only the in-flight process ID, current status, errors, and bounded history. No Tauri types in `build_runner.rs`.

```rust
pub struct BuildState(pub Mutex<BuildStateInner>);
pub struct BuildStateInner {
    pub current_build: Option<ProcessId>,
    pub status: BuildStatus,
    pub history: VecDeque<BuildRecord>,   // bounded: MAX_HISTORY = 10
    pub current_errors: Vec<BuildError>,
    next_id: u32,
}
```

### Build Service Flow

`build.service.ts` coordinates multiple IPC calls into a user-visible action:

```
runBuild() → runGradleTask (IPC, returns immediately after spawn)
           → await build:complete event (one-shot Promise resolved by listener)
           → finalizeBuild (with correct success/duration from event)

runAndDeploy() → resolveDevice (pick dialog if no online device selected)
              → setDeployPhase("building") → runBuild
              → setDeployPhase("installing") → findApkPath → installApkOnDevice
              → setDeployPhase("launching") → launchAppOnDevice (using applicationId)
              → setDeployPhase(null)
```

**Key invariants:**
- `runBuild()` only resolves after `build:complete` fires — `buildState.phase` is always correct when it returns.
- `_resolveBuildComplete` is a module-level one-shot resolver. It is set before `runGradleTask` and cleared by the `listenBuildComplete` handler.
- If `cancelBuild()` is called mid-build, `_resolveBuildComplete` is nulled so the awaiting `buildComplete` promise is abandoned without hanging.
- Build errors accumulate in `accumulatedErrors` and are passed to `finalizeBuild` for backend persistence.

**Device resolution:** `resolveDevice()` checks if `deviceState.selectedSerial` is online; if not, it dynamically imports and shows `DevicePickerDialog`. The dialog is lazy-imported to avoid circular dependency between build service and device UI.

**DeployPhase:** `buildState.deployPhase` drives the status bar during the install/launch steps after a successful build. Always cleared in the `finally` block.

---

## Device Management

### Device State

Same per-concern Mutex pattern:

```rust
pub struct DeviceState(pub Mutex<DeviceStateInner>);
pub struct DeviceStateInner {
    pub devices: Vec<Device>,
    pub selected_serial: Option<String>,
    pub polling: bool,            // guards against double-starting the poll task
}
```

Device polling runs as a detached `tokio::spawn` task. When the device list changes, it emits `device:list_changed` via `AppHandle::emit`. The frontend listens and calls `setDevices()` in the store.

### AVD Management

AVD lifecycle (create/delete/wipe) goes through `avdmanager` CLI, resolved from `$ANDROID_HOME/cmdline-tools/`. Commands return the refreshed AVD list (`Vec<AvdInfo>`) so the frontend can update in one round-trip.

### DevicePanel Mode Pattern

`DevicePanel` accepts a `mode` prop for its two usage contexts:
- `mode="panel"` (default): full-width tab content with toolbar, two sections, AVD lifecycle actions
- `mode="popover"`: compact 280px dropdown for the StatusBar pill, with "Manage Devices" footer link

---

## Logcat Pipeline

### Architecture

The logcat data flow is a four-stage pipeline:

```
adb logcat
  → Ingester (parse raw lines → RawLogLine, zero mutex)
  → Pipeline+Batcher task (every 100ms: run processors, lock state once, emit filtered)
  → StreamManager (filter entries before IPC emit)
  → Frontend (render pre-filtered entries; frontend-only filtering for age/regex/negate)
```

### Processor Chain (`services/log_pipeline.rs`)

Processors implement the `LogProcessor` trait and run sequentially per entry:

```rust
pub trait LogProcessor: Send + Sync {
    fn process(&self, entry: &mut ProcessedEntry, ctx: &mut PipelineContext);
}
```

Built-in processors (in order):
1. **PackageResolver** — attaches `package` from PID→package map
2. **CrashAnalyzer** — detects FATAL/ANR/native, assigns `crash_group_id`
3. **JsonExtractor** — detects valid JSON in message, stores raw string in `json_body`
4. **CategoryClassifier** — O(1) tag→category lookup

`PipelineContext` is owned by the pipeline task (no mutex) and carries cross-entry state (pid_to_package, crash group tracking). It is synced into `LogcatStateInner.known_packages` once per 100ms tick.

### LogStore (`services/log_store.rs`)

Bounded ring buffer with secondary indexes for O(1) crash/JSON lookups:

```rust
pub struct LogStore {
    entries: VecDeque<ProcessedEntry>,  // 50K ring buffer
    crash_ids: VecDeque<u64>,           // IDs of crash entries
    json_ids: VecDeque<u64>,            // IDs of JSON entries
    pub stats: LogStats,                // running counters (O(1) per entry)
}
```

### Backend / Frontend Filter Split

`StreamState` holds the active `LogcatFilter`. The batcher calls `filter_batch()` before emitting — only matching entries cross the IPC bridge.

The frontend `filteredEntries` memo only applies tokens the backend cannot handle:
- `age:N` — time-based, needs `Date.now()`
- `-tag:X` / `-message:X` — negation
- `tag~:X` / `message~:X` — regex

Everything else (level, simple tag/text/package, only_crashes) is handled in Rust. This reduces the JS O(n) scan to a much smaller pre-filtered set.

### ProcessedEntry (IPC type)

`json_body` is `Option<String>` (not `Value`) to avoid double-serialisation. Frontend parses when user expands the JSON viewer panel.

---

## MCP Server

### Defining a New Tool

1. Add a parameter struct (if the tool takes arguments):

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MyToolParams {
    #[schemars(description = "What this parameter does")]
    pub my_param: String,
    pub optional_param: Option<usize>,
}
```

2. Add the method to `AndroidMcpServer`'s `#[tool_router]` impl block:

```rust
#[tool(description = "One-line description for the AI agent.")]
async fn my_tool(
    &self,
    Parameters(p): Parameters<MyToolParams>,
) -> Result<CallToolResult, McpError> {
    validate_my_input(&p.my_param)?;
    let value = { self.build_state.inner.lock().await.some_field.clone() };
    Ok(CallToolResult::success(vec![Content::text(format!("Result: {value}"))]))
}
```

3. For execution errors (not protocol errors), use `CallToolResult::error()`.

### Input Validation

All MCP tools that accept strings must validate inputs before acting:
- Gradle task names: `validate_gradle_task(task)?` — alphanumeric + `:.-_`
- Package names: `validate_package_name(pkg)?` — `com.example.app` format
- Device serials: `validate_device_serial(serial)?` — alphanumeric + `-:._`
- APK paths: `self.validate_apk_path(path).await?` — must be within project build outputs

### Headless vs GUI Mode

- **GUI mode**: `AndroidMcpServer::from_app_handle(&app)` — reads shared state from Tauri
- **Headless mode**: `AndroidMcpServer::new_headless(build, device, logcat, fs, pm)` — owns fresh state
- The `--mcp` CLI flag triggers headless mode in `main.rs`. `--project <path>` sets the project root.

### State Access

The state structs use `Arc<Mutex<>>` internally, so `AndroidMcpServer` is `Clone` (all fields are cheap Arc copies).

### Structured Outputs

- `CallToolResult::structured(json!({...}))` — data the AI will reason about (devices, errors, logcat)
- `CallToolResult::success(vec![Content::text(...)])` — human-readable text
- `Content::image(base64, "image/png")` — screenshots

### Resources and Prompts

- Resources: implement `list_resources` and `read_resource` in `impl ServerHandler`. Use `android://` URIs.
- Prompts: use `#[prompt_router]` and `#[prompt]` on a separate impl block before `#[tool_handler] #[prompt_handler] impl ServerHandler`.

### Lifecycle Events (GUI Mode)

Emit Tauri events from `on_initialized` and the `start_mcp_server` task:
- `mcp:started` — when the server task starts
- `mcp:client_connected` — from `on_initialized` with `{ clientName, connectedAt }`
- `mcp:stopped` — when the server task exits

Frontend subscribes via `initMcpListeners()` in `src/stores/mcp.store.ts`.
