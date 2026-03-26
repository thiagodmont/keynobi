# Android IDE -- User Manual

> AI-First Android Development Environment  
> Version 0.1.0 (Phase 2 -- Code Intelligence)

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [IDE Layout](#ide-layout)
3. [File Management](#file-management)
4. [Code Editor](#code-editor)
5. [Code Intelligence (LSP)](#code-intelligence-lsp)
6. [Project-Wide Search](#project-wide-search)
7. [Command Palette](#command-palette)
8. [Symbols & Outline](#symbols--outline)
9. [Navigation](#navigation)
10. [Problems Panel](#problems-panel)
11. [Keyboard Shortcuts Reference](#keyboard-shortcuts-reference)

---

## Getting Started

### Opening a Project

1. Launch the app -- the IDE opens with an empty editor area.
2. Press **Cmd+O** or click the "Open Folder" button in the sidebar to select an Android project directory.
3. The file tree loads in the sidebar, respecting `.gitignore` patterns. Build artifacts (`build/`, `.gradle/`, `.idea/`) are automatically hidden.

### First-Time LSP Setup

On first project open, the IDE checks for the JetBrains Kotlin LSP installation. If not found, a prompt offers to download it (~400MB). The LSP provides code completions, diagnostics, go-to-definition, and other intelligence features. You can skip the download -- the editor still works with syntax highlighting and Tree-sitter-based symbols.

---

## IDE Layout

```
+------------------------------------------------------+
|  Title Bar (drag to move window)                     |
+--------+---------------------------------------------+
|Sidebar |                                             |
|(icons) |         Editor Area (tabs + editor)         |
|        |                                             |
| Files  |                                             |
| Search |---------------------------------------------+
| Outline|         Bottom Panel (Problems/Build/etc)   |
| Git    |         (collapsible)                       |
+--------+---------------------------------------------+
|  Status Bar (LSP status, diagnostics, cursor pos)    |
+------------------------------------------------------+
```

### Sidebar

The left sidebar has four tabs, accessible by clicking the icons:

| Icon | Tab | Description |
|------|-----|-------------|
| Folder | **Explorer** | Project file tree with CRUD operations |
| Search | **Search** | Project-wide text search |
| List | **Outline** | Document symbols for the active file |
| Branch | **Source Control** | Git integration (coming in Phase 6) |

### Bottom Panel

Toggle with **Cmd+J**. Contains tabs for:

- **Problems** -- All diagnostics (errors/warnings) across open files
- **Build** -- Build output (coming in Phase 3)
- **Logcat** -- Android log viewer (coming in Phase 4)
- **Terminal** -- Integrated terminal (coming in Phase 6)

### Status Bar

The bottom bar shows:

- **Project name** (left)
- **LSP status** -- "Kotlin LSP: Starting...", "Ready", "Error" (left)
- **Diagnostic counts** -- Error count (red) and warning count (yellow) (left)
- **Cursor position** -- `Ln X, Col Y` (right)
- **Encoding** -- UTF-8 (right)
- **Language** -- Kotlin, Gradle, XML, etc. (right)

---

## File Management

### File Tree

- **Expand/collapse** directories by clicking or pressing Right/Left arrow keys
- **Open a file** by double-clicking it in the tree
- **Keyboard navigation**: Up/Down arrows to move, Enter to open, Left to collapse, Right to expand

### Context Menu (Right-click)

Right-click a file or directory for:

- **New File** -- Creates a new file (type the name inline)
- **New Folder** -- Creates a new directory
- **Rename** -- Inline rename editor
- **Delete** -- Moves to Trash (not permanent delete)
- **Copy Path** -- Copies the absolute path to clipboard
- **Copy Relative Path** -- Copies the path relative to project root

### Tabs

- Open files appear as tabs above the editor
- A **dot indicator** on a tab means the file has unsaved changes
- **Middle-click** a tab to close it
- Closing a dirty tab prompts: Save / Don't Save / Cancel
- Closing the app window with unsaved changes prompts: Save All / Discard All / Cancel

### Saving

| Action | Shortcut |
|--------|----------|
| Save active file | **Cmd+S** |
| Save all files | **Cmd+Opt+S** |

The editor also saves via the CodeMirror keymap (Cmd+S when the editor is focused).

---

## Code Editor

The editor is powered by CodeMirror 6 with the following features:

### Syntax Highlighting

Supported languages:
- **Kotlin** (`.kt`) -- Keywords, strings, comments, annotations, types
- **Gradle/Kotlin Script** (`.gradle.kts`, `.gradle`) -- Kotlin DSL with Gradle-specific highlighting
- **XML** (`.xml`) -- Android layouts, manifests, drawables
- **JSON** (`.json`) -- Configuration files

### Editor Features

| Feature | Description |
|---------|-------------|
| **Line numbers** | Displayed in the left gutter |
| **Active line highlight** | Subtle highlight on the current line |
| **Bracket matching** | Matching brackets are highlighted |
| **Auto-close brackets** | Typing `(`, `[`, `{`, `"`, `'` auto-inserts the closing pair |
| **Code folding** | Collapse/expand code blocks via gutter arrows |
| **Multiple cursors** | Hold Opt and click to add cursors; Opt+drag for rectangular selection |
| **Find & Replace** | Cmd+F opens the in-file search bar |
| **Selection highlighting** | Other occurrences of selected text are highlighted |
| **Indent on input** | Auto-indentation as you type |

### Editor Keyboard Shortcuts (CodeMirror)

| Action | Shortcut |
|--------|----------|
| Undo | **Cmd+Z** |
| Redo | **Cmd+Shift+Z** |
| Find in file | **Cmd+F** |
| Find and replace | **Cmd+H** |
| Find next | **Cmd+G** |
| Find previous | **Cmd+Shift+G** |
| Select all | **Cmd+A** |
| Indent line | **Tab** |
| Outdent line | **Shift+Tab** |
| Move line up | **Opt+Up** |
| Move line down | **Opt+Down** |
| Copy line down | **Shift+Opt+Down** |
| Toggle line comment | **Cmd+/** |
| Fold code block | **Cmd+Shift+[** |
| Unfold code block | **Cmd+Shift+]** |
| Trigger completion | **Ctrl+Space** |

---

## Code Intelligence (LSP)

When the JetBrains Kotlin LSP is installed and ready, the following features are available for Kotlin and Gradle/Kotlin Script files:

### Completions

- **Automatic**: Completions appear as you type after `.` (member access) or `:` (type annotation)
- **Manual trigger**: Press **Ctrl+Space** to explicitly request completions
- Completions include methods, fields, classes, keywords, and more
- Each item shows its kind (method, field, class, etc.) and detail signature

### Diagnostics

- **Inline squiggles**: Red underlines for errors, yellow for warnings
- **Gutter markers**: Colored dots in the line number gutter
- **Hover**: Move the mouse over a diagnostic to see the error message
- Diagnostics are pulled after each edit (debounced) and after saving

### Hover Information

- **Cmd+hover** or hover with a ~500ms delay over a symbol to see:
  - Type information
  - Documentation (KDoc)
  - Function signatures

### Code Formatting

The LSP supports IntelliJ-style code formatting via `textDocument/formatting`.

### Code Actions & Quick Fixes

- Available on diagnostic ranges (lightbulb icon or **Cmd+.**)
- Includes: auto-import, "Add names to call arguments", "Specify type explicitly", IntelliJ inspections

---

## Project-Wide Search

Open with **Cmd+Shift+F** or click the Search icon in the sidebar.

### Search Features

| Feature | Description |
|---------|-------------|
| **Text search** | Fast ripgrep-based search across all project files |
| **Regex mode** | Toggle the `.*` button for regular expression patterns |
| **Case sensitive** | Toggle the `Aa` button |
| **Whole word** | Toggle the `ab` button |
| **File filter** | Expandable input to filter by pattern (e.g., `*.kt`) |
| **Replace** | Expandable replace input with Replace / Replace All |
| **Streaming results** | Results appear grouped by file as they are found |

### Search Results

- Results are grouped by file with match counts
- Each match shows the line number and highlighted match in context
- **Click a match** to open the file at that exact location
- File groups can be collapsed/expanded by clicking the header

### Performance

- Search respects `.gitignore` and excludes `build/`, `.gradle/`, `.idea/`, `.git/`, `node_modules/`
- Capped at 500 files / 10,000 matches to prevent UI overload
- Results from superseded searches are automatically discarded

---

## Command Palette

The central hub for discovering and executing all IDE actions.

### Modes

| Shortcut | Mode | Description |
|----------|------|-------------|
| **Cmd+P** | File search | Fuzzy search over all project files. Recently opened files appear first. |
| **Cmd+Shift+P** | Command search | Search all registered IDE commands/actions. Shows shortcut if available. |
| **Cmd+Shift+O** | Document symbols | Jump to a symbol (class, function, property) in the current file. |
| **Cmd+T** | Workspace symbols | Search symbols across the entire project (requires LSP). |

### Usage

1. Press the shortcut to open the palette
2. Start typing to filter results
3. Use **Up/Down** arrows to navigate
4. Press **Enter** to select
5. Press **Escape** to close

In file mode, typing `>` switches to command mode automatically.

### Available Commands

All commands shown in the command palette:

| Command | Shortcut | Category |
|---------|----------|----------|
| Toggle Sidebar | Cmd+B | View |
| Toggle Bottom Panel | Cmd+J | View |
| Open Folder | Cmd+O | File |
| Close Active Tab | Cmd+W | File |
| Save Active File | Cmd+S | File |
| Save All Files | Cmd+Opt+S | File |
| Previous Tab | Cmd+Shift+[ | View |
| Next Tab | Cmd+Shift+] | View |
| Search in Project | Cmd+Shift+F | Search |
| Quick Open File | Cmd+P | Navigate |
| Command Palette | Cmd+Shift+P | General |
| Go to Symbol in File | Cmd+Shift+O | Navigate |
| Go to Symbol in Workspace | Cmd+T | Navigate |
| Navigate Back | Cmd+- | Navigate |
| Navigate Forward | Cmd+Shift+- | Navigate |

---

## Symbols & Outline

Click the **Outline** icon (list icon) in the sidebar to see the document structure of the active file.

### Features

- **Hierarchical view**: Classes contain their methods and properties
- **Symbol icons**: Color-coded badges for each symbol kind:
  - **C** (gold) -- Class
  - **I** (teal) -- Interface
  - **f** (purple) -- Function
  - **m** (purple) -- Method
  - **p** (blue) -- Property
  - **F** (blue) -- Field
  - **v** (blue) -- Variable
  - **E** (gold) -- Enum
  - **K** (blue) -- Constant
- **Click to navigate**: Click any symbol to scroll the editor to its location
- **Expand/collapse**: Click the chevron to expand or collapse nested symbols
- **Auto-refresh**: Updates when you switch files or edit the current file (debounced)

### Symbol Source

- When the Kotlin LSP is ready, symbols come from the LSP (most accurate)
- When LSP is not available, Tree-sitter provides instant fallback symbols

---

## Navigation

### Navigation History

The IDE tracks your navigation positions. Every time you jump to a new location (via search result, symbol click, or go-to-definition), your previous position is saved.

| Action | Shortcut |
|--------|----------|
| Navigate back | **Cmd+-** |
| Navigate forward | **Cmd+Shift+-** |

The history stack holds up to 50 entries and deduplicates consecutive identical positions.

### Go-to-definition

When the LSP is ready:
- **Cmd+Click** on a symbol to jump to its definition
- If the definition is in another file, it opens in a new tab

### Find References

- **Shift+F12** on a symbol shows all references (requires LSP)

### Tab Navigation

| Action | Shortcut |
|--------|----------|
| Previous tab | **Cmd+Shift+[** |
| Next tab | **Cmd+Shift+]** |

---

## Problems Panel

The Problems panel (in the bottom panel, toggle with **Cmd+J**) shows all diagnostics across open files.

### Features

- Diagnostics grouped by file, sorted by severity (errors first)
- Each entry shows: severity icon, message, file location (line:col)
- **Click** any entry to jump to that location in the editor
- Badge on the "Problems" tab shows the total count
- Error count (red) and warning count (yellow) also shown in the status bar

---

## Keyboard Shortcuts Reference

### Global Shortcuts

| Shortcut | Action |
|----------|--------|
| **Cmd+O** | Open project folder |
| **Cmd+S** | Save active file |
| **Cmd+Opt+S** | Save all files |
| **Cmd+W** | Close active tab |
| **Cmd+B** | Toggle sidebar |
| **Cmd+J** | Toggle bottom panel |
| **Cmd+P** | Quick open file |
| **Cmd+Shift+P** | Command palette |
| **Cmd+Shift+F** | Search in project |
| **Cmd+Shift+O** | Go to symbol in file |
| **Cmd+T** | Go to symbol in workspace |
| **Cmd+Shift+[** | Previous tab |
| **Cmd+Shift+]** | Next tab |
| **Cmd+-** | Navigate back |
| **Cmd+Shift+-** | Navigate forward |

### Editor Shortcuts

| Shortcut | Action |
|----------|--------|
| **Cmd+Z** | Undo |
| **Cmd+Shift+Z** | Redo |
| **Cmd+F** | Find in file |
| **Cmd+H** | Find and replace in file |
| **Cmd+G** | Find next match |
| **Cmd+Shift+G** | Find previous match |
| **Cmd+A** | Select all |
| **Cmd+/** | Toggle line comment |
| **Ctrl+Space** | Trigger code completion |
| **Tab** | Indent / accept completion |
| **Shift+Tab** | Outdent |
| **Opt+Up** | Move line up |
| **Opt+Down** | Move line down |
| **Shift+Opt+Down** | Copy line down |
| **Cmd+Shift+[** | Fold code block |
| **Cmd+Shift+]** | Unfold code block |
| **Escape** | Close find bar / completion popup |

### File Tree Shortcuts

| Shortcut | Action |
|----------|--------|
| **Up/Down** | Move selection |
| **Right** | Expand directory / move to first child |
| **Left** | Collapse directory / move to parent |
| **Enter** | Open selected file |
| **Delete** | Delete selected item (with confirmation) |

### Search Panel

| Shortcut | Action |
|----------|--------|
| **Enter** | Execute search |
| **Escape** | Clear search |

---

## Supported File Types

| Extension | Language | Syntax Highlighting | LSP Intelligence |
|-----------|----------|--------------------|-----------------| 
| `.kt` | Kotlin | Yes | Yes (completions, diagnostics, navigation) |
| `.gradle.kts` | Gradle/Kotlin Script | Yes | Yes |
| `.gradle` | Gradle/Groovy | Yes (Kotlin fallback) | Partial |
| `.xml` | XML | Yes | No |
| `.json` | JSON | Yes | No |
| Other | Plain text | No | No |

---

## Troubleshooting

### LSP Not Starting

- Check the status bar for the LSP status indicator
- If it shows "Not Installed", use the download prompt or download manually
- The LSP bundles its own JRE -- no external Java installation is needed
- LSP automatically restarts on crash (up to 5 attempts with exponential backoff)

### Slow File Tree Loading

- Large projects (1000+ files) may take a moment to load initially
- `.gitignore` patterns are respected to reduce the tree size
- Build directories (`build/`, `.gradle/`) are automatically excluded

### Search Not Finding Results

- Ensure the search respects your file filter pattern
- Check if the file is in a `.gitignore`-excluded directory
- Binary files and non-UTF-8 files are automatically skipped
