# MCP Tier 1 Tools Design

**Date:** 2026-04-01  
**Status:** Approved

## Goal

Add four high-value MCP tools to the Android Dev Companion server that give Claude structured access to crash traces, app runtime state, build configuration, and app restart — reducing the need to parse raw logcat or shell output manually.

## Architecture

Three new service modules contain the business logic; `mcp_server.rs` holds thin tool handlers that call into them.

```
src-tauri/src/services/
├── crash_inspector.rs   ← get_crash_stack_trace logic
├── app_inspector.rs     ← get_app_runtime_state + restart_app logic
├── build_inspector.rs   ← get_build_config logic
└── mcp_server.rs        ← param structs + #[tool] handlers (call services)
```

`services/mod.rs` gains three new `pub mod` declarations.

---

## Tool 1: `get_crash_stack_trace`

**Purpose:** Return a fully-parsed crash from the in-memory logcat buffer, grouped by `crash_group_id`.

**Input parameters:**
- `package: Option<String>` — filter to this package; uses first match if omitted
- `crash_group_id: Option<u64>` — return this specific group; uses latest group if omitted

**Logic (`crash_inspector.rs`):**
1. Lock `LogcatState`, iterate `store.iter()` collecting entries where `crash_group_id.is_some()`
2. Group entries by `crash_group_id`
3. If `crash_group_id` given → pick that group; else pick the group with the highest ID for the requested package (or highest overall)
4. Parse the group's raw message lines:
   - Line containing `"FATAL EXCEPTION"` → marks start; the next line is `ExceptionClass: message`
   - Lines starting with `"\tat "` → stack frames; parse `com.Foo.bar(File.kt:42)` into struct fields
   - Lines starting with `"Caused by: "` → recurse to build chained `caused_by` array

**Output JSON:**
```json
{
  "crash_group_id": 3,
  "package": "com.example.app",
  "exception_type": "java.lang.NullPointerException",
  "message": "Attempt to invoke virtual method...",
  "frames": [
    { "class": "com.example.MyActivity", "method": "onCreate", "file": "MyActivity.kt", "line": 42 }
  ],
  "caused_by": [
    { "exception_type": "java.io.IOException", "message": "...", "frames": [...] }
  ],
  "raw_line_count": 18
}
```

**Error cases:**
- `"No crashes found"` / `"No crash found for package X"` — buffer has no crash entries
- `"Logcat not running — call start_logcat first"` — `streaming == false && store is empty`

---

## Tool 2: `restart_app`

**Purpose:** Stop an app (optionally clearing state), relaunch it, and wait for the main activity to be displayed.

**Input parameters:**
- `package: String` — Android package name
- `device_serial: Option<String>` — ADB serial; uses first online device if omitted
- `cold: bool` (default `true`) — `pm clear` (cold) vs `am force-stop` (warm)

**Logic (`app_inspector.rs`):**
1. Resolve device serial: use param or first online device from `adb devices`
2. Stop: `adb shell pm clear <package>` (cold) or `adb shell am force-stop <package>` (warm)
3. Resolve launcher activity: `adb shell cmd package resolve-activity --brief -c android.intent.category.LAUNCHER <package>` → parse component string `com.example/.MainActivity`
4. Launch: `adb shell am start -n <component>`
5. Poll logcat buffer every 200 ms up to 10 s for an `ActivityManager` entry whose message contains `"Displayed <package>"` → parse display time from `"+850ms"` suffix

**Output JSON:**
```json
{
  "launched": true,
  "activity": "com.example.app/.MainActivity",
  "display_time_ms": 850,
  "cold_start": true
}
```

**Error cases:**
- Package not installed → `"Package com.example.app not found on device"`
- Launcher activity not found → `"Could not resolve launcher activity for package"`
- Launch timeout (no "Displayed" within 10 s) → still returns `"launched": true`, `"display_time_ms": null`

---

## Tool 3: `get_app_runtime_state`

**Purpose:** Return the process list + thread count + RSS memory for all processes belonging to a package.

**Input parameters:**
- `package: String` — Android package name
- `device_serial: Option<String>` — ADB serial; uses first online device if omitted

**Logic (`app_inspector.rs`):**
1. `adb shell ps -A -o PID,NAME` → find all rows where `NAME` starts with or equals `package` (covers `:push`, `:sync` sub-processes)
2. For each PID: `adb shell ps -T -p <pid>` → count output rows minus header = thread count
3. For each PID: `adb shell cat /proc/<pid>/status` → extract `VmRSS` line → parse KB value

Run steps 2 and 3 concurrently per PID with `tokio::join!`.

**Output JSON:**
```json
{
  "package": "com.example.app",
  "running": true,
  "processes": [
    { "pid": 12345, "name": "com.example.app", "thread_count": 47, "rss_kb": 128456 },
    { "pid": 12389, "name": "com.example.app:push", "thread_count": 8, "rss_kb": 24000 }
  ],
  "total_threads": 55,
  "total_rss_kb": 152456
}
```

**Error cases:**
- App not running → `"running": false, "processes": [], "total_threads": 0, "total_rss_kb": 0`

---

## Tool 4: `get_build_config`

**Purpose:** Extract SDK levels, build types, and product flavors from the module's Gradle build file without executing Gradle.

**Input parameters:**
- `module: Option<String>` (default `"app"`) — subdirectory name relative to the Gradle root

**Logic (`build_inspector.rs`):**
1. Resolve Gradle root from `FsState` (same as existing tools)
2. Find `<gradle_root>/<module>/build.gradle.kts` or `build.gradle` (prefer `.kts`)
3. Read file contents as string
4. Regex-extract scalar values: `compileSdk`, `minSdk`, `targetSdk`, `applicationId`, `namespace`
5. Find `buildTypes { ... }` block via brace-balanced extraction → parse type names + `minifyEnabled`/`isMinifyEnabled`, `debuggable`/`isDebuggable`
6. Find `productFlavors { ... }` → parse flavor names + `dimension`

Parsing is regex + brace-balanced string slicing (no Gradle execution). Fields not found return `null` rather than failing.

**Output JSON:**
```json
{
  "module": "app",
  "file": "app/build.gradle.kts",
  "compile_sdk": 35,
  "min_sdk": 24,
  "target_sdk": 35,
  "application_id": "com.example.app",
  "namespace": "com.example.app",
  "build_types": [
    { "name": "debug", "minify_enabled": false, "debuggable": true },
    { "name": "release", "minify_enabled": true, "debuggable": false }
  ],
  "product_flavors": [
    { "name": "free", "dimension": "tier" },
    { "name": "paid", "dimension": "tier" }
  ]
}
```

**Error cases:**
- Module directory not found → `"Module 'app' not found under <gradle_root>"`
- No build.gradle(.kts) found → `"No build.gradle(.kts) found in app/"`

---

## Testing Strategy

Each service module ships with an inline `#[cfg(test)]` module:
- `crash_inspector`: unit tests on raw message parsing (stack frames, caused-by chains, empty buffer)
- `app_inspector`: unit tests on `ps` output parsing and `proc/status` VmRSS parsing; ADB integration skipped in unit tests
- `build_inspector`: unit tests on Gradle DSL snippets covering KTS/Groovy variants, `buildTypes`, `productFlavors`, missing fields

MCP handlers are thin (no logic) so they do not require separate tests beyond type-checking.
