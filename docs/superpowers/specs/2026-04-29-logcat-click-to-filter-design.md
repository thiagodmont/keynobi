# Logcat Click-To-Filter Design

## Goal

Let users turn values from the log entry detail panel into query-bar filter pills without typing. Clicking a value in **Entry Detail** opens a floating context menu with **Add as AND** and **Add as OR** actions.

## User Experience

The interaction starts in `LogEntryDetailPanel`. Every visible metadata value is clickable:

- Tag
- Package
- Level
- PID
- TID
- Time
- Message

Clicking one of these values opens a floated menu anchored to the clicked value, visually similar to a right-click context menu. The menu overlays the panel and does not change the detail panel layout. The menu closes when the user chooses an action, clicks outside it, presses Escape, or selects another value.

For the message field, the user can either click without a text selection to filter by the full message or select part of the message text before clicking to filter by the selected text. Selected text takes precedence over the full message.

## Filter Mapping

Clicked values become committed query-bar tokens:

| Entry Detail value | Query token |
| --- | --- |
| Tag | `tag:<value>` |
| Package | `package:<value>` |
| Level | `level:<lowercase-level>` |
| PID | `pid:<number>` |
| TID | `tid:<number>` |
| Time | `time:"<timestamp>"` |
| Full message | `message:"<message>"` |
| Selected message text | `message:"<selected text>"` |

Token values that can contain spaces, quotes, pipes, or other query separators are quoted and escaped before insertion. Numeric values are inserted without quotes. Empty package values are not filterable.

## AND And OR Semantics

The floating menu has two actions:

- **Add as AND** inserts the new token into the current group. It appends to the existing query using the same committed-token convention as the query bar, with `&&` used only when needed for an explicit visual connector.
- **Add as OR** inserts the new token as a new OR group by appending `| <token>`.

If the query is empty, both actions produce the same single-token query because there is no existing condition to combine with.

The resulting query string always ends with a trailing space so the query bar renders the inserted token as a committed pill instead of leaving it as draft text.

## Architecture

`LogEntryDetailPanel` remains a presentational component. It receives an optional callback such as `onAddFilter` and emits `{ token, mode }` when the user chooses a menu action. It does not own or parse the global query string.

`LogcatPanel` continues to own the active query through `updateQuery`. It passes an `onAddFilter` handler down to `LogEntryDetailPanel`, appends the emitted token with AND or OR semantics, and lets the existing query persistence and backend-filter synchronization effects run.

Pure query-building helpers live in `src/lib/logcat-query.ts` so token creation, quoting, and append behavior can be tested outside Solid components. The existing query bar remains the single visible source of truth for active filters.

## Query Parser Changes

`src/lib/logcat-query.ts` adds three token types:

- `pid`
- `tid`
- `time`

`pid` and `tid` match exact numeric values against `entry.pid` and `entry.tid`. `time` matches exact timestamp text against `entry.timestamp`.

These filters are evaluated in the frontend. They are not added to `LogcatFilterSpec` because the Rust backend filter currently has no fields for PID, TID, or exact timestamp, and adding backend support is unnecessary for this UI-focused feature.

## Accessibility And Interaction Details

Clickable values use button semantics where practical so they are keyboard reachable. The menu supports Escape to close and uses normal button items for the AND and OR choices.

The menu is positioned from the clicked element's bounding rectangle and clamped to the viewport. It has a high enough z-index to float above the detail panel but stays scoped to logcat UI styling.

## Error Handling

If a value cannot produce a useful token, the detail panel does not open the menu for that value. This applies to missing package values and empty selected message text. If the selected message text is whitespace-only, the full message fallback is used only when the click target is the message field itself.

## Testing

Frontend tests cover:

- Query parsing and matching for `pid:`, `tid:`, and `time:`.
- Token quoting and escaping for message and timestamp values.
- AND insertion into an empty and non-empty query.
- OR insertion into an empty and non-empty query.
- `LogEntryDetailPanel` opening a floating menu when a filterable value is clicked.
- Menu actions emitting the expected token and mode.
- Message text selection taking precedence over the full message.

No Rust tests or TypeScript binding regeneration are required because the IPC model does not change.

## Documentation Updates

Update `docs/USER_MANUAL.md` under the Logcat section to mention that Entry Detail values can be clicked to create AND/OR filters.

`docs/CODE_PATTERN.md`, `docs/DOMAIN_PATTERNS.md`, and `docs/BEST_PRACTICES.md` do not need updates unless implementation introduces a new reusable query-building pattern beyond this feature.
