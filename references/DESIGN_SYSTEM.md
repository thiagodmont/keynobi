# Design System

Keynobi's design system lives in `src/components/ui` and is documented with Storybook stories beside the primitives. The production app must not import Storybook files; Storybook imports app CSS and UI primitives only.

Keynobi is a dense developer tool with a single dark theme. The design system optimizes for compact, stable, keyboard-friendly controls that stay readable at small sizes.

Update this file when a primitive, token, or accessibility rule changes. When the code does not yet meet a rule, keep the rule and record the gap under [Known Gaps](#known-gaps).

## Running Storybook

```bash
npm run storybook          # dev server on :6006
npm run storybook:build    # static build into storybook-static/
```

Storybook (`.storybook/`) loads `src/styles/global.css`, so examples use the same theme tokens as the Tauri app. Addons: Docs (autodocs by tag) and A11y. Stories should use real component props and realistic Keynobi copy, not standalone demo styling that cannot be reused.

## Primitives

Every primitive has its own folder: `ui/{Name}/{Name}.tsx`, `{Name}.module.css`, `{Name}.stories.tsx`, `{Name}.test.tsx`, and `index.ts`. All are re-exported from `@/components/ui`.

| Group | Primitives |
|-------|------------|
| Actions | `Button`, `IconButton`, `Toolbar`, `ControlStrip`, `FilterChip` |
| Forms | `Input`, `Textarea`, `Select`, `Checkbox`, `Toggle`, `FormField`, `TagInput` |
| Feedback | `Badge`, `StatusDot`, `Alert`, `ProgressBar`, `Spinner`, `EmptyState`, `Toast` (`showToast`, `dismissToast`) |
| Surfaces | `Panel`, `DockedPanel`, `Tabs`, `MetadataGrid`, `ScrollArea`, `Resizable`, `VirtualList` |
| Overlays | `Popover`, `Dropdown`, `MenuList`, `Tooltip`, `Dialog` (`DialogHost`, `showDialog`), `CommandPalette` (`openPalette`, `closePalette`) |
| Supporting | `Kbd`, `CopyableText`, `Separator`, `Icon` |

Add a new primitive only when at least two feature areas need the same behavior, or when a local implementation would duplicate accessibility, density, or token rules the design system already owns.

### Ownership Map

Use this map before adding local markup or styles in a feature folder. **Status** flags primitives with known accessibility gaps; prefer fixing the primitive over working around it locally.

| Need | Use | Notes | Status |
|------|-----|-------|--------|
| Primary or secondary command | `Button` | Keep `variant="primary"` for the main action in a surface. Variants: `primary`, `secondary`, `ghost`, `danger`, `outline`. | Stable |
| Compact labeled toolbar action | `Button variant="outline" size="xs"` | Preferred for dense panels such as Logcat. | Stable |
| Icon inside a labeled action | `Button` + `Icon` | Preferred when a dense action still has visible text, such as Logcat Start, Restart, Copy, and Export. | Stable |
| Icon-only toolbar action | `IconButton` + `Icon` | `title` is required; add `Tooltip` when the icon is not obvious. | Needs work: `aria-pressed` |
| Toolbar or filter band | `ControlStrip` | Use `wrap` when filters may overflow. | Stable |
| Toggle filter pill | `FilterChip` | Always sets `aria-pressed` true/false. | Stable |
| Search or inline edit | `Input size="xs" \| "sm"` | Use `mono` for queries, package names, paths, and tags. | Stable |
| Longer text input | `Textarea` | Use `mono` for logs, command output, or code-like text. | Stable |
| Settings field | `FormField` + form control | Owns label, description, required marker, and error text (`role="alert"`). | Stable |
| Binary setting | `Toggle` or `Checkbox` | `Toggle` (`role="switch"`) for on/off modes, `Checkbox` for inclusion choices. | Stable |
| Semantic label | `Badge` | Use `size="xs"` and `mono` for dense developer tokens. | Stable |
| Health/state marker | `StatusDot` | Pair with nearby text unless the status is already named. | Stable |
| Inline feedback | `Alert` | Include an action only when recovery is direct. | Stable |
| Empty panel | `EmptyState` | Use `density="compact"` in sidebars, menus, and dense panels. | Stable |
| Progress | `ProgressBar` or `Spinner` | `ProgressBar` is indeterminate when `value` is undefined; use that only when progress is unknown. | Stable |
| Panel surface | `Panel` | Use for framed app sections, not nested page decoration. | Stable |
| Bottom/detail readout | `DockedPanel` | Pair with `MetadataGrid` for compact details. | Stable |
| Metadata readout | `MetadataGrid` / `MetadataCell` | Use clickable cells only when they perform a clear filter/jump action. | Stable |
| Simple option menu | `Dropdown` with `items: MenuItem[]` | Use for static action lists; mark destructive items with `destructive: true`. | Needs work: menu roles, focus |
| Custom popover content | `Popover` + `MenuList` | Use for search, rename, section headers, or custom rows. Controlled through `open` / `onOpenChange`. | Needs work: Escape, focus return |
| Custom menu rows | `MenuList`, `MenuListItem`, `MenuSectionHeader`, `MenuEmptyState` | Clickable rows get `role="menuitem"` and Enter/Space. | Needs work: container role |
| Tabs | `Tabs` | Use when switching views inside the same surface. | Needs work: arrow keys |
| Scrollable area | `ScrollArea` | Use for bounded scroll regions inside panels. | Stable |
| High-volume fixed rows | `VirtualList` | Required for large log/build lists. | Stable |
| Resizable split | `Resizable` | Keep resize state in the owning feature component/store. | Stable |
| Copy affordance | `CopyableText` | Use for IDs, paths, commands, package names, and log values. | Stable |
| Keyboard shortcut hint | `Kbd` | Use only for real shortcuts. | Stable |
| Divider | `Separator` | Use semantic orientation. | Stable |
| App command search | `CommandPalette` | Actions must come from `registerAction` or `registerKeyAndAction`. | Stable |
| Confirmation or blocking choice | `DialogHost` / `showDialog` | Keep messages short and action labels explicit; use the `danger` button style for destructive choices. | Needs work: focus trap, Escape |
| Global transient feedback | `ToastContainer` / `showToast` | Do not use for persistent state or hidden errors. | Stable |

## Tokens

Semantic tokens live in `src/styles/theme.css`. Use them for every color, font, and shared size; never hardcode a color in a component.

| Category | Tokens |
|----------|--------|
| Backgrounds | `--bg-primary`, `--bg-secondary`, `--bg-tertiary`, `--bg-quaternary`, `--bg-hover`, `--bg-active`, `--bg-selection` |
| Text | `--text-primary`, `--text-secondary`, `--text-muted`, `--text-disabled` |
| Accent | `--accent`, `--accent-hover`, `--accent-text`, `--accent-bg`, `--accent-fg`, `--accent-border` |
| Semantic | `--error`, `--warning`, `--success`, `--info`, plus `-bg` and `-border` tints |
| Borders | `--border`, `--border-focus` |
| Type | `--font-ui`, `--font-mono`, `--font-size-ui-xs` (10px), `--font-size-ui-sm` (11px), `--font-size-ui` (12px), `--font-size-ui-lg` (14px) |
| Layout | `--titlebar-height`, `--statusbar-height`, `--sidebar-*`, `--tab-height`, `--logcat-row-height` |

Rules:

- Name tokens `--{category}-{role}[-{modifier}]`.
- Use `--accent-text`, not `--accent`, for accent-colored text and links. `--accent` is for fills and borders.
- Derive tints with `color-mix` from base tokens instead of adding new raw colors.
- Stacking order: tooltips and dropdowns 1000, dialogs 9000, toasts 9999. Keep new overlays inside this scale. It is a comment today, not tokens.
- When you need a value the tokens do not cover (spacing, radius, shadow, motion), add a token instead of repeating a raw value in more than one file.

### Contrast

Target WCAG 2.2 AA: 4.5:1 for text, 3:1 for large text, UI component boundaries, and focus indicators. Measured against the main surfaces:

| Foreground | on `--bg-primary` | on `--bg-secondary` | on `--bg-tertiary` | Use for |
|------------|------|------|------|---------|
| `--text-primary` | 10.4 | 9.5 | 8.6 | Body text |
| `--text-secondary` | 7.3 | 6.7 | 6.0 | Secondary text |
| `--text-muted` | 5.4 | 5.0 | **4.45** | Hints; avoid on `--bg-tertiary` |
| `--text-disabled` | 2.4 | 2.2 | 2.0 | Disabled only (exempt) |
| `--accent-text` | 6.0 | 5.5 | 5.0 | Accent text and links |
| `--accent` | **3.7** | **3.4** | **3.0** | Fills, borders, focus ring; not text |
| `--error` / `--warning` / `--success` / `--info` | ≥ 6.0 | ≥ 5.5 | ≥ 4.9 | Status text |

White text on an `--accent` fill is 4.5:1, the minimum. Keep button labels at least 12px.

## Visual Rules

- Keep dense app controls compact and stable in width and height. Sizes are `xs`, `sm`, and `md`; `xs` is for dense toolbars.
- Use cards only for repeated examples or framed tools. Avoid nested cards.
- Static styling lives in CSS Modules. Inline styles are reserved for runtime-derived values such as log level color, row selection background, and viewport-constrained popover coordinates.
- Honor `prefers-reduced-motion` for spinners, pulses, and transitions.

### Icons and Fonts

- `Icon` renders from a built-in inline SVG map (Phosphor Regular style, 256 viewBox, default size 16). There is no icon package dependency. Add a new icon to the map in `Icon.tsx` and to its story.
- Icons are always `aria-hidden`; the control that contains them provides the accessible name.
- An unknown icon `name` falls back to the `file` icon silently. Check the story after adding icons.
- Fonts are system stacks (`--font-ui`, `--font-mono`); no webfonts are bundled.

## Accessibility Rules

- Interactive controls must expose an accessible name.
- Custom clickable elements must expose an interactive role and keyboard support.
- Toggle-like controls must set explicit false states such as `aria-pressed="false"` or `aria-checked="false"`, not omit the attribute.
- Icon-only controls must have `title`, `aria-label`, visible label text, or a tooltip attached to a named control.
- Menus and menu-like popovers should use `role="menu"` / `role="menuitem"` or a more specific role when appropriate.
- Errors that need immediate attention should use visible text and semantic roles such as `role="alert"` where the primitive supports it.
- Do not rely on color alone for status. Pair color with text, icon shape, badge label, or nearby context.
- Every focusable control shows a visible focus indicator with at least 3:1 contrast. Use `:focus-visible` with `--border-focus`. Never remove an outline without a replacement.
- Pointer targets should be at least 24×24 px (WCAG 2.2 SC 2.5.8). Dense `xs` toolbar controls are an accepted exception when spacing keeps neighboring targets from overlapping.

### Keyboard Contract

| Component | Required behavior |
|-----------|-------------------|
| `Button`, `IconButton`, `FilterChip`, `Toggle`, `Checkbox` | Enter/Space activates. |
| `Dropdown`, `MenuList` | Arrow keys move real focus (or `aria-activedescendant`) between items; Enter/Space selects; Escape closes; focus returns to the trigger. Trigger exposes `aria-haspopup` and `aria-expanded`. |
| `Popover` | Escape closes; focus returns to the trigger; clicking outside closes. |
| `Dialog` | Focus moves into the dialog on open and is trapped while it is open; Escape cancels; focus returns to the previously focused element on close. |
| `Tabs` | Left/Right arrows move between tabs; Home/End jump to the first/last tab. |
| `CommandPalette` | Up/Down move through results; Enter runs; Escape closes. |
| `VirtualList` rows | Up/Down move selection when the list has focus. |

## Adoption Checklist

Use this checklist when refactoring a feature folder toward the design system:

- Identify duplicated UI patterns before editing.
- Check the ownership map and existing stories before creating local UI.
- Replace local markup with primitives without changing feature behavior.
- Move static inline styles into a CSS Module that uses tokens.
- Keep domain orchestration in the feature component/store; primitives stay generic.
- Preserve existing accessibility names, keyboard behavior, and visible states.
- Add or update a primitive story when a new reusable state or composition appears.
- Add or update tests when behavior, keyboard support, state, or accessibility semantics are touched.
- Run `npm test && npm run test:ds` before handoff.

## Adoption Order

Use design-system adoption to reduce real duplication, not to churn stable code. Only Logcat uses CSS Modules today; every other feature folder uses inline styles, so adoption also means moving static styles into CSS Modules.

1. `src/components/logcat` - mostly adopted; use as the reference for dense filters, popovers, metadata, and docked detail panels.
2. `src/components/device` - next target: many raw buttons and inline styles in device rows, menus, and AVD dialogs.
3. `src/components/settings` and `src/components/health` - normalize fields, status messaging, and form layout.
4. `src/components/build` - normalize build toolbar actions, progress, history rows, and output panels.
5. `src/components/projects`, `mcp`, `onboarding`, `update`, and `layout` - smaller surfaces; adopt primitives when a feature change touches them.
6. `src/components/ui-hierarchy` - adopt cautiously because capture/tree/wireframe interactions are more specialized.
7. `src/components/common` - retire or migrate legacy shared UI (`LogViewer`, `ErrorBoundary`) only when an active feature refactor needs it.

Stop a refactor when the remaining local code is genuinely domain-specific or when extracting it would make the behavior harder to understand.

For dense feature surfaces such as Logcat, use primitives for control semantics and keep domain-specific row/chip layout in local CSS Modules.

## Story Guidelines

- Put stories next to the primitive (`ui/{Name}/{Name}.stories.tsx`) under the title `Design System/Components/{Name}`.
- Never import stories from app code.
- Prefer controlled examples with Solid signals for interactive states.
- Keep domain-specific examples realistic, but do not call Tauri APIs from stories.

### Minimum Story Coverage

Every reusable primitive should have Storybook coverage for the states it owns:

- Default/common usage.
- Variants, tones, sizes, or density options.
- Disabled, loading, error, empty, or indeterminate states when supported.
- Keyboard or open/closed states for interactive overlays and menus.
- A realistic Keynobi example using product language and real density.

Grouped overview stories (`Design System/Actions`, `Feedback`, `Forms`, `Foundations`, `Surfaces`) are useful for scanning, but they do not replace component-level stories beside the primitive.

## Primitive Acceptance Criteria

Before adding a new primitive or expanding an existing one:

- There is a concrete reuse need across at least two consumers, or one consumer would otherwise duplicate non-trivial accessibility/state/styling behavior.
- The public props describe behavior and state, not one-off visual tweaks.
- Styling uses CSS Modules and semantic theme tokens.
- The primitive meets the accessibility rules and keyboard contract above.
- The primitive has focused Vitest coverage for behavior and accessibility risk.
- The primitive has Storybook coverage following the minimum coverage standard.
- If the primitive is interactive, its story ID is added to `A11Y_STORY_IDS` in `e2e/storybook/design-system.spec.ts`.
- This document is updated if the primitive establishes a new pattern.

## Verification

For design-system work, run:

```bash
npm test && npm run test:ds
```

`test:ds` runs `typescript:check`, `lint`, and `test:storybook`. The Storybook suite (`playwright.storybook.config.ts`, `e2e/storybook/`) builds Storybook, serves `storybook-static` directly on `127.0.0.1:6106`, and then:

- Smoke-renders every `Design System/Components/*` story and fails on page or console errors.
- Runs axe (`@axe-core/playwright`, default rules) on the stories listed in `A11Y_STORY_IDS` and fails on any violation of any impact.

Do not use the app Vite preview server for Storybook tests. CI runs `npm run test:ds` in the `Design System` job.

## Known Gaps

Places where the primitives or features do not yet meet the rules above. Remove an entry when it is fixed.

- `IconButton` and `Toolbar` omit `aria-pressed` when inactive instead of setting `"false"`.
- `Dropdown` has no `role="menu"`/`menuitem`, no `aria-haspopup`/`aria-expanded`, moves a CSS class instead of focus, and does not return focus on close. `MenuList` has no default container role.
- `Popover` and `Dialog` have no Escape handling or focus management; `Dialog` has no focus trap.
- `Tabs` has no arrow-key navigation.
- Only `MetadataGrid` defines `:focus-visible`; `global.css` removes the outline on `input`/`textarea`.
- No `prefers-reduced-motion` handling.
- Primitives hardcode radius, z-index, shadow, and 10/11px font sizes. `Button` hardcodes its danger color, and `FilterChip` references an undefined `--accent-rgb`. There are no spacing, radius, shadow, or motion tokens yet.
- Feature components contain about 200 hardcoded colors in inline styles (worst: `McpPanel`, `StatusBar`, `ProjectSidebar`, `HealthPanel`). No stylelint rule enforces token use.
- `global.css` colors links with `--accent` (3.7:1).
- axe runs on 8 curated stories, not every component story.
- `theme.css` still contains legacy editor/LSP highlight styles that belong elsewhere or can be removed.
