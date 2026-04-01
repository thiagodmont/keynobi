# MCP Server Service Extraction Design

**Date:** 2026-04-01  
**Status:** Approved

## Goal

Apply the Tier 1 service-extraction pattern consistently across `mcp_server.rs`. Every `#[tool]` handler with inline business logic gets that logic extracted into a focused service module. `mcp_server.rs` becomes a file of thin handlers: validate inputs → call service → return JSON.

## Architecture

Two new service modules are created; three existing services gain new public functions.

```
src-tauri/src/services/
├── device_inspector.rs   ← get_device_info, dump_app_info, get_memory_info, take_screenshot
├── health_inspector.rs   ← run_health_check logic + detect_sdk_path
├── build_runner.rs       ← gains run_task() (spawn-poll-collect-timeout from run_gradle_task)
├── logcat.rs             ← gains level_char(), parse_level_str()
├── adb_manager.rs        ← gains resolve_device_serial()
└── mcp_server.rs         ← thin handlers; param structs + validate_* helpers stay here
```

`services/mod.rs` gains two new `pub mod` declarations: `device_inspector`, `health_inspector`.

---

## Module 1: `device_inspector.rs` (new)

**Purpose:** ADB-based device queries — properties, app info, memory, screenshot.

**Public types:**

```rust
#[derive(Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub build_version_sdk: Option<String>,
    pub build_version_release: Option<String>,
    pub product_manufacturer: Option<String>,
    pub product_model: Option<String>,
    pub product_name: Option<String>,
    pub build_fingerprint: Option<String>,
    pub build_id: Option<String>,
    pub display_size: Option<String>,
    pub battery: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct DumpedAppInfo {
    pub package: String,
    pub install_path: Option<String>,
    pub data_dir: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub first_install: Option<String>,
    pub raw_dump_excerpt: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryInfo {
    pub package: String,
    pub total_pss: Option<String>,
    pub java_heap_pss: Option<String>,
    pub java_heap_rss: Option<String>,
    pub native_heap_pss: Option<String>,
    pub native_heap_rss: Option<String>,
    pub graphics_pss: Option<String>,
    pub graphics_rss: Option<String>,
    pub raw: String,
}
```

**Public functions:**

```rust
pub async fn get_device_info(adb: &PathBuf, serial: &str) -> Result<DeviceInfo, String>
pub async fn dump_app_info(adb: &PathBuf, serial: &str, package: &str) -> Result<DumpedAppInfo, String>
pub async fn get_memory_info(adb: &PathBuf, serial: &str, package: &str) -> Result<MemoryInfo, String>
pub async fn take_screenshot(adb: &PathBuf, serial: &str) -> Result<Vec<u8>, String>
```

**Implementation notes:**
- `get_device_info`: runs 9 concurrent `getprop` calls + `wm size` + `dumpsys battery` via `tokio::join!`, same as current inline code
- `dump_app_info`: runs `pm path` + `dumpsys package` concurrently; returns `Err` when both outputs are empty (package not installed)
- `get_memory_info`: runs `dumpsys meminfo <package>`; returns `Err` when output is empty or contains "No process found"
- `take_screenshot`: runs `adb exec-out screencap -p`, returns raw PNG bytes; caller (handler) does base64 encoding
- `extract_dump_value` and `extract_dump_two_values` become **private** to this module

**Tests (inline `#[cfg(test)]`):**
- `extract_dump_value` finds version name/code
- `extract_dump_value` returns None for missing key
- `extract_dump_two_values` extracts PSS and RSS columns
- `extract_dump_two_values` returns (None, None) for missing key
- `extract_dump_two_values` handles single-column line
- `extract_dump_two_values` both non-zero values

These 6 tests migrate from `mcp_server.rs`.

**mcp_server.rs handlers after extraction:**
```rust
async fn get_device_info(&self, Parameters(p): ...) -> ... {
    validate_device_serial(&p.device_serial)?;
    let adb = adb_manager::get_adb_path(&settings_manager::load_settings());
    match device_inspector::get_device_info(&adb, &p.device_serial).await {
        Ok(info) => Ok(CallToolResult::structured(json!(info))),
        Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
    }
}
```

---

## Module 2: `health_inspector.rs` (new)

**Purpose:** Environment health check — Java, Android SDK, ADB, Gradle wrapper availability.

**Public types:**

```rust
#[derive(Debug)]
pub struct HealthReport {
    pub all_ok: bool,
    pub java_ok: bool,
    pub sdk_ok: bool,
    pub adb_ok: bool,
    pub gradlew_ok: bool,
    pub project_open: bool,
    pub detected_sdk: Option<String>,
    pub project_path: Option<std::path::PathBuf>,
}
```

`HealthReport` does **not** derive `Serialize` — the mcp_server.rs handler constructs the nested JSON (`"checks": { "java": { "ok": ..., "hint": ... } }`) from the flat struct fields. That presentation shape is a handler concern.

**Public functions:**

```rust
pub async fn run_health_check(
    settings: &AppSettings,
    project_root: Option<&Path>,
    gradle_root: Option<&Path>,
) -> HealthReport

pub fn detect_sdk_path(configured: Option<&str>, project_root: Option<&Path>) -> Option<String>
```

**Implementation notes:**
- `run_health_check`: runs `java -version` and `adb version` concurrently via `tokio::join!`; calls `detect_sdk_path`; if SDK is newly detected and differs from settings, persists it back via `settings_manager::save_settings`
- `detect_sdk_path`: 4-step priority chain — configured path → `local.properties` → env vars (`ANDROID_HOME`, `ANDROID_SDK_ROOT`) → standard platform locations; validates each candidate has `platforms/` or `platform-tools/`; `expand("~/...")` helper stays private

**Tests (inline `#[cfg(test)]`):**
- `detect_sdk_path` accepts valid configured path (has `platform-tools/`)
- `detect_sdk_path` ignores invalid configured path (no platforms dir)
- `detect_sdk_path` falls back to `local.properties`
- `detect_sdk_path` skips missing `local.properties` without panic
- `detect_sdk_path` does not return non-existent configured path

These 5 tests migrate from `mcp_server.rs`.

---

## Module 3: `build_runner.rs` enhancement

**New public types:**

```rust
pub struct GradleTaskResult {
    pub success: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub errors: Vec<BuildError>,
}
```

**New public function:**

```rust
pub async fn run_task(
    task: &str,
    extra_args: &[&str],
    gradle_root: &Path,
    gradlew: &Path,
    timeout_sec: u64,
    env: std::collections::HashMap<String, String>,
    build_state: &BuildState,
    process_manager: &ProcessManager,
) -> Result<GradleTaskResult, String>
// Err(_) = Gradle process failed to spawn
// Ok(result) with timed_out=true = build was cancelled after timeout
// Ok(result) with success=false = build failed normally
```

**Implementation notes:**
- Sets `BuildStatus::Running` before spawning
- Spawns via `process_manager::spawn` with line callbacks that call `parse_build_line`, accumulate `BuildError`s, and track duration via `AtomicU64`
- Polls every 200ms up to `timeout_sec`; on timeout calls existing `cancel_build` and sets `timed_out = true`
- Calls existing `record_build_result` before returning
- `run_gradle_task` handler becomes ~25 lines: validate → set variant → find gradlew → build args/env → call `run_task` → format message from `GradleTaskResult`

No new tests required — `run_task` is integration-level (spawns real processes); existing build flow is covered by manual testing. The extracted function is called from the same handler, so correctness is verified by the existing smoke tests.

---

## Module 4: `logcat.rs` — helper migration

Two free functions currently in `mcp_server.rs` belong in the logcat domain:

```rust
// Move to logcat.rs as pub fn:
pub fn level_char(level: &LogcatLevel) -> &'static str
pub fn parse_level_str(s: &str) -> LogcatLevel
```

`mcp_server.rs` call sites change from `level_char(&e.level)` to `logcat::level_char(&e.level)` and `parse_level_str(s)` to `logcat::parse_level_str(s)`.

No new tests needed — these are pure functions; they are covered implicitly by logcat pipeline tests.

---

## Module 5: `adb_manager.rs` — helper migration

One async free function moves to `adb_manager`:

```rust
// Move to adb_manager.rs as pub async fn:
pub async fn resolve_device_serial(adb: &PathBuf, requested: Option<&str>) -> Option<String>
```

It already uses `adb devices` output, which is `adb_manager`'s domain. All call sites in `mcp_server.rs` change from the local `resolve_device_serial(...)` to `adb_manager::resolve_device_serial(...)`.

---

## `mcp_server.rs` — what stays

- All `#[derive(Deserialize, JsonSchema)]` param structs (MCP protocol types)
- All `#[tool]` and `#[prompt]` handler method bodies (now thin: validate → call service → return JSON)
- `validate_gradle_task`, `validate_package_name`, `validate_device_serial` (MCP input guards)
- `capitalize_first` (used only in prompt template string formatting)
- `get_gradle_root`, `validate_apk_path` on `impl AndroidMcpServer` (access `self.fs_state`)
- `ServerHandler` impl, `LoggingMcpServer`, `start_mcp_server`, `run_headless_mcp`
- Tests for validators (`validate_gradle_task`, `validate_package_name`, `validate_device_serial`) and `capitalize_first`

**Estimated size reduction:** ~500 lines removed (handler bodies + migrated helpers + migrated tests), from ~2175 to ~1675 lines.

---

## Testing Strategy

Each new service module ships with an inline `#[cfg(test)]` module:
- `device_inspector`: unit tests on `extract_dump_value` and `extract_dump_two_values` (migrated from `mcp_server.rs`)
- `health_inspector`: unit tests on `detect_sdk_path` (migrated from `mcp_server.rs`)
- `build_runner` (`run_task`): no new unit tests; integration-level function covered by smoke testing

MCP handlers are thin after extraction and require no additional tests beyond the existing validator tests that stay in `mcp_server.rs`.
