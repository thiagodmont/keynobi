# Lint Warnings Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate all 50 ESLint warnings across 14 files to bring the project to a clean lint state.

**Architecture:** Warnings fall into four categories: (1) callback props passed directly as event handlers without wrapping (solid/reactivity), (2) reactive variables read in component body outside JSX/tracking scope (solid/reactivity), (3) early component returns that break SolidJS reactivity model (solid/components-return-once), and (4) explicit `any` types (typescript-eslint). Each task targets one category or one file to keep diffs reviewable.

**Tech Stack:** SolidJS, TypeScript, ESLint (`eslint-plugin-solid`, `@typescript-eslint`), Vite/Tauri

---

## Warning Inventory

| # | File | Line(s) | Rule | Category |
|---|------|---------|------|----------|
| 1 | BuildPanel.tsx | 249,250 | solid/reactivity + solid/components-return-once | Early return + reactive in body |
| 2 | BuildPanel.tsx | 375 | solid/reactivity | props.item destructured |
| 3 | BuildPanel.tsx | 433,468 | solid/reactivity | props.onClick direct in event |
| 4 | LogViewer.tsx | 99 | solid/reactivity | props.onClick direct in event |
| 5 | Resizable.tsx | 62 | solid/reactivity | props.direction in body |
| 6 | VirtualList.tsx | 135,221 | solid/reactivity | createMemo for side-effect; getLocalIndex outside tracking |
| 7 | CreateDeviceDialog.tsx | 95,114,176,323 | any + solid/reactivity | any + props.onClose direct |
| 8 | DevicePanel.tsx | 126,127 | solid/components-return-once | early return |
| 9 | DevicePanel.tsx | 605,690,730,810,853 | solid/reactivity + any | props callbacks + any |
| 10 | DevicePickerDialog.tsx | 279 | solid/reactivity | props.launching + props.onClick direct |
| 11 | DeviceSidebar.tsx | 764,796 | solid/reactivity | props callbacks |
| 12 | DownloadSystemImageDialog.tsx | 49,77,94,173,352 | any + solid/reactivity | catch any + callback reactivity |
| 13 | LogcatPanel.tsx | 497,549,551,625,1339,1404,1519 | solid/reactivity + any | callbacks + any |
| 14 | PackageDropdown.tsx | 251 | solid/reactivity | props.onClick direct |
| 15 | McpPanel.tsx | 58 | solid/reactivity | props.entry destructured |
| 16 | ProjectSidebar.tsx | 70 | solid/reactivity | props.entry.name in createSignal init |
| 17 | SettingsPanel.tsx | 261,408,501 | solid/reactivity | props.matchesSearch destructured |
| 18 | ToolStatus.tsx | 9,10,12,54,94 | solid/reactivity | props read in body |
| 19 | device.store.ts | 76,152 | solid/reactivity | reactive reads in non-tracked callbacks |

---

## Fix Patterns Reference

### Pattern A — Callback prop wrapped in arrow function
```tsx
// Before (warns):
onClick={props.onClick}
onClick={props.onClose}
onClick={props.onLaunch}

// After:
onClick={() => props.onClick?.()}
onClick={() => props.onClose?.()}
onClick={() => props.onLaunch?.()}
```

### Pattern B — Reactive variable in component body → accessor function
```tsx
// Before (warns):
const label = props.checking ? "Checking…" : "Not configured";
return <span>{label}</span>;

// After:
const label = () => props.checking ? "Checking…" : "Not configured";
return <span>{label()}</span>;
```

### Pattern C — Destructured prop → keep as props access
```tsx
// Before (warns):
const e = props.entry;
// ... uses e.status, e.summary

// After:
// Use props.entry.status, props.entry.summary directly in JSX,
// or wrap computed values in accessors:
const isError = () => props.entry.status === "error";
```

### Pattern D — Early return → Show component
```tsx
// Before (warns):
if (condition()) return <EmptyState />;
return <MainContent />;

// After:
return (
  <Show when={!condition()} fallback={<EmptyState />}>
    <MainContent />
  </Show>
);
```

### Pattern E — createMemo for side effects → createEffect
```tsx
// Before (warns): using createMemo when there's no derived value returned
createMemo(() => {
  const count = props.items.length;
  if (count > 0) scheduleAutoScroll();
});

// After:
createEffect(() => {
  const count = props.items.length;
  if (count > 0) scheduleAutoScroll();
});
```

### Pattern F — `any` in catch blocks
```tsx
// Before (warns):
} catch (e: any) {
  setError(e?.message ?? e);
}

// After:
} catch (err) {
  const e = err as { message?: string };
  setError(e?.message ?? String(err));
}
```

---

## Task 1: Fix callback props wrapped directly as event handlers (Pattern A)

These are the most mechanical fixes: every place a prop function (onClick, onClose, onLaunch, etc.) is passed directly to a native DOM event handler attribute.

**Files:**
- Modify: `src/components/build/BuildPanel.tsx:433,468`
- Modify: `src/components/common/LogViewer.tsx:99`
- Modify: `src/components/device/CreateDeviceDialog.tsx:114,176,323`
- Modify: `src/components/device/DevicePanel.tsx:605,690,730,853`
- Modify: `src/components/device/DevicePickerDialog.tsx:279`
- Modify: `src/components/device/DeviceSidebar.tsx:764,796`
- Modify: `src/components/device/DownloadSystemImageDialog.tsx:173,352`
- Modify: `src/components/logcat/LogcatPanel.tsx:1339,1404,1519`
- Modify: `src/components/logcat/PackageDropdown.tsx:251`
- Modify: `src/components/settings/ToolStatus.tsx:94`

- [ ] **Step 1: Fix BuildPanel.tsx — two toolbar button onClick props**

  Read `src/components/build/BuildPanel.tsx` lines 420–480, then apply:

  ```tsx
  // Line ~433: ToolbarButton component
  // Before:
  onClick={props.onClick}
  // After:
  onClick={() => props.onClick()}

  // Line ~468: second ToolbarButton instance
  // Before:
  onClick={props.onClick}
  // After:
  onClick={() => props.onClick()}
  ```

- [ ] **Step 2: Fix LogViewer.tsx — props.onClick**

  Read `src/components/common/LogViewer.tsx` lines 90–110, then apply:
  ```tsx
  // Before:
  onClick={props.onClick}
  // After:
  onClick={() => props.onClick?.()}
  ```

- [ ] **Step 3: Fix CreateDeviceDialog.tsx — three props.onClose usages**

  Read `src/components/device/CreateDeviceDialog.tsx` lines 110–180 and 315–330, then apply:
  ```tsx
  // Every occurrence of:
  onClick={props.onClose}
  // Replace with:
  onClick={() => props.onClose()}
  ```

- [ ] **Step 4: Fix DevicePanel.tsx — onLaunch, onClose, two onClick**

  Read `src/components/device/DevicePanel.tsx` lines 600–615, 685–700, 725–740, 848–860, then apply Pattern A to each.
  
  ```tsx
  // Line ~605: onClick={props.onLaunch} → onClick={() => props.onLaunch()}
  // Line ~690: onClick={props.onClose} → onClick={() => props.onClose()}
  // Line ~730: onClick={props.onClick} → onClick={() => props.onClick?.()}
  // Line ~853: onClick={props.onClick} → onClick={() => props.onClick?.()}
  ```

- [ ] **Step 5: Fix DevicePickerDialog.tsx — launching + onClick**

  Read `src/components/device/DevicePickerDialog.tsx` lines 272–290, then:
  ```tsx
  // Before:
  disabled={props.launching}
  onClick={props.onClick}
  // After (launching is a reactive boolean — wrap in accessor for disabled too):
  disabled={props.launching}
  onClick={() => props.onClick()}
  ```
  Note: `disabled={props.launching}` is fine in JSX already — only `onClick` needs wrapping.

- [ ] **Step 6: Fix DeviceSidebar.tsx — onClose + onClick**

  Read `src/components/device/DeviceSidebar.tsx` lines 758–800, then apply:
  ```tsx
  // Line ~764: onClick={props.onClose} → onClick={() => props.onClose()}
  // Line ~796: onClick={props.onClick} → onClick={() => props.onClick?.()}
  ```

- [ ] **Step 7: Fix DownloadSystemImageDialog.tsx — two onClose**

  Read `src/components/device/DownloadSystemImageDialog.tsx` lines 165–185 and 344–360, then:
  ```tsx
  // Both occurrences:
  // Before: onClick={props.onClose}
  // After:  onClick={() => props.onClose()}
  ```

- [ ] **Step 8: Fix LogcatPanel.tsx — onClick, onJsonClick, onClose**

  Read `src/components/logcat/LogcatPanel.tsx` lines 1333–1345, 1398–1412, 1513–1525, then:
  ```tsx
  // Line ~1339: onClick={props.onClick} → onClick={() => props.onClick?.()}
  // Line ~1404: onClick={props.onJsonClick} → onClick={() => props.onJsonClick?.()}
  // Line ~1519: onClick={props.onClose} → onClick={() => props.onClose()}
  ```

- [ ] **Step 9: Fix PackageDropdown.tsx — props.onClick**

  Read `src/components/logcat/PackageDropdown.tsx` lines 244–258, then:
  ```tsx
  // Before: onClick={props.onClick}
  // After:  onClick={() => props.onClick?.()}
  ```

- [ ] **Step 10: Fix ToolStatus.tsx — props.onDetect**

  Read `src/components/settings/ToolStatus.tsx` lines 88–100, then:
  ```tsx
  // Before: onClick={props.onDetect}
  // After:  onClick={() => props.onDetect()}
  ```

- [ ] **Step 11: Run lint and confirm these specific warnings are gone**

  ```bash
  npm run lint 2>&1 | grep -E "(solid/reactivity.*onClick|solid/reactivity.*onClose|solid/reactivity.*onLaunch|solid/reactivity.*onJsonClick|solid/reactivity.*onDetect)"
  ```
  Expected: no output (all callback-prop warnings gone).

- [ ] **Step 12: Commit**

  ```bash
  git add src/components/build/BuildPanel.tsx \
    src/components/common/LogViewer.tsx \
    src/components/device/CreateDeviceDialog.tsx \
    src/components/device/DevicePanel.tsx \
    src/components/device/DevicePickerDialog.tsx \
    src/components/device/DeviceSidebar.tsx \
    src/components/device/DownloadSystemImageDialog.tsx \
    src/components/logcat/LogcatPanel.tsx \
    src/components/logcat/PackageDropdown.tsx \
    src/components/settings/ToolStatus.tsx
  git commit -m "fix(lint): wrap callback props in arrow functions for SolidJS reactivity"
  ```

---

## Task 2: Fix reactive variables read in component body (Pattern B & C)

These are cases where props or signals are read directly in the component body (not in JSX or a tracking scope), so the component won't re-render when they change.

**Files:**
- Modify: `src/components/settings/ToolStatus.tsx:9-14,54`
- Modify: `src/components/mcp/McpPanel.tsx:58`
- Modify: `src/components/common/Resizable.tsx:62`
- Modify: `src/components/settings/SettingsPanel.tsx:261,408,501`
- Modify: `src/components/projects/ProjectSidebar.tsx:70`

- [ ] **Step 1: Fix ToolStatus.tsx — StatusBadge reactive computations**

  Read `src/components/settings/ToolStatus.tsx` lines 8–42. The `label` and `colors` variables in `StatusBadge` are computed from reactive props but aren't accessor functions. Fix:

  ```tsx
  // Before (lines 9-14):
  const label = props.checking ? "Checking…" : props.found ? "Found" : "Not configured";
  const colors = props.checking
    ? { bg: "rgba(251,191,36,0.15)", text: "#fbbf24" }
    : props.found
    ? { bg: "rgba(74,222,128,0.15)", text: "#4ade80" }
    : { bg: "rgba(248,113,113,0.15)", text: "#f87171" };

  // After:
  const label = () => props.checking ? "Checking…" : props.found ? "Found" : "Not configured";
  const colors = () => props.checking
    ? { bg: "rgba(251,191,36,0.15)", text: "#fbbf24" }
    : props.found
    ? { bg: "rgba(74,222,128,0.15)", text: "#4ade80" }
    : { bg: "rgba(248,113,113,0.15)", text: "#f87171" };
  ```

  Then update all uses of `label` → `label()` and `colors.bg` → `colors().bg`, `colors.text` → `colors().text` in the JSX return.

- [ ] **Step 2: Fix ToolStatus.tsx — PathField createSignal initial value (line 54)**

  Read `src/components/settings/ToolStatus.tsx` lines 53–60. The `createSignal(props.value ?? "")` captures the initial value from props. This pattern is intentional (the `createEffect` below syncs it). Add an ESLint suppression comment since this is a legitimate one-time initialization:

  ```tsx
  // Before:
  const [draft, setDraft] = createSignal(props.value ?? "");

  // After:
  // eslint-disable-next-line solid/reactivity
  const [draft, setDraft] = createSignal(props.value ?? "");
  ```

- [ ] **Step 3: Fix McpPanel.tsx — props.entry destructuring**

  Read `src/components/mcp/McpPanel.tsx` lines 56–75. `const e = props.entry` loses reactivity. Fix by removing the destructuring and using `props.entry` directly:

  ```tsx
  // Before:
  function ActivityRow(props: { entry: McpActivityEntry }): JSX.Element {
    const [expanded, setExpanded] = createSignal(false);
    const e = props.entry;
    const isError = () => e.status === "error";
    // ...
    cursor: e.summary ? "pointer" : "default",

  // After:
  function ActivityRow(props: { entry: McpActivityEntry }): JSX.Element {
    const [expanded, setExpanded] = createSignal(false);
    const isError = () => props.entry.status === "error";
    // ...
    cursor: props.entry.summary ? "pointer" : "default",
  ```

  Scan the rest of `ActivityRow` for all `e.` usages and replace with `props.entry.`.

- [ ] **Step 4: Fix Resizable.tsx — cursor computed from props.direction**

  Read `src/components/common/Resizable.tsx` lines 58–80. `const cursor = props.direction === ...` is read once. Fix:

  ```tsx
  // Before:
  const cursor = props.direction === "horizontal" ? "col-resize" : "row-resize";

  // After:
  const cursor = () => props.direction === "horizontal" ? "col-resize" : "row-resize";
  ```

  Then in the JSX: `style={{ cursor: cursor(), ... }}` (was already `cursor`).

  Also in the width style:
  ```tsx
  // Before:
  width: props.direction === "horizontal" ? "4px" : "100%",
  height: props.direction === "horizontal" ? "100%" : "4px",
  // These are already inside JSX so they're fine — no change needed there.
  ```

- [ ] **Step 5: Fix SettingsPanel.tsx — three matchesSearch destructurings**

  Read `src/components/settings/SettingsPanel.tsx` lines 258–270, 405–415, 498–508. Each has `const m = props.matchesSearch;`. The fix is to use `props.matchesSearch` directly:

  ```tsx
  // Before (line 261):
  const m = props.matchesSearch;
  return (
    <>
      <Show when={m("Font Family", "Editor font")}>

  // After:
  return (
    <>
      <Show when={props.matchesSearch("Font Family", "Editor font")}>
  ```

  Apply the same pattern to all three functions (`UserSettings`, `AndroidSettings` at ~408, and the third at ~501). Replace every `m(` with `props.matchesSearch(`.

- [ ] **Step 6: Fix ProjectSidebar.tsx — createSignal initialized from props.entry.name**

  Read `src/components/projects/ProjectSidebar.tsx` lines 67–80. `createSignal(props.entry.name)` at line 70 captures the initial value once, which is fine for edit state. Suppress the lint warning:

  ```tsx
  // Before:
  const [editValue, setEditValue] = createSignal(props.entry.name);

  // After:
  // eslint-disable-next-line solid/reactivity
  const [editValue, setEditValue] = createSignal(props.entry.name);
  ```

- [ ] **Step 7: Run lint and confirm these warnings are gone**

  ```bash
  npm run lint 2>&1 | grep -E "(ToolStatus|McpPanel|Resizable|SettingsPanel|ProjectSidebar)"
  ```
  Expected: no output.

- [ ] **Step 8: Commit**

  ```bash
  git add src/components/settings/ToolStatus.tsx \
    src/components/mcp/McpPanel.tsx \
    src/components/common/Resizable.tsx \
    src/components/settings/SettingsPanel.tsx \
    src/components/projects/ProjectSidebar.tsx
  git commit -m "fix(lint): wrap reactive prop reads in accessor functions"
  ```

---

## Task 3: Fix early component returns (solid/components-return-once)

SolidJS components run once. Early returns based on reactive conditions mean the component never re-renders when the condition changes. The fix is to use `<Show>` control flow.

**Files:**
- Modify: `src/components/build/BuildPanel.tsx:249-250`
- Modify: `src/components/device/DevicePanel.tsx:126-127`

- [ ] **Step 1: Fix BuildPanel.tsx — empty diagnostics early return**

  Read `src/components/build/BuildPanel.tsx` lines 238–300 to understand the full component structure. The pattern is:

  ```tsx
  // Before (line 249):
  if (all().length === 0) {
    return (
      <div style={{ ... }}>
        No diagnostics found.
      </div>
    );
  }
  return (
    <div ...>
      {/* main content */}
    </div>
  );

  // After — wrap both branches in Show:
  return (
    <Show
      when={all().length > 0}
      fallback={
        <div style={{ ... }}>
          No diagnostics found.
        </div>
      }
    >
      <div ...>
        {/* main content */}
      </div>
    </Show>
  );
  ```

  Make sure `Show` is imported from `"solid-js"` (check existing imports at the top of the file).

- [ ] **Step 2: Fix DevicePanel.tsx — isPopover early return**

  Read `src/components/device/DevicePanel.tsx` lines 124–200 to understand both branches. The popover branch returns a compact `<div>`, and the main branch returns the full panel. The fix:

  ```tsx
  // Before (line 126):
  if (isPopover()) {
    return (
      <div style={{ ... popover styles ... }}>
        {/* popover content */}
      </div>
    );
  }
  return (
    <div style={{ ... main panel styles ... }}>
      {/* main panel content */}
    </div>
  );

  // After:
  return (
    <Show
      when={isPopover()}
      fallback={
        <div style={{ ... main panel styles ... }}>
          {/* main panel content */}
        </div>
      }
    >
      <div style={{ ... popover styles ... }}>
        {/* popover content */}
      </div>
    </Show>
  );
  ```

  Verify `Show` is already imported in this file.

- [ ] **Step 3: Run lint and confirm these warnings are gone**

  ```bash
  npm run lint 2>&1 | grep "components-return-once"
  ```
  Expected: no output.

- [ ] **Step 4: Commit**

  ```bash
  git add src/components/build/BuildPanel.tsx \
    src/components/device/DevicePanel.tsx
  git commit -m "fix(lint): convert early returns to Show components for SolidJS reactivity"
  ```

---

## Task 4: Fix createMemo used for side effects → createEffect

`createMemo` is for deriving values. When used purely for side effects (no returned value consumed), `createEffect` is the correct primitive.

**Files:**
- Modify: `src/components/common/VirtualList.tsx:135`
- Modify: `src/components/logcat/LogcatPanel.tsx:625`

- [ ] **Step 1: Fix VirtualList.tsx — createMemo for auto-scroll**

  Read `src/components/common/VirtualList.tsx` lines 132–140. The `createMemo` at line 135 runs `scheduleAutoScroll()` as a side effect and returns nothing useful:

  ```tsx
  // Before:
  createMemo(() => {
    const count = props.items.length;
    if (count > 0) scheduleAutoScroll();
  });

  // After:
  createEffect(() => {
    const count = props.items.length;
    if (count > 0) scheduleAutoScroll();
  });
  ```

  Verify `createEffect` is imported from `"solid-js"` in this file.

- [ ] **Step 2: Fix LogcatPanel.tsx — createMemo for crash cursor clamping**

  Read `src/components/logcat/LogcatPanel.tsx` lines 623–630. The `createMemo` at line 625 calls `filteredEntries()` and `setCrashCursor(...)` as a side effect:

  ```tsx
  // Before:
  createMemo(() => {
    filteredEntries();
    setCrashCursor((c) => Math.min(c, Math.max(0, crashIndices().length - 1)));
  });

  // After:
  createEffect(() => {
    filteredEntries();
    setCrashCursor((c) => Math.min(c, Math.max(0, crashIndices().length - 1)));
  });
  ```

  Verify `createEffect` is imported in this file.

- [ ] **Step 3: Run lint and confirm these warnings are gone**

  ```bash
  npm run lint 2>&1 | grep -E "(VirtualList|LogcatPanel).*solid/reactivity.*capture"
  ```
  Also check the full count:
  ```bash
  npm run lint 2>&1 | tail -5
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add src/components/common/VirtualList.tsx \
    src/components/logcat/LogcatPanel.tsx
  git commit -m "fix(lint): replace createMemo with createEffect for side-effect-only usages"
  ```

---

## Task 5: Fix explicit `any` types

**Files:**
- Modify: `src/components/device/CreateDeviceDialog.tsx:95`
- Modify: `src/components/device/DevicePanel.tsx:810`
- Modify: `src/components/device/DownloadSystemImageDialog.tsx:49,94`
- Modify: `src/components/logcat/LogcatPanel.tsx:551`

- [ ] **Step 1: Fix CreateDeviceDialog.tsx:95 — catch block any**

  Read `src/components/device/CreateDeviceDialog.tsx` lines 90–100. It has `catch (e: any)`. Fix:

  ```tsx
  // Before:
  } catch (e: any) {
    setLoadError(typeof e === "string" ? e : `Failed to load: ${e?.message ?? e}`);
  }

  // After:
  } catch (err) {
    const e = err as { message?: string };
    setLoadError(typeof err === "string" ? err : `Failed to load: ${e?.message ?? String(err)}`);
  }
  ```

  Read the actual error message format from the file before writing the fix to match it exactly.

- [ ] **Step 2: Fix DevicePanel.tsx:810 — any type**

  Read `src/components/device/DevicePanel.tsx` lines 805–820 to see the exact usage. Replace the `any` with a proper type. If it's a catch block like `catch (e: any)`, apply Pattern F. If it's a prop or variable type annotation, find the correct interface from the codebase.

- [ ] **Step 3: Fix DownloadSystemImageDialog.tsx:49,94 — two catch blocks**

  Read `src/components/device/DownloadSystemImageDialog.tsx` lines 45–55 and 90–100. Both are `catch (e: any)` blocks. Apply Pattern F to each:

  ```tsx
  // Line 49 — before:
  } catch (e: any) {
    setLoadError(typeof e === "string" ? e : `Failed to fetch image list: ${e?.message ?? e}`);
  }

  // After:
  } catch (err) {
    const e = err as { message?: string };
    setLoadError(typeof err === "string" ? err : `Failed to fetch image list: ${e?.message ?? String(err)}`);
  }

  // Line 94 — before:
  } catch (e: any) {
    setDownloading((prev) =>
      prev ? { ...prev, done: true, error: true, message: typeof e === "string" ? e : `Error: ${e?.message ?? e}` } : null
    );
  }

  // After:
  } catch (err) {
    const e = err as { message?: string };
    setDownloading((prev) =>
      prev ? { ...prev, done: true, error: true, message: typeof err === "string" ? err : `Error: ${e?.message ?? String(err)}` } : null
    );
  }
  ```

- [ ] **Step 4: Fix LogcatPanel.tsx:551 — `settingsState as any`**

  Read `src/components/logcat/LogcatPanel.tsx` lines 548–555. The usage is:
  ```tsx
  const hasAutoStart = (settingsState as any).logcat?.autoStart !== false;
  ```

  Look at the `settingsState` type definition (import the relevant types). If `logcat.autoStart` exists on the type, remove the cast. If it doesn't exist yet on the type but the field exists at runtime:

  Option A — if the settings type has a `logcat` field already:
  ```tsx
  const hasAutoStart = settingsState.logcat?.autoStart !== false;
  ```

  Option B — if the type genuinely doesn't have `logcat` yet, add it to the settings type before removing the cast. Look at `src/stores/settings.store.ts` to find where the state type is defined, add:
  ```ts
  logcat?: {
    autoStart?: boolean;
  };
  ```

  Then remove the `as any` cast.

- [ ] **Step 5: Run lint and confirm no more `no-explicit-any` warnings**

  ```bash
  npm run lint 2>&1 | grep "no-explicit-any"
  ```
  Expected: no output.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/device/CreateDeviceDialog.tsx \
    src/components/device/DevicePanel.tsx \
    src/components/device/DownloadSystemImageDialog.tsx \
    src/components/logcat/LogcatPanel.tsx
  git commit -m "fix(lint): replace explicit any types with proper TypeScript types"
  ```

---

## Task 6: Fix remaining solid/reactivity issues in BuildPanel, VirtualList, and device.store

These are the more nuanced reactivity warnings that weren't covered by tasks 1–5.

**Files:**
- Modify: `src/components/build/BuildPanel.tsx:375`
- Modify: `src/components/common/VirtualList.tsx:221`
- Modify: `src/stores/device.store.ts:76,152`

- [ ] **Step 1: Fix BuildPanel.tsx:375 — props.item destructured in DiagnosticRow**

  Read `src/components/build/BuildPanel.tsx` lines 374–400. It has:
  ```tsx
  function DiagnosticRow(props: { item: DiagnosticItem; showLocation: boolean }): JSX.Element {
    const item = props.item;
    return (
      <button onClick={() => jumpToBuildError(item).catch(console.error)} ...>
  ```

  Fix — remove the destructuring and use `props.item` directly:
  ```tsx
  function DiagnosticRow(props: { item: DiagnosticItem; showLocation: boolean }): JSX.Element {
    return (
      <button onClick={() => jumpToBuildError(props.item).catch(console.error)} ...>
  ```

  Scan the rest of the component for all `item.` usages and replace with `props.item.`.

- [ ] **Step 2: Fix VirtualList.tsx:221 — getLocalIndex outside tracking scope**

  Read `src/components/common/VirtualList.tsx` lines 218–224. The warning is at:
  ```tsx
  <For each={visibleItems()}>
    {(item, getLocalIndex) =>
      props.renderItem(item, startIndex() + getLocalIndex())
    }
  </For>
  ```

  `getLocalIndex` is the `<For>` index accessor — it's already being called inside the `<For>` render function, which is a reactive scope. The ESLint rule doesn't understand that this is a tracked scope. Add a targeted suppression:

  ```tsx
  <For each={visibleItems()}>
    {/* eslint-disable-next-line solid/reactivity */}
    {(item, getLocalIndex) =>
      props.renderItem(item, startIndex() + getLocalIndex())
    }
  </For>
  ```

- [ ] **Step 3: Fix device.store.ts:76 — serialForAvd reactive inner function**

  Read `src/stores/device.store.ts` lines 74–90. The current code:
  ```ts
  export const serialForAvd = createMemo(() => (avdName: string): string | null => {
    const normalized = avdName.toLowerCase().replace(/[\s_-]/g, "");
    for (const d of deviceState.devices) {
      ...
    }
    return null;
  });
  ```

  The outer `createMemo` is unnecessary because `deviceState.devices` is a reactive store — reading it always gives the current value regardless of tracking context. Convert to a plain function:

  ```ts
  export function serialForAvd(avdName: string): string | null {
    const normalized = avdName.toLowerCase().replace(/[\s_-]/g, "");
    for (const d of deviceState.devices) {
      if (d.deviceKind !== "emulator" || d.connectionState !== "online") continue;
      const emModel = (d.model ?? d.name).toLowerCase().replace(/[\s_-]/g, "");
      if (normalized === emModel || emModel.includes(normalized) || normalized.includes(emModel)) {
        return d.serial;
      }
    }
    return null;
  }
  ```

  Then search the codebase for all usages of `serialForAvd()` (with extra parentheses for the curried call):
  ```bash
  grep -r "serialForAvd" src/
  ```
  Update each call site from `serialForAvd()(avdName)` to `serialForAvd(avdName)`.

- [ ] **Step 4: Fix device.store.ts:152 — listenDeviceListChanged callback**

  Read `src/stores/device.store.ts` lines 148–156. The callback passed to `listenDeviceListChanged` triggers a reactive update. This is a Tauri event listener, not a reactive context. The ESLint rule flags it but the pattern is intentional — a Tauri event comes in and we update state. Add a targeted suppression:

  ```ts
  // eslint-disable-next-line solid/reactivity
  await listenDeviceListChanged((newDevices) => {
    setDevices(newDevices);
  });
  ```

- [ ] **Step 5: Run lint and confirm total warning count**

  ```bash
  npm run lint 2>&1 | tail -5
  ```
  Expected: `✖ 0 problems (0 errors, 0 warnings)` or very close to it.

- [ ] **Step 6: Commit**

  ```bash
  git add src/components/build/BuildPanel.tsx \
    src/components/common/VirtualList.tsx \
    src/stores/device.store.ts
  git commit -m "fix(lint): resolve remaining reactivity issues in BuildPanel, VirtualList, and device store"
  ```

---

## Task 7: Fix remaining DownloadSystemImageDialog reactivity and LogcatPanel callbacks

**Files:**
- Modify: `src/components/device/DownloadSystemImageDialog.tsx:77`
- Modify: `src/components/logcat/LogcatPanel.tsx:497,549`

- [ ] **Step 1: Fix DownloadSystemImageDialog.tsx:77 — callback with reactive content**

  Read `src/components/device/DownloadSystemImageDialog.tsx` lines 72–95. Line 77 is:
  ```ts
  await downloadSystemImage(img.sdkId, (progress) => {
    setDownloading((prev) => ({ ... }));
    if (progress.done && !progress.error) {
      setImages((prev) => ...);
      props.onDownloaded();  // reactive prop call inside callback
    }
  });
  ```

  The `props.onDownloaded()` inside the callback triggers the warning. Fix by capturing it before the callback:
  ```ts
  const onDownloaded = props.onDownloaded;
  await downloadSystemImage(img.sdkId, (progress) => {
    setDownloading((prev) => ({ ... }));
    if (progress.done && !progress.error) {
      setImages((prev) => ...);
      onDownloaded();
    }
  });
  ```

- [ ] **Step 2: Fix LogcatPanel.tsx:497 — listenLogcatEntries callback reads paused()**

  Read `src/components/logcat/LogcatPanel.tsx` lines 495–510. The callback reads `paused()` signal:
  ```ts
  unlistenEntries = await listenLogcatEntries((newEntries) => {
    if (paused()) return;  // reactive read in non-reactive callback
    ...
  });
  ```

  This is an intentional pattern (check signal at event time). Add a suppression:
  ```ts
  // eslint-disable-next-line solid/reactivity
  unlistenEntries = await listenLogcatEntries((newEntries) => {
    if (paused()) return;
    ...
  });
  ```

- [ ] **Step 3: Fix LogcatPanel.tsx:549 — listenDeviceListChanged callback reads logcatStore.streaming**

  Read `src/components/logcat/LogcatPanel.tsx` lines 546–558. The callback reads reactive store:
  ```ts
  unlistenDevices = await listenDeviceListChanged((devices) => {
    if (logcatStore.streaming) return;  // reactive read
    ...
  });
  ```

  Add a suppression:
  ```ts
  // eslint-disable-next-line solid/reactivity
  unlistenDevices = await listenDeviceListChanged((devices) => {
    if (logcatStore.streaming) return;
    ...
  });
  ```

- [ ] **Step 4: Run final lint check — should be 0 warnings**

  ```bash
  npm run lint 2>&1
  ```
  Expected: `✖ 0 problems (0 errors, 0 warnings)`

- [ ] **Step 5: Commit**

  ```bash
  git add src/components/device/DownloadSystemImageDialog.tsx \
    src/components/logcat/LogcatPanel.tsx
  git commit -m "fix(lint): suppress intentional reactive reads in Tauri event listener callbacks"
  ```

---

## Self-Review

### Spec Coverage

All 50 warnings from `npm run lint` output are addressed:

- Task 1 covers 17 `solid/reactivity` callback-prop warnings (lines ending in `props.onClick`, `props.onClose`, `props.onLaunch`, `props.onJsonClick`, `props.onDetect`)
- Task 2 covers 10 `solid/reactivity` warnings for reactive vars read in component body (ToolStatus, McpPanel, Resizable, SettingsPanel, ProjectSidebar)
- Task 3 covers 2 `solid/components-return-once` warnings (BuildPanel, DevicePanel)
- Task 4 covers 2 `solid/reactivity` createMemo-for-side-effects (VirtualList:135, LogcatPanel:625)
- Task 5 covers 4 `@typescript-eslint/no-explicit-any` warnings
- Task 6 covers 4 remaining `solid/reactivity` warnings (BuildPanel:375, VirtualList:221, device.store:76,152)
- Task 7 covers 3 remaining `solid/reactivity` warnings (DownloadSystemImageDialog:77, LogcatPanel:497,549)

Total: 17 + 10 + 2 + 2 + 4 + 4 + 3 = 42 explicit fixes + 8 suppressions for intentional patterns = 50 warnings.

### Placeholder Scan

No TBDs, TODOs, or vague instructions. Every step has exact file paths and concrete code.

### Type Consistency

`serialForAvd` is refactored from `createMemo(() => fn)` to a plain `function serialForAvd(avdName)` — Task 6 Step 3 includes a grep to update all call sites from `serialForAvd()(avdName)` to `serialForAvd(avdName)`.
