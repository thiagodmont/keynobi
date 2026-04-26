# Domain Patterns

Domain-specific implementation details. These supplement the foundational rules in `CODE_PATTERN.md` with per-domain context needed when working in a specific area.

---

## Table of Contents

1. [Build System](#build-system)
2. [Device Management](#device-management)
3. [Logcat Pipeline](#logcat-pipeline)
4. [UI hierarchy (layout viewer)](#ui-hierarchy-layout-viewer)
5. [MCP Server](#mcp-server)

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

## UI hierarchy (layout viewer)

Black-box capture of the focused window via **UI Automator** (`uiautomator dump`), parsed into a bounded tree for the **Layout** tab and MCP.

### Rust services

- **`services/ui_hierarchy.rs`** — `capture_ui_hierarchy_snapshot` is the single pipeline for the Layout tab, MCP, and UI automation: log `dumpsys activity activities`, `probe_foreground_activity` (first **512 KiB** for a resumed-activity line), `probe_layout_context` (parallel **12s** cap) for capped excerpts of `dumpsys window windows` (**12 KiB**), `dumpsys display` (**8 KiB**), `wm size`, `wm density`, then `dump_hierarchy_xml`. **`dump_hierarchy_xml`** tries `exec-out uiautomator dump --compressed /dev/tty`, then plain `exec-out`, then `shell uiautomator dump` with and without `--compressed` + `exec-out cat` of `window_dump.xml` paths. Raw XML is capped at **4 MiB**; dump attempts use a **25s** timeout. `strip_ui_automator_noise` trims junk around the XML. `build_snapshot` attaches `UiLayoutContext` + `command_log` (every adb line). See `ui_hierarchy_parse::tests::parses_minified_single_line_xml` for **`--compressed`**-style whitespace.
- **`services/ui_hierarchy_parse.rs`** — `roxmltree` → `UiNode` tree with caps: **8000** nodes, depth **64**, attribute strings **2048** chars. Parses **`selected`** (e.g. bottom nav). `compute_screen_hash` (SHA-256) fingerprints interactive-relevant fields. `extract_interactive_rows` produces flat rows for MCP (`interactive_only`).
- **`services/ui_automation.rs`** — MCP UI control: `capture_ui_snapshot` (same dump path as Layout), DFS **`find_ui_elements`** with `treePath` aligned to the layout viewer (`0`, `0.1`, …), **`find_ui_parent_from_snapshot`** / **`normalize_tree_path`** / **`get_node_at_path`** for one-step parent traversal, match cap **100**, string fields truncated for JSON; **`encode_adb_input_text`** for `adb shell input text`; allowlisted **`send_ui_key`** keyevents; `adb_input_tap` / `adb_input_swipe` with coordinate bounds **0..=16384**; **`grant_runtime_permission`** validates `android.permission.*` + package. Unit-tested with hierarchy fixtures (no device).
- **Fixtures** — `services/fixtures/ui_hierarchy_*.xml` for unit tests (classic View + Compose-shaped XML).

### IPC

- **Command** — `dump_ui_hierarchy` in `commands/ui_hierarchy.rs`; `device_serial: Option<String>` (omit → use `DeviceState.selected_serial`). Reuses `commands::device::validate_device_serial`.
- **Types** — `models/ui_hierarchy.rs`: `UiNode` (includes **`selected`**), `UiLayoutContext` (window/display/wm excerpts), `UiHierarchySnapshot` (`ts-rs` → `src/bindings/`; includes `layoutContext`, `command_log`). `UiInteractiveRow` is MCP-only (schemars, no TS).

### Frontend

- **`lib/ui-hierarchy-display.ts`** — `collapseBoringChains` (synthetic `android.view.KeynobiCollapsedWrappers` rows), `defaultExpandDepthForNodeCount`, search path helpers (`pathOverridesToRevealPath` expands through the match; **`pathOverridesToRevealAncestorPath`** opens only prefixes so a row is visible without expanding its subtree), **`parentLayoutPath`** (direct parent index path for **Find parent**), `formatRowSnippet` / `isMergedTapTargetHeuristic`, package inference for toolbar, and **wireframe** helpers (`parseBoundsRect`, `flattenNodesWithBounds`, `inferScreenSizeFromRects`, `pickNodePathAtDevicePoint`, `prepareWireframeDrawList`, cap **`WIREFRAME_RECT_CAP`**). Vitest covers collapse, paths, parent path vs tree, and wireframe parsing/hit-test.
- **`LayoutWireframe.tsx`** — SVG wireframe beside the tree; click uses SVG CTM inverse + `pickNodePathAtDevicePoint` (smallest-area win). Paths match the tree (`data-layout-path` + `scrollIntoView` on selection).
- **`layout-detail-get-node.ts`** — `layoutDetailGetNode(selectedNode)` builds `NodeDetailPanel`’s `getNode` callback. Do not pass Solid `Show`’s render-prop `n` into props (proxy / not callable at runtime). Vitest: `layout-detail-get-node.test.ts`.
- **`layoutViewer.store.ts`** + **`LayoutViewerPanel.tsx`** — refresh; **Hide boilerplate**, interactive-only, filter with prev/next match, **wireframe | tree | detail** layout, dominant package line; on **selection** (wireframe or tree), merge ancestor path overrides, force **`globalExpand`** back to `auto` if needed, then deferred `scrollIntoView`; footer shows `commandLog` from the latest snapshot.
- **Tab / shortcut** — `MainTab` includes `"layout"`; **Cmd+4** (`view.layoutPanel`).

### MCP

- **`get_ui_hierarchy`** — `GetUiHierarchyParams`: optional `device_serial` (resolved like `restart_app`), `interactive_only`, `max_interactive_rows` (default **80**, max **500**). Full mode returns `UiHierarchySnapshot` JSON; interactive mode returns `{ rows, screenHash, commandLog, … }` without the full tree.
- **`find_ui_elements`** — `FindUiElementsParams` (in `ui_automation.rs`, re-used by the tool router): optional `device_serial`, text/id/class/package filters, optional `clickable_only` / `editable_only` / `enabled_only`, `max_results` (default **50**, max **100**). Requires at least one primary filter (not flags alone). Returns `matches`, `screenHash`, `commandLog`, etc.
- **`list_clickable_elements`** — `ListClickableElementsParams`: optional `device_serial`, `enabled_only`, `max_results` (default/max **100**). Returns every clickable node in `UiElementMatch` shape without requiring a primary search filter.
- **`find_ui_parent`** — `FindUiParentParams`: optional `device_serial`, required non-empty `treePath` (same convention as the Layout tab), optional `expect_screen_hash`. Returns `parent` (`UiElementMatch` shape), `parentTreePath`, and snapshot metadata. Empty `treePath` is rejected (display root has no parent).
- **`ui_tap_element`** — `UiTapElementParams`: optional `device_serial`, required `treePath`, optional `expect_screen_hash`. Fresh-dumps, resolves the current center from the tree, then taps. Prefer over raw `ui_tap` when a `treePath` is available.
- **`ui_fill_input`** — `UiFillInputParams`: optional `device_serial`, required ASCII `text`, preferred `treePath` target or fallback `x`/`y`, optional `expect_screen_hash`, `clear_before` (default **true**). Always taps the target first before clearing/typing.
- **`hide_soft_keyboard`** — `HideSoftKeyboardParams`: optional `device_serial`, optional `force`. Checks `dumpsys input_method` and sends Back only when the soft keyboard appears visible; `force=true` sends Back when visibility is unknown.
- **`ui_wait_for_idle`** — `UiWaitForIdleParams`: optional `device_serial`, `stable_polls` (default **2**), `poll_interval_ms` (default **300**, min **200**), `timeout_ms` (default **5000**, max **30000**). Polls hierarchy `screenHash` until stable.
- **`ui_scroll_until_element`** — `UiScrollUntilElementParams`: search filters like `find_ui_elements`, optional explicit swipe coordinates, `max_swipes` (default **8**, max **25**). Infers a vertical scroll from hierarchy bounds when coordinates are omitted.
- **`ui_assert_element`** — `UiAssertElementParams`: search filters plus optional expected state flags (`clickable`, `editable`, `enabled`, `focused`, `checked`, `selected`) or `should_exist=false`. Returns MCP error when the assertion fails.
- **`open_deep_link`** — `OpenDeepLinkParams`: optional `device_serial`, required URI, optional package. Validates URI scheme/control chars, then runs `am start -a android.intent.action.VIEW -d <uri>` with optional `-p <package>`.
- **`open_app_settings`** — `OpenAppSettingsParams`: optional `device_serial`, required package, optional `panel` (`appInfo`, `permissions`, `notifications`). Opens Android app settings via `am start`.
- **`set_device_orientation`** — `SetDeviceOrientationParams`: optional `device_serial`, `orientation` (`portrait`, `landscape`, `reversePortrait`, `reverseLandscape`, `auto`). Uses `settings put system accelerometer_rotation` and `user_rotation`.
- **`set_network_state`** — `SetNetworkStateParams`: optional `device_serial`, optional `wifi`, `mobile_data`, `airplane_mode`. Best-effort; returns every adb command outcome because Android/emulator versions differ.
- **`ui_tap`**, **`ui_type_text`**, **`ui_swipe`**, **`send_ui_key`**, **`grant_runtime_permission`**, **`revoke_runtime_permission`** — parameter structs in `ui_automation.rs`; optional `device_serial`; `ui_tap` / `ui_type_text` support optional `expect_screen_hash` against a fresh dump.

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
- UI automation: coordinates bounded in `ui_automation`; runtime permissions via `validate_runtime_permission` (`android.permission.*`); keyevents via allowlist in `resolve_ui_key_code`

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
