# Domain Patterns

Domain-specific rules for Keynobi. These supplement `CODE_PATTERN.md`; keep this file focused on invariants that are not obvious from local code.

Update this file when a domain workflow, boundary, or safety rule changes.

---

## Build

### Backend State

`BuildState` owns build status, in-flight process IDs, bounded history, accumulated errors, and the bounded raw build log. Keep process lifecycle state in the build domain; do not spread Gradle process ownership into UI code.

Key caps:

- Build history: `MAX_HISTORY`.
- Raw build output retained for MCP/history: `MAX_BUILD_LOG`.
- Persisted build logs: rotated by build history and age/size policy.

### Frontend Flow

`build.service.ts` owns build orchestration:

```text
runBuild()
  -> runGradleTask
  -> wait for build:complete
  -> finalizeBuild
```

`runAndDeploy()` builds first, then installs and launches against the resolved online device. Always clear `deployPhase` in `finally`.

### Invariants

- Build output streams through a Tauri Channel.
- Build lifecycle completion uses events.
- `runBuild()` resolves only after completion/cancellation state is known.
- Cancellation must clear pending build-completion waiters and process IDs.
- Parsed build errors are persisted through `finalizeBuild`.

---

## Device Management

### Backend State

`DeviceState` owns connected devices, selected serial, and the polling guard. Device polling runs as a detached task and emits `device:list_changed` when the visible list changes.

### AVDs

AVD lifecycle commands go through Android SDK tools and return the refreshed AVD list when practical so the frontend updates in one round-trip.

### Frontend Modes

`DevicePanel` supports two contexts:

- `mode="panel"`: full device management surface.
- `mode="popover"`: compact status-bar picker with a manage-devices path.

Device-picking flows must validate that `selectedSerial` is still online before using it.

---

## Logcat

### Pipeline

Logcat is a backend-first streaming pipeline:

```text
adb logcat
  -> raw line ingestion
  -> processor chain
  -> bounded LogStore
  -> backend filter
  -> batched IPC emit
  -> frontend render/filter refinements
```

Backend processing owns package resolution, crash detection, JSON detection, category classification, stats, and ring-buffer storage.

### Filtering

Handle high-volume and simple filters in Rust before crossing IPC. Frontend-only filters are limited to cases that require browser state or JS-only semantics, such as age filters, negation, and regex.

Filtered Logcat context expansion should fetch adjacent raw rows from the backend ring buffer by anchor entry id. Do not clear the active filter or replace the filtered backfill just to show surrounding context; merge bounded context rows into the visible frontend list and mark them as expanded context.

### Frontend Boundaries

Keep `LogcatPanel.tsx` as composition/orchestration. Domain logic lives in focused helpers:

- Query parsing/matching compatibility surface: `lib/logcat-query.ts`.
- Query variable helpers: `lib/logcat-query-variables.ts`.
- Backend/frontend filter boundary helpers: `lib/logcat-frontend-only-tokens.ts`.
- Stack-frame parsing for Studio jumps: `lib/logcat-stack-frame.ts`.
- Current project package resolution: `lib/logcat-mine-package.ts`.
- Backend filter conversion: `lib/logcat-filter-spec.ts`.
- Autocomplete data: `lib/logcat-suggestions.ts`.
- UI entry storage and UI cap: `stores/logcat.store.ts`.
- Async request ordering: `services/logcat.service.ts`.
- Query state/debounce orchestration: `logcat-query-controller.ts`.
- Query interaction: `QueryBar.tsx`, `QueryBarParts.tsx`, `querybar-query-state.ts`, `querybar-styles.ts`.
- Suggestion runtime: `logcat-suggestion-runtime.ts`.
- Saved filter menu presets and pieces: `saved-filter-presets.ts`, `SavedFilterMenuParts.tsx`.
- Entry text formatting for copy/export: `logcat-entry-format.ts`.
- Presentational pieces: `LogcatToolbar.tsx`, `SavedFilterMenu.tsx`, `LogcatRows.tsx`, `LogcatFilterControls.tsx`, `LogcatJsonDetailPanel.tsx`.

Presentational components should not call Tauri IPC except for narrow row actions, such as opening a stack frame in Android Studio.
Reusable Logcat UI chrome should prefer design-system primitives before adding Logcat-local style helpers:

- `Button variant="outline" size="xs"` for compact toolbar/menu actions.
- `FilterChip` for quick filter pills and compact toggle chips.
- `Popover` for controlled custom dropdown surfaces that need search, rename, or custom row content.
- `MenuList` primitives for custom popover rows, section headers, and empty states.
- `Input` compact/mono modes for dense filter, search, and inline-edit fields.
- `Badge size="xs"` for dense row badges and small state markers.
- `DockedPanel` and `MetadataGrid` for log entry detail/readout panels.
- `ControlStrip` for Logcat toolbar and filter bands.
- `EmptyState density="compact"` for dense panel empty/waiting states.

### IPC Type

`ProcessedEntry.json_body` is `Option<String>`, not a JSON value. The frontend parses it only when the user opens JSON details.

---

## Layout Viewer and UI Automation

### Capture

The layout viewer and MCP UI automation share the same UI Automator capture path. Keep capture logic centralized in `services/ui_hierarchy.rs` / `services/ui_automation.rs` so GUI and MCP behavior stay consistent.

### Bounds and Caps

The hierarchy parser must keep explicit caps for:

- Raw XML size.
- Node count.
- Tree depth.
- Attribute string length.
- Interactive row count.
- UI automation match count.
- Tap/swipe coordinate bounds.

### Paths

Tree paths use the same index convention across the layout viewer and MCP tools. If UI presentation collapses boilerplate nodes, preserve enough mapping to reveal or act on the real underlying node.

### Screen Hash

Use `screenHash` to protect automation from stale UI state. Tools that act on UI coordinates or tree paths should support `expect_screen_hash` when stale-screen safety matters.

---

## MCP

### Tool Definitions

MCP tools live in `services/mcp_server.rs` and are declared with `rmcp` `#[tool_router]` / `#[tool]` macros. Do not describe MCP tools as generated from the frontend action registry.

### Validation

Validate every external string before acting:

- Gradle task names.
- Package names.
- Device serials.
- APK paths.
- Deep links.
- Runtime permissions.
- UI key names.
- UI automation coordinates and tree paths.

Execution failures should usually return `CallToolResult::error(...)`. Protocol or schema failures should return `McpError`.

### Output Shape

Prefer compact structured output:

- Use `CallToolResult::structured(json!(...))` for machine-readable results.
- Use `CallToolResult::success(...)` for human-readable text.
- Keep payloads bounded and omit noisy command logs unless the tool is explicitly for diagnostics.

### Modes

- GUI mode uses shared Tauri state from `AndroidMcpServer::from_app_handle`.
- Headless mode uses `AndroidMcpServer::new_headless` and initializes state from `--project`, last active project, or current directory.
- Headless MCP logs to stderr; stdout is reserved for MCP JSON-RPC.

### Activity

GUI MCP lifecycle and tool activity should update the MCP activity store through the existing activity logging path. Keep activity entries bounded.
