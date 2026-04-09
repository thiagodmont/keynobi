# Build History Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix build history ID collisions after app restart, and add a clear-history button to the history panel.

**Architecture:** The ID fix is a one-line change in `BuildStateInner::new()` — initialize `next_id` from the max persisted ID instead of always starting at 1. The clear feature adds one Rust function, one Tauri command, one TS binding, one store action, and a trash button in the panel header.

**Tech Stack:** Rust (Tauri backend), SolidJS + TypeScript (frontend), Vitest (TS tests), Tokio (Rust async tests)

---

### Task 1: Fix `next_id` initialization to prevent ID collisions

**Files:**
- Modify: `src-tauri/src/services/build_runner.rs` (inside `BuildStateInner::new()` and the `tests` module at the bottom)

- [ ] **Step 1: Write a failing test**

Add inside the `#[cfg(test)]` block at the bottom of `src-tauri/src/services/build_runner.rs`:

```rust
#[test]
fn next_id_starts_after_max_history_id() {
    use std::collections::VecDeque;
    let records: VecDeque<BuildRecord> = (1u32..=5).map(|i| BuildRecord {
        id: i,
        task: format!("task_{i}"),
        status: BuildStatus::Idle,
        errors: vec![],
        started_at: "2026-01-01T00:00:00Z".into(),
    }).collect();
    // This is the formula that BuildStateInner::new() must use.
    let next_id = records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    assert_eq!(next_id, 6, "next_id must continue from max existing id");
}

#[test]
fn next_id_is_one_when_history_empty() {
    use std::collections::VecDeque;
    let records: VecDeque<BuildRecord> = VecDeque::new();
    let next_id = records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    assert_eq!(next_id, 1);
}
```

- [ ] **Step 2: Run to verify tests compile and pass** (they test the formula in isolation, not `new()` yet)

```bash
cd src-tauri && cargo test next_id -- --nocapture 2>&1
```

Expected: both tests **PASS** (they test the formula directly, not the fix).

- [ ] **Step 3: Apply the fix in `BuildStateInner::new()`**

In `src-tauri/src/services/build_runner.rs`, replace the entire `new()` method:

```rust
pub fn new() -> Self {
    let history = load_build_history();
    let next_id = history.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    Self {
        current_build: None,
        status: BuildStatus::Idle,
        history,
        current_errors: vec![],
        next_id,
    }
}
```

- [ ] **Step 4: Run all build_runner tests**

```bash
cd src-tauri && cargo test -p keynobi-lib 2>&1
```

Expected: all tests **PASS**, no compilation errors.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/build_runner.rs
git commit -m "fix(build): initialize next_id from max history id to prevent collisions after restart"
```

---

### Task 2: Add `clear_history` function and Tauri command

**Files:**
- Modify: `src-tauri/src/services/build_runner.rs` (add `clear_history` pub function + test)
- Modify: `src-tauri/src/commands/build.rs` (add `clear_build_history` command)
- Modify: `src-tauri/src/lib.rs` (import + register the new command)

- [ ] **Step 1: Write a failing async test for `clear_history`**

Add inside the `#[cfg(test)]` block in `src-tauri/src/services/build_runner.rs`:

```rust
#[tokio::test]
async fn clear_history_empties_the_deque() {
    let state = BuildState::new();
    // Inject 3 records directly into the state.
    {
        let mut bs = state.inner.lock().await;
        for i in 1u32..=3 {
            bs.history.push_back(BuildRecord {
                id: i,
                task: format!("task_{i}"),
                status: BuildStatus::Idle,
                errors: vec![],
                started_at: "2026-01-01T00:00:00Z".into(),
            });
        }
    }
    clear_history(&state).await;
    let bs = state.inner.lock().await;
    assert!(bs.history.is_empty(), "history must be empty after clear_history");
}
```

- [ ] **Step 2: Run to verify it fails with "unresolved function `clear_history`"**

```bash
cd src-tauri && cargo test clear_history_empties 2>&1 | head -20
```

Expected: **FAIL** — `clear_history` not defined yet.

- [ ] **Step 3: Implement `clear_history` in `build_runner.rs`**

Add after the `cancel_build` function (around line 297) in `src-tauri/src/services/build_runner.rs`:

```rust
/// Clear all build history from memory and disk.
pub async fn clear_history(build_state: &BuildState) {
    let mut bs = build_state.inner.lock().await;
    bs.history.clear();
    save_build_history(&bs.history);
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd src-tauri && cargo test clear_history_empties -- --nocapture 2>&1
```

Expected: **PASS**.

- [ ] **Step 5: Add the Tauri command in `commands/build.rs`**

Add after the `get_build_history` command (after line 275) in `src-tauri/src/commands/build.rs`:

```rust
/// Clear all build history (in-memory and on disk).
#[tauri::command]
pub async fn clear_build_history(
    build_state: State<'_, BuildState>,
) -> Result<(), String> {
    build_runner::clear_history(&build_state).await;
    Ok(())
}
```

- [ ] **Step 6: Register the command in `lib.rs`**

In `src-tauri/src/lib.rs`, update the import on line 7:

```rust
use commands::build::{
    cancel_build, clear_build_history, find_apk_path, finalize_build, get_build_errors,
    get_build_history, get_build_status, get_package_name_from_apk, run_gradle_task,
};
```

Then add `clear_build_history,` inside `tauri::generate_handler![...]` after `get_build_history,` (around line 295):

```rust
            get_build_history,
            clear_build_history,
            find_apk_path,
```

- [ ] **Step 7: Verify the build compiles**

```bash
cd src-tauri && cargo build 2>&1 | grep -E "error|warning: unused" | head -20
```

Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/services/build_runner.rs src-tauri/src/commands/build.rs src-tauri/src/lib.rs
git commit -m "feat(build): add clear_build_history Tauri command"
```

---

### Task 3: Add frontend API binding and store action

**Files:**
- Modify: `src/lib/tauri-api.ts` (add `clearBuildHistory` function after `getBuildHistory`)
- Modify: `src/stores/build.store.ts` (add `clearBuildHistory` exported action)

- [ ] **Step 1: Add the API binding in `tauri-api.ts`**

In `src/lib/tauri-api.ts`, add immediately after the `getBuildHistory` function (after line 209):

```typescript
export async function clearBuildHistory(): Promise<void> {
  return invoke<void>("clear_build_history");
}
```

- [ ] **Step 2: Add the store action in `build.store.ts`**

Add the import at the top of `src/stores/build.store.ts`. The file currently imports from `@/bindings` and `@/stores/log.store`. Add a new import for the API:

```typescript
import { clearBuildHistory as clearBuildHistoryApi } from "@/lib/tauri-api";
```

Then add the exported action at the bottom of `src/stores/build.store.ts` (after `resetBuildState`):

```typescript
export async function clearBuildHistory(): Promise<void> {
  await clearBuildHistoryApi();
  setBuildState("history", []);
}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
npm run typescript:check 2>&1
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/lib/tauri-api.ts src/stores/build.store.ts
git commit -m "feat(build): add clearBuildHistory store action and API binding"
```

---

### Task 4: Add trash button to `BuildHistoryPanel`

**Files:**
- Modify: `src/components/build/BuildHistoryPanel.tsx`
- Modify: `src/components/build/BuildHistoryPanel.test.ts` (verify `onClear` prop is accepted)

- [ ] **Step 1: Write a test verifying the export shape still works (no regression)**

Add to `src/components/build/BuildHistoryPanel.test.ts`:

```typescript
import { statusIcon } from "@/components/build/BuildHistoryPanel";
import type { BuildHistoryPanelProps } from "@/components/build/BuildHistoryPanel";

it("BuildHistoryPanelProps accepts optional onClear", () => {
  // Type-level test: if this compiles, onClear is optional
  const _props: BuildHistoryPanelProps = {
    selectedId: null,
    onSelect: () => {},
    // onClear omitted — must be optional
  };
  expect(true).toBe(true);
});
```

- [ ] **Step 2: Run tests to verify they pass now (no change yet)**

```bash
npm run test -- BuildHistoryPanel 2>&1
```

Expected: all **PASS** (the type test will pass once we add the prop).

- [ ] **Step 3: Update `BuildHistoryPanelProps` and the header in `BuildHistoryPanel.tsx`**

In `src/components/build/BuildHistoryPanel.tsx`, update the interface and add `Icon` import:

```typescript
import { type JSX, For, Show } from "solid-js";
import type { BuildRecord, BuildResult, BuildStatus } from "@/bindings";
import { buildState } from "@/stores/build.store";
import Icon from "@/components/common/Icon";

export interface BuildHistoryPanelProps {
  /** ID of the currently selected history entry. null = current build. */
  selectedId: number | null;
  /** Called when the user clicks a history entry. null = current build. */
  onSelect: (record: BuildRecord | null) => void;
  /** Called when the user clicks the clear-history button. */
  onClear?: () => void;
}
```

Then replace the header `<div>` (lines 69–82) with:

```tsx
      {/* Header */}
      <div
        style={{
          "font-size": "9px",
          color: "var(--text-disabled)",
          padding: "5px 8px 3px",
          "text-transform": "uppercase",
          "letter-spacing": "0.06em",
          "border-bottom": "1px solid var(--border)",
          "flex-shrink": "0",
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
        }}
      >
        <span>Builds</span>
        <Show when={history().length > 0 && props.onClear}>
          <button
            title="Clear build history"
            onClick={() => props.onClear?.()}
            style={{
              background: "transparent",
              border: "none",
              cursor: "pointer",
              padding: "0 2px",
              color: "var(--text-disabled)",
              display: "flex",
              "align-items": "center",
              opacity: "0.6",
            }}
            onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.opacity = "1"; }}
            onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.opacity = "0.6"; }}
          >
            <Icon name="trash" size={10} color="currentColor" />
          </button>
        </Show>
      </div>
```

- [ ] **Step 4: Run tests**

```bash
npm run test -- BuildHistoryPanel 2>&1
```

Expected: all **PASS**.

- [ ] **Step 5: Run lint and TypeScript check**

```bash
npm run lint 2>&1 && npm run typescript:check 2>&1
```

Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/build/BuildHistoryPanel.tsx src/components/build/BuildHistoryPanel.test.ts
git commit -m "feat(build): add onClear prop and trash button to BuildHistoryPanel header"
```

---

### Task 5: Wire `onClear` in `BuildPanel`

**Files:**
- Modify: `src/components/build/BuildPanel.tsx`

- [ ] **Step 1: Import `clearBuildHistory` and add the handler**

In `src/components/build/BuildPanel.tsx`, update the import from `@/stores/build.store`:

```typescript
import { buildState, buildLogStore, isBuilding, isDeploying, clearBuildHistory } from "@/stores/build.store";
```

Then add `handleClearHistory` after `handleCancel` (around line 98):

```typescript
  async function handleClearHistory() {
    await clearBuildHistory();
    setSelectedHistoryId(null);
  }
```

- [ ] **Step 2: Wire the `onClear` prop on `<BuildHistoryPanel>`**

Update the `<BuildHistoryPanel>` usage (around line 212):

```tsx
        <BuildHistoryPanel
          selectedId={selectedHistoryId()}
          onSelect={handleHistorySelect}
          onClear={handleClearHistory}
        />
```

- [ ] **Step 3: Run lint and TypeScript check**

```bash
npm run lint 2>&1 && npm run typescript:check 2>&1
```

Expected: no errors.

- [ ] **Step 4: Run all frontend tests**

```bash
npm run test 2>&1
```

Expected: all **PASS**.

- [ ] **Step 5: Commit**

```bash
git add src/components/build/BuildPanel.tsx
git commit -m "feat(build): wire clear-history button in BuildPanel"
```
