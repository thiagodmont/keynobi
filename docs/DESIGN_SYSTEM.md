# Design System

Keynobi's design system lives in `src/components/ui` and is documented with
Storybook stories beside the primitives. The production app must not import
Storybook files; Storybook imports app CSS and UI primitives only.

## Running Storybook

```bash
npm run storybook
npm run storybook:build
```

Storybook loads `src/styles/global.css`, so examples use the same theme tokens
as the Tauri app. Stories should use real component props and realistic Keynobi
copy, not standalone demo styling that cannot be reused.

## Component Choice

Use existing primitives before adding local component styling:

- `Button`, `IconButton`, `Toolbar`, `ControlStrip`, and `FilterChip` for actions.
- `Input`, `Textarea`, `Select`, `Checkbox`, `Toggle`, `FormField`, and `TagInput` for forms.
- `Badge`, `StatusDot`, `Alert`, `ProgressBar`, `Spinner`, and `EmptyState` for feedback.
- `Panel`, `DockedPanel`, `Popover`, `Dropdown`, `MenuList`, `Tabs`, `MetadataGrid`, and `ScrollArea` for surfaces.
- `Kbd`, `CopyableText`, `Separator`, and `Icon` for supporting UI.

Add a new primitive only when at least two feature areas need the same behavior
or when a local implementation would duplicate accessibility, density, or token
rules already owned by the design system.

### Ownership Map

Use this map before adding local markup or styles in a feature folder.

| Need | Use | Notes |
|------|-----|-------|
| Primary or secondary command | `Button` | Keep `variant="primary"` for the main action in a surface. |
| Compact labeled toolbar action | `Button variant="outline" size="xs"` | Preferred for dense panels such as Logcat. |
| Icon-only toolbar action | `IconButton` + `Icon` | Provide a meaningful `title`; add `Tooltip` when the icon is not obvious. |
| Toolbar or filter band | `ControlStrip` | Use `wrap` when filters may overflow. |
| Toggle filter pill | `FilterChip` | Preserve explicit pressed state. |
| Search or inline edit | `Input size="xs" | "sm"` | Use `mono` for queries, package names, paths, and tags. |
| Longer text input | `Textarea` | Use `mono` for logs, command output, or code-like text. |
| Settings field | `FormField` + form control | Owns label, description, required marker, and error text. |
| Binary setting | `Toggle` or `Checkbox` | Use `Toggle` for on/off modes, `Checkbox` for inclusion choices. |
| Semantic label | `Badge` | Use `size="xs"` and `mono` for dense developer tokens. |
| Health/state marker | `StatusDot` | Pair with nearby text unless the status is already named. |
| Inline feedback | `Alert` | Include action only when recovery is direct. |
| Empty panel | `EmptyState` | Use `density="compact"` in sidebars, menus, and dense panels. |
| Progress | `ProgressBar` or `Spinner` | Use indeterminate only when progress is unknown. |
| Panel surface | `Panel` | Use for framed app sections, not nested page decoration. |
| Bottom/detail readout | `DockedPanel` | Pair with `MetadataGrid` for compact details. |
| Metadata readout | `MetadataGrid` / `MetadataCell` | Use clickable cells only when they perform a clear filter/jump action. |
| Simple option menu | `Dropdown` | Use for static action lists. |
| Custom popover content | `Popover` + `MenuList` | Use for search, rename, section headers, or custom rows. |
| Custom menu rows | `MenuList`, `MenuListItem`, `MenuSectionHeader`, `MenuEmptyState` | Clickable rows should expose an interactive role. |
| Tabs | `Tabs` | Use when switching views inside the same surface. |
| Scrollable area | `ScrollArea` | Use for bounded scroll regions inside panels. |
| High-volume fixed rows | `VirtualList` | Required for large log/build lists. |
| Resizable split | `Resizable` | Keep resize state in the owning feature component/store. |
| Copy affordance | `CopyableText` | Use for IDs, paths, commands, package names, and log values. |
| Keyboard shortcut hint | `Kbd` | Use only for real shortcuts. |
| Divider | `Separator` | Use semantic orientation. |
| App command search | `CommandPalette` | Actions must come from `registerAction` or `registerKeyAndAction`. |
| Confirmation or blocking choice | `DialogHost` / `showDialog` | Keep messages short and action labels explicit. |
| Global transient feedback | `ToastContainer` / `showToast` | Do not use for persistent state or hidden errors. |

## Adoption Checklist

Use this checklist when refactoring a feature folder toward the design system:

- Identify duplicated UI patterns before editing.
- Check the ownership map and existing stories before creating local UI.
- Replace local markup with primitives without changing feature behavior.
- Keep domain orchestration in the feature component/store; primitives stay generic.
- Preserve existing accessibility names, keyboard behavior, and visible states.
- Add or update a primitive story when a new reusable state or composition appears.
- Add or update tests when behavior, keyboard support, state, or accessibility semantics are touched.
- Run `npm test && npm run test:ds` before handoff.

## Adoption Order

Use design-system adoption to reduce real duplication, not to churn stable code.

1. `src/components/logcat` - mostly adopted; use as the reference for dense filters, popovers, metadata, and docked detail panels.
2. `src/components/device` - likely next target for repeated status rows, menus, toolbar actions, and empty states.
3. `src/components/settings` - normalize fields, health/status messaging, and form layout.
4. `src/components/build` - normalize build toolbar actions, progress, history rows, and output panels.
5. `src/components/ui-hierarchy` - adopt cautiously because capture/tree/wireframe interactions are more specialized.
6. `src/components/common` - retire or migrate legacy shared UI only when an active feature refactor needs it.

Stop a refactor when the remaining local code is genuinely domain-specific or
when extracting it would make the behavior harder to understand.

## Story Guidelines

- Put stories next to the primitive or in `src/components/ui`.
- Use `*.stories.tsx`; never import these files from app code.
- Cover common variants, disabled/loading/error states, compact density, and
  keyboard/accessibility semantics.
- Prefer controlled examples with Solid signals for interactive states.
- Keep domain-specific examples realistic, but do not call Tauri APIs from stories.

### Minimum Story Coverage

Every reusable primitive should have Storybook coverage for the states it owns:

- Default/common usage.
- Variants, tones, sizes, or density options.
- Disabled, loading, error, empty, or indeterminate states when supported.
- Keyboard or open/closed states for interactive overlays and menus.
- A realistic Keynobi example using product language and real density.

Grouped overview stories are useful for scanning, but they do not replace
component-level stories beside the primitive.

## Visual Rules

- Use semantic CSS tokens from `src/styles/theme.css`; avoid hardcoded colors.
- Keep dense app controls compact and stable in width/height.
- Use cards only for repeated examples or framed tools. Avoid nested cards.
- Icon-only controls need labels through `title`, `aria-label`, or `Tooltip`.
- Toggle controls should expose explicit pressed/checked states.

## Accessibility Rules

- Interactive controls must expose an accessible name.
- Custom clickable elements must expose an interactive role and keyboard support.
- Toggle-like controls must preserve explicit false states such as
  `aria-pressed="false"` or `aria-checked="false"`.
- Icon-only controls must have `title`, `aria-label`, visible label text, or a
  tooltip attached to a named control.
- Menus and menu-like popovers should use `role="menu"` / `role="menuitem"` or a
  more specific role when appropriate.
- Errors that need immediate attention should use visible text and semantic
  roles such as `role="alert"` where the primitive supports it.
- Do not rely on color alone for status. Pair color with text, icon shape, badge
  label, or nearby context.

## Primitive Acceptance Criteria

Before adding a new primitive or expanding an existing one:

- There is a concrete reuse need across at least two consumers, or one consumer
  would otherwise duplicate non-trivial accessibility/state/styling behavior.
- The public props describe behavior and state, not one-off visual tweaks.
- Styling uses CSS Modules and semantic theme tokens.
- The primitive has focused Vitest coverage for behavior and accessibility risk.
- The primitive has Storybook coverage following the minimum coverage standard.
- Project docs are updated if the primitive establishes a new pattern.

## Verification

For design-system work, run:

```bash
npm run test:ds
```

`test:ds` runs TypeScript, lint, Storybook build, and Playwright Storybook
smoke/a11y checks. The Storybook Playwright suite uses the static Storybook
build through `playwright.storybook.config.ts` so it does not depend on the
production Tauri app or app E2E server.

CI runs the same command in the `Design System` workflow job.

Run the full frontend test suite when changing shared behavior or component
contracts:

```bash
npm test
```
