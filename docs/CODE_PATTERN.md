# Code Patterns

Concrete implementation rules for Keynobi. `BEST_PRACTICES.md` explains why these rules exist; `DOMAIN_PATTERNS.md` adds per-domain invariants.

Update this file when a cross-cutting implementation pattern changes.

---

## Repository Layout

- `src/` - SolidJS frontend.
- `src/components/ui/` - shared design-system primitives, exported from `@/components/ui`.
- `src/components/{domain}/` - domain UI panels and presentational pieces.
- `src/stores/` - Solid stores and state actions.
- `src/services/` - frontend orchestration across stores, IPC, and UI flows.
- `src/lib/tauri-api.ts` - typed wrappers for Tauri `invoke` calls.
- `src/lib/` - pure frontend utilities.
- `src/test/` - Vitest setup, typed factories, and mock backend.
- `src/bindings/` - generated TypeScript bindings; do not edit manually.
- `src-tauri/src/commands/` - thin Tauri command handlers.
- `src-tauri/src/services/` - Rust business logic.
- `src-tauri/src/models/` - Rust IPC models exported with `ts-rs`.
- `src-tauri/src/utils/` - shared Rust utilities.
- `src-tauri/benches/` - Criterion benchmarks.
- `e2e/` - Playwright web-mode integration tests.
- `scripts/` - release, versioning, metrics, and packaging scripts.

---

## Rust Patterns

### State

Each domain owns its own state struct. Avoid unrelated fields in the same Mutex because it creates unnecessary contention.

Implement `Default` by delegating to `new()` when a state type has construction logic.

### Mutex Discipline

Hold locks for the shortest possible time:

```rust
let root = {
    let fs = fs_state.0.lock().await;
    fs.gradle_root.as_ref().or(fs.project_root.as_ref())
        .ok_or("No project open")?
        .clone()
};

let result = do_io(&root)?;
```

Do not hold a Mutex across I/O, process execution, event emission, or `await`.

### Commands

Tauri commands:

- Accept IPC-shaped inputs.
- Validate paths and other untrusted inputs.
- Clone required state and drop locks.
- Delegate business logic to `services/`.
- Convert errors at the boundary with `.map_err(|e| e.to_string())`.

No business logic belongs in command handlers.

### Path Security

Any command or tool that accepts a path must validate it against the effective project root.

The effective root is `gradle_root` when available, otherwise `project_root`. Canonicalize both root and target, then check the canonical target is within the canonical root. For relative paths, prefer shared validators such as `utils::path::validate_within_root`.

Never use raw `path.starts_with(root)` for security.

MCP tools may have narrower validators, such as APK paths being restricted to build outputs.

### Models and Bindings

Every Rust type crossing IPC derives:

```rust
#[derive(Debug, Serialize, Deserialize, Clone, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
```

After changing exported models, run:

```bash
npm run generate:bindings
```

Commit regenerated files under `src/bindings/`.

### Errors and Logging

- Use `thiserror` for structured service errors where it improves clarity.
- Do not use `unwrap()` in production Rust.
- Use `tracing` macros for logs.
- GUI log filtering uses `KEYNOBI_LOG`; headless MCP uses `RUST_LOG`.

---

## Frontend Patterns

### Stores

Each logical domain has a `{domain}.store.ts` file that exports:

- The reactive state object.
- Named action functions.
- Reset helpers for tests when state is mutable across tests.

Components read stores but do not mutate them directly. Use `produce` for array splices and nested updates; use direct path setters for scalar updates.

Do not destructure Solid stores. Access properties through the store object so fine-grained reactivity remains intact.

### Services

Frontend services coordinate user-visible flows across IPC calls, events, stores, and dialogs. Examples: build/run/deploy, project open, logcat start/stop.

Services may dynamically import UI pieces to avoid cycles, but reusable UI state still belongs in stores.

### Components

- `components/ui/` contains reusable primitives with CSS Modules and `var(--*)` theme tokens.
- `components/layout/` contains shell layout.
- `components/{domain}/` contains domain-specific UI.
- `components/common/` is for legacy or broadly shared non-design-system components; prefer `components/ui/` for new primitives.

Components should delegate side effects to services or store actions. Components may use Tauri plugin APIs directly only for UI-specific operations such as dialogs or window controls.

### SolidJS Reactivity

- Use `createMemo` for derived values used in render paths or multiple places.
- Call signals/memos in JSX: `{label()}`, not `{label}`.
- Register `onCleanup` for timers, subscriptions, and listeners.
- Guard async flows where stale responses can arrive out of order.

### Design System

Use shared UI primitives from `@/components/ui`.

- `Button` for labeled actions.
- `IconButton` for compact toolbar controls.
- `Dropdown` / `MenuItem` for option menus; mark destructive items with `destructive: true`.
- `Tooltip` for icon-only controls when practical.
- Semantic CSS tokens over hardcoded colors.
- `Panel`, `Tabs`, `Toolbar`, and form controls before inventing local variants.

---

## IPC and Events

### Typed IPC

All `invoke` calls belong in `src/lib/tauri-api.ts`. Components and services import typed wrappers from there.

```typescript
export async function getBuildStatus(): Promise<BuildStatus> {
  return invoke<BuildStatus>("get_build_status");
}
```

### Commands, Events, and Channels

- Commands are request/response.
- Events are lifecycle notifications.
- Channels are for high-frequency streams such as build output.

Always call event `unlisten()` in cleanup.

---

## Actions and Keybindings

Every keyboard shortcut that should appear in the command palette must be registered with `registerKeyAndAction()` in `App.tsx`.

Use `registerAction()` for command-palette actions without shortcuts.

Do not use bare `registerKeybinding()` for app commands unless the shortcut is intentionally hidden from the command palette.

---

## Testing Patterns

### Vitest

- Tests live next to the code they cover.
- Mock Tauri APIs through `src/test/setup.ts` and `src/test/mock-backend/`.
- Use factories from `src/test/factories/` for IPC-shaped data.
- Reset mutable stores in `beforeEach`.
- Test behavior and state transitions, not internal implementation details.

### Playwright

- E2E tests live under `e2e/`.
- Web mode uses `vite-plugin-tauri-mock.ts`.
- Tests may call `window.__e2e__.invoke(...)` and `window.__e2e__.triggerEvent(...)` for IPC contract checks.
- Per-test startup settings go in `window.__keynobi_e2e_settings_overrides` before `page.goto("/")`.

### Rust

- Unit tests live in `#[cfg(test)]` modules near the service code.
- Use `tempfile::TempDir` for filesystem fixtures.
- Command tests should focus on validation and boundary behavior.

### Verification Gate

For design-system work, shared panel styling, broad UI refactors, or token/color changes, run:

```bash
npm test && npm run typescript:check && npm run lint
```

For Rust changes, match the relevant CI commands in `CONTRIBUTING.md`.

---

## Naming

| Scope | Pattern | Example |
|-------|---------|---------|
| Rust modules | `snake_case.rs` | `fs_manager.rs` |
| TypeScript modules | `kebab-case.ts` | `file-utils.ts` |
| Stores | `{domain}.store.ts` | `build.store.ts` |
| Store tests | `{domain}.store.test.ts` | `build.store.test.ts` |
| Components | `PascalCase.tsx` | `DevicePanel.tsx` |
| Component tests | `{name}.test.tsx` | `DevicePanel.test.tsx` |
| Services | `{domain}.service.ts` | `project.service.ts` |
| Generated bindings | Do not create manually | `BuildStatus.ts` |
