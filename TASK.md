# Android IDE — Task List

> Organized by phase. Check off tasks as completed.
> Reference: PLAN.md for full architecture details.

---

## Phase 1 — Foundation

### Phase Goal

> A developer launches the app, opens an Android project folder, browses the file tree, opens multiple Kotlin and Gradle files in tabs with syntax highlighting, edits them, and saves changes. The app feels responsive and looks like a modern IDE with a dark theme.

**Acceptance criteria — the phase is DONE when all of these are true:**
1. `npm run tauri dev` launches a macOS window with title bar, sidebar, editor area, bottom panel zone, and status bar.
2. User clicks "Open Folder" (or uses Cmd+O), picks a real Android project directory, and the file tree renders within 1 second for a project with 500+ files.
3. `.gitignore` patterns are respected — `build/`, `.gradle/`, `.idea/` folders do not appear in the tree.
4. Double-clicking a `.kt` file opens it in a new tab with Kotlin syntax highlighting (keywords, strings, comments, annotations are colored).
5. Double-clicking a `.gradle.kts` file opens it with Gradle/Kotlin script highlighting.
6. Opening the same file twice reuses the existing tab instead of creating a duplicate.
7. User can have 5+ files open in tabs, switch between them, and each tab restores cursor position and scroll position.
8. Editing a file marks its tab as dirty (dot indicator). Cmd+S saves it to disk and clears the dirty indicator.
9. Closing a dirty tab prompts "Save changes?" with Save / Discard / Cancel options.
10. The file tree updates reactively when files are created, renamed, or deleted externally (e.g., from another terminal).
11. The whole UI uses a cohesive dark theme similar to VS Code Dark+ or Cursor's default theme.

---

### 1.1 — Project Scaffolding

> Set up the monorepo structure: a Tauri 2.0 Rust backend + SolidJS TypeScript frontend, wired together with Vite. After this section, `npm run tauri dev` opens an empty window.

- [ ] **Install prerequisites** — Verify Rust stable toolchain (`rustup`), Node.js 20+ (`node -v`), and Xcode Command Line Tools (`xcode-select --install`). These are required by Tauri 2.0 to compile the Rust backend and link against macOS frameworks.

- [ ] **Install Tauri CLI** — Run `cargo install tauri-cli` (provides the `cargo tauri` command used for dev server, builds, and sidecar management).

- [ ] **Initialize the Tauri 2.0 + SolidJS project** — Use `npm create tauri-app@latest` and select SolidJS + TypeScript template. This scaffolds:
  - `src/` — frontend SolidJS code
  - `src-tauri/` — Rust backend with `Cargo.toml`, `tauri.conf.json`, `src/main.rs`
  - `package.json` with Vite + SolidJS dev dependencies

  If the template doesn't exist for SolidJS, manually scaffold: `npm create vite@latest` with SolidJS template, then `cd src-tauri && cargo tauri init` to add Tauri.

- [ ] **Configure `tauri.conf.json`** — Set these values:
  - `productName`: `"Android IDE"`
  - `identifier`: `"com.androidide.app"` (reverse-DNS, used for macOS bundle ID)
  - `windows[0].title`: `"Android IDE"`
  - `windows[0].width`: `1400`, `windows[0].height`: `900` (sensible default for IDE)
  - `windows[0].decorations`: `false` (we render our own title bar for a cleaner look)
  - `windows[0].transparent`: `false` (keep simple for now)
  - `bundle.icon`: update with app icon paths

- [ ] **Configure Tauri 2.0 capabilities** — Create `src-tauri/capabilities/default.json` with permissions the app needs:
  - `core:default` — basic Tauri IPC
  - `fs:default` — file system access (read/write project files)
  - `fs:allow-read` and `fs:allow-write` scoped to user-selected directories
  - `dialog:default` — native open-folder dialog
  - `shell:default` — spawn child processes (needed in Phase 2 for Gradle, but set up now)

  Without these, Tauri 2.0's security model will block IPC calls at runtime.

- [ ] **Configure TypeScript** — Set `tsconfig.json`:
  - `strict: true` (catch type errors early)
  - `paths`: `{ "@/*": ["./src/*"] }` for clean imports
  - `target`: `"ES2021"` (WKWebView supports modern JS)

- [ ] **Configure Vite** — In `vite.config.ts`:
  - Add `vite-plugin-solid` for SolidJS JSX transform
  - Add path alias: `resolve.alias` mapping `@` to `./src`
  - Set `server.strictPort: true` (Tauri expects the dev server on a specific port)

- [ ] **Set up code quality tools** — These ensure consistent code style across the project:
  - Frontend: `npm install -D eslint prettier eslint-plugin-solid` + config files
  - Backend: Ensure `rustfmt.toml` exists (use default Rust formatting) and `clippy` is configured in CI (catches common Rust mistakes)

- [ ] **Initialize git repository** — `git init`, create `.gitignore` with: `node_modules/`, `target/`, `dist/`, `.DS_Store`, `*.log`. First commit with the scaffolded project structure.

- [ ] **Verify dev loop works** — Run `npm run tauri dev`. It should:
  1. Start Vite dev server (frontend hot-reload)
  2. Compile the Rust backend (`cargo build`)
  3. Open a native macOS window with an empty SolidJS page
  4. Hot-reload the frontend when you edit `.tsx` files (Rust changes require restart)

**Section done when:** `npm run tauri dev` opens a native macOS window showing a SolidJS "Hello World" page.

---

### 1.2 — App Shell Layout

> Build the main window skeleton that all future panels will live inside. This is the visual frame: title bar, sidebar, editor area, bottom panel, and status bar. Use CSS Grid for the top-level layout with resizable splitters between zones. After this section, the app looks like an IDE shell — empty, but with the right structure.

**Visual layout:**
```
┌──────────────────────────────────────────────────────┐
│  TitleBar (custom macOS, traffic lights + app name)  │
├────────┬─────────────────────────────────────────────┤
│Sidebar │                                             │
│(icons) │         Editor Area (tabs + editor)         │
│        │                                             │
│ Files  │                                             │
│ Search │─────────────────────────────────────────────│
│ Git    │         Bottom Panel (Build/Logcat/etc)     │
│        │         (collapsible, resizable height)     │
├────────┴─────────────────────────────────────────────┤
│  StatusBar (build status | device | variant | LSP)   │
└──────────────────────────────────────────────────────┘
```

- [ ] **Create root `App.tsx` layout** — Use CSS Grid with named areas:
  - Columns: `sidebar (48px fixed)` | `splitter (4px)` | `main (1fr)`
  - Rows for `main`: `editor-area (1fr)` | `splitter (4px)` | `bottom-panel (var, min 100px, default 250px)`
  - Wrap everything in `titlebar (top)` and `statusbar (bottom)` rows
  - Create a `ui.store.ts` with signals for: `sidebarVisible: boolean`, `bottomPanelVisible: boolean`, `bottomPanelHeight: number`, `sidebarWidth: number`, `activeSidebarTab: 'files' | 'search' | 'git'`

- [ ] **Implement `TitleBar.tsx`** — Custom title bar (since we set `decorations: false` in Tauri config):
  - Reserve space on the left for macOS traffic light buttons (close/minimize/maximize) — these still render natively, we just need ~80px padding-left so content doesn't overlap them
  - Display app name "Android IDE" centered or left-aligned after traffic lights
  - Add `data-tauri-drag-region` attribute to the title bar div so the user can drag the window by it (Tauri feature)
  - Show the current project folder name in the title bar (e.g., "Android IDE — MyApp")
  - Height: 38px (standard macOS title bar height)

- [ ] **Implement `Sidebar.tsx`** — Vertical icon bar on the far left:
  - 48px wide, full height (below title bar, above status bar)
  - Icon buttons stacked vertically: Files (folder icon), Search (magnifying glass), Git (branch icon)
  - Active icon has a left border accent and brighter color
  - Clicking an icon sets `activeSidebarTab` in the store, which controls what renders in the sidebar content area
  - A secondary content panel (240px wide, resizable) appears next to the icon bar showing the active view (file tree, search panel, git panel). For Phase 1 only the file tree is functional; others show "Coming soon" placeholder.
  - The content panel collapses when clicking the already-active icon (toggle behavior)

- [ ] **Implement `PanelContainer.tsx`** — Bottom panel zone for Build, Logcat, Terminal, AI Chat:
  - Tab bar at the top of the panel zone with panel names
  - Only one panel visible at a time (tab switching)
  - For Phase 1, show placeholder "Build", "Logcat", "Terminal" tabs with empty content and a "Coming in Phase 2/4" label
  - Entire container collapses (height → 0 with animation) when `bottomPanelVisible` is false
  - Minimum height: 100px. Default height: 250px. Maximum: 60% of window height.

- [ ] **Implement `Resizable.tsx`** — A generic drag-to-resize splitter component:
  - Renders a thin bar (4px wide or tall) between two areas
  - On mouse-down + drag, updates the size signal in the store
  - Supports both horizontal (sidebar width) and vertical (bottom panel height) modes
  - Shows a resize cursor (`col-resize` or `row-resize`) on hover
  - Props: `direction: 'horizontal' | 'vertical'`, `onResize: (delta: number) => void`, `minSize: number`, `maxSize: number`
  - Double-click resets to default size

- [ ] **Implement `StatusBar.tsx`** — Thin bar at the very bottom of the window:
  - Height: 24px, dark background
  - Left side: placeholder items for future phases — "Ready" text, build status icon, connected devices count
  - Right side: placeholder items — file encoding (UTF-8), line/column position, Kotlin language label
  - Each item is a `StatusBarItem` component with optional click handler and tooltip
  - For Phase 1 just show: `Ready` on the left, and `Ln X, Col Y` + `UTF-8` + `Kotlin` on the right (update line/col from active editor)

- [ ] **Apply dark theme** — Create `styles/theme.css` with CSS custom properties:
  - Background colors: `--bg-primary` (editor), `--bg-secondary` (sidebar/panels), `--bg-tertiary` (title bar, status bar)
  - Text colors: `--text-primary`, `--text-secondary`, `--text-muted`
  - Accent color: `--accent` (for active states, selections)
  - Border color: `--border`
  - Scrollbar styling (thin, dark, subtle)
  - Font: `--font-mono` set to `"SF Mono", "Fira Code", "JetBrains Mono", "Menlo", monospace`
  - Font size: `--font-size-editor: 13px`, `--font-size-ui: 12px`
  - Use a palette similar to VS Code Dark+ or One Dark Pro (dark grays: #1e1e1e, #252526, #2d2d30, #3e3e42)
  - Import in `global.css` and apply `html, body { background: var(--bg-primary); color: var(--text-primary); }` plus `* { margin: 0; padding: 0; box-sizing: border-box; }`

- [ ] **Add icon library** — Install `@phosphor-icons/web` (lightweight icon set with consistent style) or use inline SVG components. We need icons for: folder, file, chevron-right, chevron-down, close (×), search, git-branch, terminal, play, and more. Create an `Icon.tsx` wrapper that takes `name` and `size` props.

- [ ] **Implement keyboard shortcuts** — Create `lib/keybindings.ts`:
  - Register global keyboard shortcuts via `document.addEventListener('keydown', ...)`:
    - `Cmd+B` — toggle sidebar visibility
    - `Cmd+J` — toggle bottom panel visibility
    - `Cmd+O` — open folder dialog (calls Tauri `dialog.open` with `directory: true`)
  - Use a keybinding registry pattern so future phases can add shortcuts: `registerKeybinding(key: string, action: () => void, context?: string)`
  - Prevent default browser behavior for registered shortcuts (e.g., Cmd+S must not trigger browser save dialog)

**Section done when:** The app renders a proper IDE shell layout with sidebar (icons + content area), editor zone, collapsible bottom panel, status bar, and dark theme. Panels resize by dragging splitters. Cmd+B toggles sidebar, Cmd+J toggles bottom panel.

---

### 1.3 — File System Backend (Rust)

> Build the Rust services that read the project directory, watch for changes, and provide file CRUD operations. The frontend will call these via Tauri IPC commands. All file I/O goes through this layer — the frontend never touches the filesystem directly.

**Key architectural decision:** The `ignore` crate (from the ripgrep project) is used instead of plain `walkdir` for directory traversal because it natively understands `.gitignore` rules, which is critical — Android projects have massive `build/` directories that must be excluded. Without this, loading a project with compiled outputs would be unusably slow.

- [ ] **Add Rust crate dependencies** — In `src-tauri/Cargo.toml`:
  ```toml
  [dependencies]
  notify = { version = "7", features = ["macos_fsevent"] }  # File watching via macOS FSEvents
  walkdir = "2"                                              # Recursive traversal (for non-gitignore cases)
  ignore = "0.4"                                             # .gitignore-aware traversal (primary)
  serde = { version = "1", features = ["derive"] }           # Serialize Rust structs for Tauri IPC
  serde_json = "1"                                           # JSON serialization
  tokio = { version = "1", features = ["full"] }             # Async runtime (Tauri uses tokio)
  ```

- [ ] **Define data models** — Create `src-tauri/src/models/file.rs`:
  ```rust
  #[derive(Serialize, Clone)]
  pub struct FileNode {
      pub name: String,          // "MainActivity.kt"
      pub path: String,          // Absolute path: "/Users/.../app/src/main/.../MainActivity.kt"
      pub kind: FileKind,        // File or Directory
      pub children: Option<Vec<FileNode>>,  // None for files, Some([...]) for directories
      pub extension: Option<String>,        // "kt", "gradle.kts", "xml" — used for icon selection
  }

  #[derive(Serialize, Clone)]
  pub enum FileKind { File, Directory }
  ```
  Directories come first (sorted alphabetically), then files (sorted alphabetically) — matching VS Code/Cursor convention.

- [ ] **Implement `fs_manager.rs` — `build_file_tree`** — Create `src-tauri/src/services/fs_manager.rs`:
  - Accept a root path, use the `ignore::WalkBuilder` to traverse recursively
  - `WalkBuilder` automatically reads `.gitignore` files at every directory level and excludes matching paths
  - Additionally hardcode exclusions for: `build/`, `.gradle/`, `.idea/`, `.git/`, `node_modules/`, `*.class`, `*.dex` (common Android artifacts that aren't in .gitignore in some projects)
  - Build a tree structure from the flat walk results: for each entry, find its parent directory in the tree and append
  - Sort: directories first (alphabetical), then files (alphabetical) — case-insensitive
  - Performance target: build tree for a 500-file project in < 200ms
  - Return `FileNode` with `children: Some(vec![])` for directories (even empty ones) and `children: None` for files

- [ ] **Implement `fs_manager.rs` — file watching** — Use `notify` crate with `RecommendedWatcher` (picks FSEvents on macOS):
  - `start_watching(root: PathBuf, tx: Sender<FileEvent>)` — recursively watches the project root
  - Debounce events: file editors (including our own) often trigger multiple write events for a single save. Use `notify`'s built-in debouncer with a 200ms window.
  - Map `notify` events to our `FileEvent` enum: `Created(path)`, `Modified(path)`, `Deleted(path)`, `Renamed(old, new)`
  - Emit events via Tauri's event system (`app_handle.emit("file:changed", payload)`) so the frontend can subscribe
  - Ignore events for paths that match our exclusion filters (don't notify about changes in `build/`)

- [ ] **Implement file CRUD operations** — In `fs_manager.rs` (or `commands/file_system.rs`):
  - `read_file(path: String) -> Result<String, String>` — Read file content as UTF-8. Return error for binary files or permission errors. Set a max file size limit (10MB) and return an error for huge files instead of hanging.
  - `write_file(path: String, content: String) -> Result<(), String>` — Write content atomically: write to a temp file first, then rename (prevents corruption on crash). Preserve original file permissions.
  - `create_file(path: String) -> Result<(), String>` — Create an empty file. Error if parent directory doesn't exist.
  - `create_directory(path: String) -> Result<(), String>` — Create a directory, including parent dirs (`create_dir_all`).
  - `delete_file(path: String) -> Result<(), String>` — Move to macOS Trash instead of permanent delete (use `trash` crate or `NSFileManager` via `objc` crate). Safer for users.
  - `rename_file(old_path: String, new_path: String) -> Result<(), String>` — Rename/move a file. Error if target already exists.

- [ ] **Register Tauri IPC commands** — In `src-tauri/src/lib.rs` (or `commands/mod.rs`):
  - Register all file commands in the `tauri::Builder` with `.invoke_handler(tauri::generate_handler![...])`
  - Mark commands as `#[tauri::command]` with proper async signatures
  - Set up shared app state using `tauri::Manager::manage()` to share the file watcher handle and project root path across commands
  - Create an `AppState` struct to hold: `project_root: Option<PathBuf>`, `watcher: Option<RecommendedWatcher>`

- [ ] **Implement "Open Folder" Tauri command** — `open_project(path: String)`:
  - Set the `project_root` in app state
  - Start the file watcher for this root
  - Build and return the initial file tree
  - This is called after the user picks a folder via the native dialog on the frontend side

**Section done when:** Rust backend can build a file tree for a real Android project (respecting .gitignore), read/write/create/delete files, and emit events when files change on disk.

---

### 1.4 — File Tree Component (Frontend)

> Render the project file tree in the sidebar. The user can browse directories, open files, and see the tree update when files change externally. This is the primary navigation for the IDE — it must be fast and familiar to anyone who has used VS Code or Android Studio.

- [ ] **Implement `FileTree.tsx`** — The main tree component, rendered inside the sidebar content area:
  - On mount (or when `project_root` changes), call the Rust `get_file_tree` command to fetch the full tree
  - Render the tree recursively using `FileTreeNode` components
  - Manage expand/collapse state locally: `expandedDirs: Set<string>` (store absolute paths of expanded dirs). Root-level directories start expanded, deeper ones start collapsed.
  - Listen for Tauri `file:changed` events and refresh the affected subtree (don't reload the entire tree — just re-fetch the changed directory's children from Rust and merge)
  - Show a loading spinner while the initial tree is being fetched
  - Show an empty state with "Open Folder" button when no project is open

- [ ] **Implement `FileTreeNode.tsx`** — A single row in the tree (file or directory):
  - Indentation: `padding-left = depth * 16px` (visually nests children under parents)
  - Directory row: chevron-right icon (▶) when collapsed, chevron-down (▼) when expanded. Click the entire row to toggle.
  - File row: icon based on extension (`.kt` → Kotlin icon, `.gradle.kts` → Gradle icon, `.xml` → XML icon, `.json` → JSON icon, default → generic file icon). Use color-coded letter icons as a simple approach: K (purple), G (green), X (orange), etc.
  - Highlight on hover (subtle background change)
  - Selected file has a distinct background highlight (stays highlighted while that file is open in the active tab)
  - **Single-click**: select the node (highlight it), preview the file in a "preview tab" (italic title, replaced when you click another file — optional, can skip for MVP and just open normally)
  - **Double-click**: open the file in a regular tab (non-preview, persistent)
  - Text is the filename only, not the full path. Show full path in a tooltip on hover.
  - Truncate long filenames with ellipsis

- [ ] **Right-click context menu** — When right-clicking a file or directory node:
  - On a file: New File, New Folder, Rename, Delete, Copy Path, Copy Relative Path, Reveal in Finder
  - On a directory: New File, New Folder, Rename, Delete, Copy Path, Collapse All, Reveal in Finder
  - On empty space: New File, New Folder, Open in Terminal
  - Use a custom `<ContextMenu>` component positioned at mouse coordinates
  - "New File" / "New Folder": show an inline text input in the tree (editable node) where the user types the name, press Enter to create, Escape to cancel
  - "Rename": convert the filename into an inline text input, pre-filled with current name, selected. Enter confirms, Escape cancels.
  - "Delete": show a confirmation dialog ("Move {name} to Trash?")
  - "Reveal in Finder": call `tauri::api::shell::open` with the file's parent directory
  - "Copy Path": copy absolute path to clipboard via `navigator.clipboard.writeText()`

- [ ] **Keyboard navigation in the tree** — When the file tree is focused:
  - `↑`/`↓` — move selection up/down through visible nodes
  - `→` — expand directory (if collapsed) or move to first child
  - `←` — collapse directory (if expanded) or move to parent
  - `Enter` — open selected file in editor (same as double-click)
  - `Delete` / `Backspace` — trigger delete with confirmation
  - `Space` — toggle expand/collapse on directory
  - The tree panel is focusable and receives focus when clicked

- [ ] **"Open Folder" flow** — Implement the full project-open sequence:
  1. User clicks "Open Folder" button (in empty state or via Cmd+O)
  2. Frontend calls Tauri's `dialog.open({ directory: true, multiple: false })` to show a native macOS folder picker
  3. If the user selects a folder, call the Rust `open_project(path)` command
  4. Rust starts file watching and returns the file tree
  5. Frontend stores the project root in `project.store.ts` and renders the tree
  6. Title bar updates to show the project folder name
  7. If the user opens a new folder while another is open, close all open tabs (prompt to save dirty ones first)

- [ ] **Performance consideration** — For Phase 1, a flat recursive render is fine for typical Android projects (200-1000 files). If performance becomes an issue in testing with very large projects, implement windowed/virtualized rendering (render only visible rows + buffer). Defer this optimization unless needed — SolidJS's fine-grained reactivity already avoids unnecessary re-renders.

**Section done when:** User can open an Android project folder, browse the file tree with expand/collapse, see file icons, right-click for context actions (new file, rename, delete), and double-click to open files. Tree updates when files change externally.

---

### 1.5 — Code Editor Integration (Frontend)

> Integrate CodeMirror 6 as the code editor. Each open file gets its own editor state (text content, cursor, scroll position, undo history). When the user switches tabs, we swap the editor state without destroying the DOM element — this makes tab switching instant. Kotlin and Gradle files get proper syntax highlighting.

**Why CodeMirror 6 over Monaco:** Monaco (VS Code's editor) has documented compatibility issues with WebKit/WKWebView (which Tauri uses on macOS). CodeMirror 6 is designed for cross-browser compatibility, is more modular (smaller bundle), and has a cleaner extension API. See PLAN.md for full rationale.

- [ ] **Install CodeMirror 6 packages** — Add to `package.json`:
  ```
  @codemirror/state        — EditorState (immutable state container)
  @codemirror/view         — EditorView (DOM rendering)
  @codemirror/commands      — Standard keybindings (Cmd+Z undo, etc.)
  @codemirror/language      — Language support infrastructure, indentation
  @codemirror/autocomplete  — Autocomplete popup (used in Phase 3 for LSP, but set up the infrastructure now)
  @codemirror/search        — Find/replace within file (Cmd+F)
  @codemirror/lint          — Diagnostic display infrastructure (used in Phase 3 for LSP errors)
  @codemirror/legacy-modes  — Bridge to CodeMirror 5 language modes (Kotlin mode lives here)
  @codemirror/lang-javascript — For potential JSON file editing
  ```

- [ ] **Implement `lib/codemirror/setup.ts`** — Base editor configuration shared by all file types:
  - Extension array:
    - `lineNumbers()` — line numbers in the left gutter
    - `highlightActiveLineGutter()` — highlight the active line's gutter
    - `highlightSpecialChars()` — show non-printable characters
    - `history()` — undo/redo with Cmd+Z / Cmd+Shift+Z
    - `foldGutter()` — collapsible code regions (functions, classes, if blocks)
    - `drawSelection()` — custom selection rendering
    - `dropCursor()` — cursor shown when dragging text
    - `EditorState.allowMultipleSelections.of(true)` — multiple cursors support
    - `indentOnInput()` — auto-indent on typing
    - `bracketMatching()` — highlight matching brackets
    - `closeBrackets()` — auto-close `(`, `[`, `{`, `"`, `'`
    - `autocompletion()` — autocomplete infrastructure (will be connected to LSP in Phase 3)
    - `rectangularSelection()` — Alt+drag for rectangular selection
    - `highlightActiveLine()` — subtle highlight on the current line
    - `highlightSelectionMatches()` — highlight other occurrences of selected text
    - `keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, ...completionKeymap, ...foldKeymap])` — standard keybindings
    - `EditorView.lineWrapping` — NOT enabled by default (code editors don't wrap lines, they scroll horizontally)
  - Export as `const baseExtensions: Extension[]`

- [ ] **Implement `lib/codemirror/kotlin.ts`** — Kotlin syntax highlighting:
  - Use `@codemirror/legacy-modes` to import the `clike` mode from CodeMirror 5
  - Configure the `clike` mode with Kotlin-specific settings:
    - Keywords: `fun`, `val`, `var`, `class`, `interface`, `object`, `when`, `if`, `else`, `for`, `while`, `do`, `return`, `break`, `continue`, `is`, `in`, `as`, `by`, `constructor`, `init`, `companion`, `data`, `sealed`, `enum`, `abstract`, `open`, `override`, `private`, `protected`, `internal`, `public`, `suspend`, `inline`, `crossinline`, `noinline`, `reified`, `expect`, `actual`, `annotation`, `typealias`, `import`, `package`, `try`, `catch`, `finally`, `throw`, `null`, `true`, `false`, `this`, `super`, `it`, `typeof`
    - Block keywords: `class`, `interface`, `fun`, `if`, `else`, `for`, `while`, `when`, `try`, `catch`, `finally`
    - Types: `Int`, `Long`, `Short`, `Byte`, `Float`, `Double`, `Boolean`, `Char`, `String`, `Unit`, `Nothing`, `Any`, `Array`, `List`, `Map`, `Set`, `MutableList`, `MutableMap`, `MutableSet`
    - Atoms: `true`, `false`, `null`
    - String prefixes: `"""` for multiline strings, `$` for string templates
    - Annotation handling: `@` prefix
    - Comment styles: `//` line comments, `/* */` block comments, `/** */` doc comments
  - Wrap in `StreamLanguage.define(kotlinMode)` and export as a CodeMirror `LanguageSupport` extension

- [ ] **Implement `lib/codemirror/gradle.ts`** — Gradle/Kotlin script syntax:
  - Reuse the Kotlin mode since `.gradle.kts` files are Kotlin DSL
  - Add additional highlighting for Gradle-specific DSL functions: `plugins`, `dependencies`, `repositories`, `android`, `buildTypes`, `productFlavors`, `compileSdk`, `minSdk`, `targetSdk`, `implementation`, `testImplementation`, `kapt`, `ksp`
  - For plain `.gradle` files (Groovy-based), use the `groovy` mode from `@codemirror/legacy-modes` if available, or fall back to Kotlin mode (acceptable for beta)

- [ ] **Implement `lib/codemirror/theme.ts`** — Custom dark theme matching the app palette:
  - Use `EditorView.theme({...})` to set:
    - Editor background: `var(--bg-primary)` (same as app background)
    - Gutter background: slightly darker
    - Active line: subtle highlight (e.g., `#ffffff08`)
    - Selection: `#264f78` (VS Code-like selection blue)
    - Matching bracket: `#ffffff30` background
    - Cursor: `#fff` (white, blinking)
  - Use `HighlightStyle.define([...])` for syntax token colors:
    - Keywords: `#569cd6` (blue)
    - Strings: `#ce9178` (orange)
    - Comments: `#6a9955` (green)
    - Numbers: `#b5cea8` (light green)
    - Types/classes: `#4ec9b0` (teal)
    - Functions: `#dcdcaa` (yellow)
    - Properties: `#9cdcfe` (light blue)
    - Annotations: `#d7ba7d` (gold)
    - Operators: `#d4d4d4` (light gray)
  - Export as `const editorTheme: Extension`

- [ ] **Implement `CodeEditor.tsx`** — The SolidJS wrapper around CodeMirror 6:
  - On mount: create a `div` ref and instantiate `new EditorView({ parent: divRef })` once
  - **State management pattern**: maintain a `Map<filePath, EditorState>` in the editor store. When a file is opened, create a new `EditorState` with the file content and extensions. When switching tabs, call `editorView.setState(stateForFile)` — this instantly swaps the content without recreating the DOM.
  - **Language detection**: based on file extension:
    - `.kt` → Kotlin language extension
    - `.gradle.kts`, `.gradle` → Gradle extension
    - `.xml` → basic XML highlighting (use `@codemirror/lang-xml` if available, or skip for Phase 1)
    - `.json` → JavaScript language extension (for `package.json` etc.)
    - Other → no highlighting (plain text)
  - **Change tracking**: register an `EditorView.updateListener` that fires on every edit. Compare the doc content with the last-saved content to determine dirty state. Update `editor.store.ts` dirty flag for this file.
  - **Save integration**: listen for Cmd+S (register in the CM6 keymap, not globally, so it only fires when the editor is focused). On Cmd+S: read the current doc content from `editorView.state.doc.toString()`, call the Rust `write_file` command, and clear the dirty flag on success.
  - **Focus management**: call `editorView.focus()` after tab switch or on mount. The editor should always have focus when it's the active element (so keyboard shortcuts like Cmd+Z work without clicking first).
  - **Responsive**: the editor fills its container. Use `EditorView.domEventHandlers` or CSS to ensure it stretches to fit the available space. Handle window resize.

**Section done when:** Opening a `.kt` file shows Kotlin syntax highlighting with keywords in blue, strings in orange, comments in green. The editor supports undo/redo, bracket matching, code folding, find/replace (Cmd+F), and multiple cursors.

---

### 1.6 — Tab System

> Manage multiple open files as tabs. Each tab represents one open file with its own editor state. The tab bar sits above the editor area and provides visual feedback about which files are open, which is active, and which have unsaved changes. Tab switching must be instant (< 50ms perceived) because developers switch tabs constantly.

- [ ] **Implement `editor.store.ts`** — Central reactive store for all editor state:
  ```typescript
  interface OpenFile {
    path: string;            // Absolute file path (unique key)
    name: string;            // Filename for display ("MainActivity.kt")
    content: string;         // Last-saved content (for dirty detection)
    dirty: boolean;          // true if editor content differs from last save
    editorState: EditorState; // CodeMirror EditorState for this file
    language: string;        // "kotlin" | "gradle" | "xml" | "json" | "text"
  }

  // Store signals:
  openFiles: Map<string, OpenFile>   // path → OpenFile
  activeFilePath: string | null       // currently visible tab
  tabOrder: string[]                  // ordered list of paths (tab bar order)
  recentFiles: string[]               // last 20 opened files (for Cmd+P in Phase 3)
  ```
  - `openFile(path)`: check if already open → just activate it. Otherwise, call Rust `read_file`, create `EditorState` with content + language extensions, add to `openFiles` and `tabOrder`, set as active.
  - `closeFile(path)`: if dirty → prompt save dialog. Remove from `openFiles` and `tabOrder`. If it was active → activate the nearest remaining tab (prefer right, then left, then null).
  - `setActiveFile(path)`: update `activeFilePath`. Swap the `EditorView` state to this file's `EditorState`. Restore cursor/scroll position.
  - `markDirty(path)` / `markClean(path)`: toggle the dirty flag.
  - `saveFile(path)`: get current content from EditorState doc, call Rust `write_file`, update `content` field, `markClean`.

- [ ] **Implement `EditorTabs.tsx`** — The tab bar component:
  - Renders a horizontal row of tabs from `tabOrder`
  - Each tab shows:
    - File icon (small, matching the file type)
    - Filename (just the name, not the full path)
    - Dirty indicator: when `dirty` is true, replace the close button with a filled dot (●). On hover, the dot turns back into the close (×) button.
    - Close button (×): visible on hover for clean files, always visible for the active tab
  - Active tab: brighter background, top border accent color, text is fully opaque
  - Inactive tabs: dimmer background, text is slightly transparent
  - **Click behavior**: left-click → activate tab. Middle-click → close tab. Right-click → context menu.
  - **Context menu per tab**: Close, Close Others, Close All, Close to the Right, Copy Path, Reveal in Finder
  - **Overflow**: when tabs exceed the available width, add horizontal scroll (scroll with mouse wheel or trackpad). Do NOT wrap to multiple lines.
  - **Drag to reorder**: (nice-to-have, can skip for MVP) drag a tab to rearrange `tabOrder`.
  - **Keyboard shortcuts**:
    - `Cmd+W` — close active tab
    - `Cmd+Shift+[` or `Ctrl+Shift+Tab` — switch to previous tab
    - `Cmd+Shift+]` or `Ctrl+Tab` — switch to next tab
    - `Cmd+1` through `Cmd+9` — switch to tab by position (1=first, 9=last)
  - When the last tab is closed, show a welcome/empty state in the editor area (e.g., "Open a file from the sidebar" + keyboard shortcut hints)

- [ ] **Save/restore editor state on tab switch** — Critical for developer experience:
  - Before switching away from the active tab: capture the current `EditorState` (includes cursor position, selection, scroll position, undo history, fold state) by reading `editorView.state`
  - Store it back in the `openFiles` map for that path
  - After switching to the new tab: call `editorView.setState(newFile.editorState)` to restore everything
  - This means undo/redo history is preserved per file — undoing in file A doesn't undo changes in file B

**Section done when:** User can open multiple files in tabs, see which are dirty, close them (with save prompt for dirty ones), switch between them with preserved cursor/scroll/undo, and use keyboard shortcuts to navigate tabs.

---

### 1.7 — File Operations & IPC Layer

> Wire together the frontend and backend for all file operations. Create a typed TypeScript API layer that wraps Tauri IPC calls, handle errors gracefully, and implement save/prompt flows. This section makes the IDE feel solid — saves are reliable, errors are handled, and the user is protected from data loss.

- [ ] **Implement `lib/tauri-api.ts`** — Typed async wrappers for all Rust commands:
  ```typescript
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { listen } from '@tauri-apps/api/event';

  export async function getFileTree(root: string): Promise<FileNode> { ... }
  export async function readFile(path: string): Promise<string> { ... }
  export async function writeFile(path: string, content: string): Promise<void> { ... }
  export async function createFile(path: string): Promise<void> { ... }
  export async function createDirectory(path: string): Promise<void> { ... }
  export async function deleteFile(path: string): Promise<void> { ... }
  export async function renameFile(oldPath: string, newPath: string): Promise<void> { ... }
  export async function openProject(path: string): Promise<FileNode> { ... }
  export async function openFolderDialog(): Promise<string | null> { ... }
  ```
  - Each function wraps `invoke('command_name', { args })` with proper types
  - Add error handling: catch Tauri invoke errors and convert to user-friendly messages
  - Export TypeScript types matching the Rust models (`FileNode`, `FileKind`, `FileEvent`, etc.)

- [ ] **Implement file save flow**:
  - `Cmd+S` in the editor → `saveFile(activeFilePath)` in the store:
    1. Read current doc content from the `EditorView`: `editorView.state.doc.toString()`
    2. Call `writeFile(path, content)` via Tauri
    3. If success: update `openFiles[path].content = content`, set `dirty = false`
    4. If error: show a toast notification at the bottom of the screen ("Failed to save: {reason}")
  - `Cmd+Option+S` — "Save All": iterate over all dirty files in `openFiles` and save each one

- [ ] **Implement unsaved-changes protection**:
  - **Closing a dirty tab**: before removing from `openFiles`, check `dirty`. If dirty, show a modal dialog:
    - "Do you want to save the changes you made to {filename}?"
    - Three buttons: `Save` (save then close), `Don't Save` (close without saving), `Cancel` (abort close)
    - Use a SolidJS modal component (not a native dialog — more consistent look)
  - **Closing the entire app**: Tauri's `window.onCloseRequested` event can be intercepted. If any files are dirty, show a dialog listing all dirty files with Save All / Discard All / Cancel options. If the user confirms, allow the window to close; otherwise, prevent it.
  - **Opening a new project** (new Cmd+O while files are open): same flow — prompt for dirty files first, then close all tabs, then open the new project.

- [ ] **External file change handling** — When the file watcher detects a modified file that is currently open in a tab:
  - If the tab is clean (user hasn't edited it): silently reload the file content into the EditorState. This keeps the editor in sync when external tools (e.g., Gradle, git) modify files.
  - If the tab is dirty (user has unsaved edits): show a notification bar at the top of the editor: "This file has been changed externally. [Reload] [Keep mine]". The user decides.
  - If the file is deleted externally while open: show notification "This file has been deleted. [Save as new file] [Close tab]".

- [ ] **Toast notification system** — Create a simple `<Toast>` component:
  - Appears at the bottom-right of the window
  - Auto-dismisses after 3-5 seconds
  - Types: success (green), error (red), warning (yellow), info (blue)
  - Used for: save success (if configured), save error, file deleted, etc.
  - Stack multiple toasts vertically if they overlap

**Section done when:** Cmd+S reliably saves files. Closing dirty tabs prompts the user. External file changes are handled gracefully. Errors show helpful toast messages. The app protects users from data loss.

---

### 1.8 — Integration & Verification

> Bring everything together and verify Phase 1 works end-to-end with a real Android project. Fix bugs, polish rough edges, and ensure the foundation is solid for Phase 2.

- [ ] **End-to-end test with a real project** — Clone Google's Sunflower sample app (`git clone https://github.com/android/sunflower.git`) and open it in the IDE:
  - [ ] File tree loads correctly — `build/`, `.gradle/`, `.idea/` are hidden
  - [ ] Kotlin files (`.kt`) open with proper syntax highlighting
  - [ ] `build.gradle.kts` files open with Gradle highlighting
  - [ ] Can open 5+ files in tabs simultaneously without lag
  - [ ] Tab switching is instant (cursor position and scroll restored)
  - [ ] Editing a file and pressing Cmd+S saves it (verify with `cat` in terminal)
  - [ ] Creating/renaming/deleting a file from the context menu works and the tree updates
  - [ ] Modifying a file externally (in another editor) updates the tree and open tab

- [ ] **Test with a large project** — If available, test with a 1000+ file project to verify:
  - [ ] File tree loads within 2 seconds
  - [ ] No visible lag when expanding large directories
  - [ ] Editor does not slow down with a 2000-line Kotlin file open

- [ ] **Polish pass**:
  - [ ] Verify all keyboard shortcuts work: Cmd+O, Cmd+S, Cmd+W, Cmd+B, Cmd+J, Cmd+Shift+[, Cmd+Shift+], Cmd+F (find in file)
  - [ ] Verify theme is consistent: no white flashes, no unstyled elements, scrollbars are dark
  - [ ] Verify the app window title shows "Android IDE — {project_name}"
  - [ ] Verify status bar shows line/column position of cursor in the active editor
  - [ ] Verify no Rust panics or TypeScript console errors in normal usage

- [ ] **Commit and tag** — Create a clean commit of all Phase 1 work. Tag as `v0.1.0-alpha` for reference.

---

## Phase 2 — Build System + Devices
**Goal:** Complete build-deploy-run cycle from the IDE.

### Process Manager (Rust)
- [ ] Implement `process_manager.rs`:
  - [ ] `spawn_process(cmd, args, cwd, env) -> ProcessHandle` using `tokio::process::Command`
  - [ ] Stream stdout and stderr line-by-line via Tauri Channel
  - [ ] Support cancellation: send SIGTERM, track child PID
  - [ ] Track running processes: `HashMap<ProcessId, ChildProcess>`
  - [ ] Emit process exit event with exit code

### Build Runner (Rust)
- [ ] Implement `build_runner.rs`:
  - [ ] `run_gradle_task(project_root, task, variant?) -> BuildProcess`
  - [ ] Detect `gradlew` in project root and parent directories
  - [ ] Set `JAVA_HOME` environment variable if configured
  - [ ] Stream build output via Channel as `BuildLine { kind: Output|Error|Warning|Progress, content, file?, line?, col? }`
  - [ ] Parse Kotlin compiler errors: regex for `e: file:///path:line:col: message`
  - [ ] Parse Kotlin warnings: `w: file:///path:line:col: message`
  - [ ] Parse Gradle task progress: `:app:compileDebugKotlin`, task success/failure
  - [ ] Detect build success (`BUILD SUCCESSFUL`) and failure (`BUILD FAILED`)
  - [ ] Maintain build history: last 10 builds with full logs
  - [ ] Support cancellation via process manager
- [ ] Register commands: `run_gradle_task`, `cancel_build`, `get_build_history`

### Build Panel (Frontend)
- [ ] Implement `build.store.ts`: build state (idle/running/success/failed), log lines, structured errors, history
- [ ] Implement `BuildLogViewer.tsx`:
  - [ ] Virtualized streaming log with auto-scroll (pause on manual scroll)
  - [ ] ANSI escape code color rendering
  - [ ] Filter toggle: show all / errors only / warnings only
  - [ ] Line count indicator
- [ ] Implement `BuildPanel.tsx`:
  - [ ] Two view modes: Raw Log and Problems List
  - [ ] Build toolbar: Run, Cancel, Clear, view toggle
  - [ ] Problems list: grouped by file, clickable to jump to file:line in editor
  - [ ] Build summary: success/failure badge, duration, error count
  - [ ] Build history dropdown
- [ ] Jump to error on click: open file in editor, scroll to error line, highlight

### Variant Manager (Rust)
- [ ] Implement `variant_manager.rs`:
  - [ ] Parse `app/build.gradle.kts` and `build.gradle.kts` using Tree-sitter Kotlin grammar
  - [ ] Extract `buildTypes` block entries (debug, release, custom)
  - [ ] Extract `productFlavors` block entries and `flavorDimensions`
  - [ ] Compute all variant combinations as `Vec<Variant { name, build_type, flavors, gradle_task }>`
  - [ ] Fallback: run `./gradlew tasks --all` and parse task names to infer variants
  - [ ] Cache variants per project, invalidate on `build.gradle.kts` change
- [ ] Register commands: `get_build_variants`, `set_active_variant`

### Variant Selector (Frontend)
- [ ] Implement `VariantSelector.tsx`: searchable dropdown showing all variants, current variant highlighted
- [ ] Show active variant in status bar
- [ ] Keyboard shortcut to open variant picker (Cmd+Shift+V)
- [ ] Persist last-used variant per project

### ADB Manager (Rust)
- [ ] Add `adb_client` crate dependency
- [ ] Implement `adb_manager.rs`:
  - [ ] `list_devices() -> Vec<Device { serial, name, type: Physical|Emulator, state, api_level, model }`
  - [ ] Start polling for devices every 2 seconds, emit `device:connected` / `device:disconnected` events
  - [ ] `install_apk(device_serial, apk_path)` — `adb -s <serial> install -r <apk>`
  - [ ] `launch_app(device_serial, package, activity)` — `adb shell am start -n <package>/<activity>`
  - [ ] `get_device_properties(serial)` — `adb shell getprop`
- [ ] Register commands: `list_devices`, `install_apk`, `launch_app`

### Device Panel (Frontend)
- [ ] Implement `device.store.ts`: connected devices list, selected device, AVD list, emulator state
- [ ] Implement `DevicePanel.tsx`:
  - [ ] List connected physical devices and running emulators with model/API level
  - [ ] List available AVDs (from `~/.android/avd/`)
  - [ ] Launch emulator button per AVD
  - [ ] Select active device (for build/deploy target)
  - [ ] Refresh button
  - [ ] Device selector in toolbar/status bar

### Run Button
- [ ] Implement "Run" toolbar button and Cmd+R shortcut:
  1. Check active variant is set
  2. Check active device is selected
  3. Run `assembleDebug` (or active variant task)
  4. On success: run `install_apk` + `launch_app`
  5. Show progress in status bar and build panel
- [ ] Implement "Stop" button to kill app on device

**Phase 2 Done When:** Can build any Android project, switch build variants, deploy to an emulator, and see streaming build errors with file links.

---

## Phase 3 — Code Intelligence
**Goal:** Full Kotlin code intelligence via LSP — completions, diagnostics, navigation.

### Tree-sitter (Rust)
- [ ] Add `tree-sitter` and `tree-sitter-kotlin` crate dependencies
- [ ] Implement `treesitter.rs`:
  - [ ] `parse_file(content) -> Tree` — parse Kotlin file, cache tree per file
  - [ ] `extract_symbols(tree) -> Vec<Symbol { name, kind, range }>` — functions, classes, interfaces, objects, properties
  - [ ] `find_node_at_position(tree, line, col) -> Node` — for fallback go-to-definition
  - [ ] `get_document_outline(tree) -> Vec<OutlineItem>` — for symbols sidebar
  - [ ] Incremental re-parse on file change (Tree-sitter `edit` API)

### LSP Client (Rust)
- [ ] Add `lsp-types` and `serde_json` dependencies
- [ ] Implement `lsp_client.rs`:
  - [ ] Spawn `kotlin-language-server` sidecar process (via Tauri sidecar API)
  - [ ] Implement JSON-RPC 2.0 over stdin/stdout: request/response correlation by ID, notifications (no ID)
  - [ ] LSP `initialize` handshake with capabilities declaration
  - [ ] `textDocument/didOpen` on file open
  - [ ] `textDocument/didChange` on every edit (full text sync for now, incremental later)
  - [ ] `textDocument/didSave` on file save
  - [ ] `textDocument/didClose` on tab close
  - [ ] `textDocument/completion` → return completion items
  - [ ] `textDocument/hover` → return hover content
  - [ ] `textDocument/definition` → return target location
  - [ ] `textDocument/references` → return all reference locations
  - [ ] `textDocument/implementation` → return implementation locations
  - [ ] `textDocument/documentSymbol` → return file outline
  - [ ] `workspace/symbol` → return project-wide symbols
  - [ ] `textDocument/publishDiagnostics` notification handler
  - [ ] `textDocument/rename` → workspace edit
  - [ ] Handle LSP server crash: restart with backoff, notify user
  - [ ] Log all LSP traffic at debug level
- [ ] Register commands: `lsp_complete`, `lsp_hover`, `lsp_definition`, `lsp_references`, `lsp_implementation`, `lsp_rename`, `lsp_document_symbols`

### Kotlin LSP Sidecar
- [ ] Download `kotlin-language-server` release from fwcd/kotlin-language-server GitHub releases
- [ ] Pre-build or download binaries for `aarch64-apple-darwin` and `x86_64-apple-darwin`
- [ ] Place in `src-tauri/binaries/kotlin-lsp-aarch64-apple-darwin` and `kotlin-lsp-x86_64-apple-darwin`
- [ ] Configure in `tauri.conf.json` under `bundle.externalBin`
- [ ] Add JVM check: verify Java 11+ is available, show error if not
- [ ] Configure LSP workspace root on project open

### CodeMirror LSP Extension (Frontend)
- [ ] Implement `lib/codemirror/lsp-extension.ts`:
  - [ ] Diagnostics: render inline squiggles (underline decoration), gutter markers
  - [ ] Diagnostics: hover tooltip showing error message
  - [ ] Diagnostics panel at bottom (errors/warnings count)
  - [ ] Completions: request from Rust LSP client on trigger, show CM6 completion popup
  - [ ] Hover: request on Cmd+hover or after 500ms idle, show tooltip with type info and docs
  - [ ] Signature help: show on `(` typed
  - [ ] Debounce didChange notifications (300ms after last keystroke)

### Navigation
- [ ] Go-to-definition: Cmd+Click or F12 — open file in editor at definition location
- [ ] Go-to-implementation: Cmd+F12 — open implementation location
- [ ] Find references: Shift+F12 — show all references in search-results-style panel
- [ ] Peek definition: Option+F12 — inline peek preview (future, can skip for beta)
- [ ] Highlight all occurrences of symbol at cursor (via LSP `textDocument/documentHighlight`)
- [ ] "Go Back" navigation: Alt+Left, history of cursor positions

### Project-Wide Search
- [ ] Add ripgrep library crates (`grep-regex`, `grep-searcher`, `grep-matcher`) to `Cargo.toml`
- [ ] Implement `search_engine.rs`:
  - [ ] `search_text(query, root, options: {regex, case_sensitive, whole_word, include_pattern, exclude_pattern}) -> Stream<SearchResult>`
  - [ ] Respect `.gitignore` and project exclusions
  - [ ] Stream results as they are found (not batch-at-end)
  - [ ] Context lines: show N lines before/after each match
- [ ] Implement `SearchPanel.tsx`:
  - [ ] Search input with regex toggle, case toggle, whole-word toggle
  - [ ] File filter input (e.g. `*.kt`)
  - [ ] Streaming results grouped by file
  - [ ] Click result to open file at match location
  - [ ] Replace functionality (show preview, apply)
- [ ] Keyboard shortcut: Cmd+Shift+F open search panel

### Command Palette
- [ ] Implement `CommandPalette.tsx`:
  - [ ] Cmd+P: fuzzy file search (files in project, recently opened first)
  - [ ] Cmd+Shift+P: command search (all registered IDE actions)
  - [ ] Cmd+T: symbol search (workspace symbols via LSP)
  - [ ] Cmd+Shift+O: symbols in current file (document symbols)
  - [ ] Fuzzy matching with ranked results
  - [ ] Keyboard navigation (arrow keys, Enter to select, Escape to close)
- [ ] Register all IDE actions (build, device, panel toggles, etc.) in a central action registry

### Symbols Panel
- [ ] Implement document symbols sidebar (outline view): show Kotlin file structure — classes, functions, properties
- [ ] Subscribe to LSP `textDocument/documentSymbol` on active file change
- [ ] Clicking symbol scrolls editor to its location

**Phase 3 Done When:** Go-to-definition, completions, hover docs, find-references, and project search all work reliably on a real Kotlin Android project.

---

## Phase 4 — Logcat + Emulator
**Goal:** Production-quality logcat and AI-accessible emulator control.

### Logcat Streaming (Rust)
- [ ] Implement `logcat.rs`:
  - [ ] `start_logcat(device_serial) -> LogcatStream` — connect via adb_client logcat API
  - [ ] Parse `threadtime` format: `MM-DD HH:MM:SS.mmm PID TID LEVEL TAG: message`
  - [ ] Create `LogEntry` model: `{ id, timestamp, pid, tid, level, tag, package, message, is_crash }`
  - [ ] Detect crash entries: stack traces starting with `FATAL EXCEPTION`, `AndroidRuntime`, etc.
  - [ ] Ring buffer: store last 50,000 entries (configurable), drop oldest on overflow
  - [ ] Server-side filter engine:
    - [ ] `tag:` filter (exact and regex with `~`)
    - [ ] `level:` filter (V/D/I/W/E/F levels, >= semantics)
    - [ ] `package:` filter (exact and regex)
    - [ ] `message:` filter (contains and regex)
    - [ ] `age:` filter (entries within last Nm/Nh)
    - [ ] `is:crash` filter
    - [ ] `is:stacktrace` filter
    - [ ] Negation with `-` prefix
    - [ ] Implicit AND between different keys, OR between same keys
  - [ ] Batch entries at 60fps intervals before streaming to frontend (not per-line)
  - [ ] Persist logcat session to disk (SQLite via `rusqlite` crate or JSON)
  - [ ] Clear logcat buffer (`adb logcat -c`)
- [ ] Register commands: `start_logcat`, `stop_logcat`, `clear_logcat`, `get_recent_logs`, `search_logs`, `get_crash_logs`, `save_logcat_session`, `load_logcat_session`

### Logcat Panel (Frontend)
- [ ] Implement `logcat.store.ts`: entries (reactive array), filters, active session, streaming state
- [ ] Implement `LogcatEntry.tsx`: single log row — timestamp, level badge, tag, message. Color-coded: V=gray, D=blue, I=green, W=yellow, E=red/bold, F=purple/bold
- [ ] Implement `LogcatFilter.tsx`:
  - [ ] Filter input with Android Studio-style syntax (`tag:MyTag level:ERROR -tag:Volley`)
  - [ ] Autocomplete suggestions for filter keys
  - [ ] Log level quick toggles (V/D/I/W/E buttons)
  - [ ] Package filter dropdown (auto-populated from current logs)
  - [ ] Regex mode toggle
- [ ] Implement `LogcatPanel.tsx`:
  - [ ] Virtualized list using `VirtualList.tsx` (render only visible rows)
  - [ ] Auto-scroll to bottom (disable on manual scroll up, re-enable on scroll-to-bottom)
  - [ ] Search within logcat (Cmd+F)
  - [ ] Copy selected entry / copy message only
  - [ ] "Jump to Crash" button when crash is detected
  - [ ] Session management: save/load named sessions
  - [ ] Clear button, pause/resume button
  - [ ] Entry count display
  - [ ] "AI: Explain this crash" button on crash entries (Phase 5 integration)

### Emulator Controller (Rust)
- [ ] Implement `emulator_ctl.rs`:
  - [ ] `list_avds() -> Vec<AVD>` — scan `~/.android/avd/` directory, parse `.ini` files
  - [ ] `launch_emulator(avd_name, options)` — spawn `emulator @avd_name` with flags (`-no-boot-anim`, `-gpu auto`)
  - [ ] Track emulator processes (PID, console port, ADB port)
  - [ ] Wait for emulator to appear in `adb devices` (polling with timeout)
  - [ ] `connect_console(port) -> EmulatorConsole` — TCP connection to telnet console (port 5554+)
  - [ ] Console authentication: read `~/.emulator_console_auth_token`
  - [ ] Console commands:
    - [ ] `set_location(lat, lon, alt?)` — `geo fix <lon> <lat>`
    - [ ] `set_network_speed(type)` — `network speed <full|hsdpa|umts|edge|gprs>`
    - [ ] `set_network_delay(type)` — `network delay <none|gprs|umts|edge>`
    - [ ] `get_battery() / set_battery(level, status)` — `power capacity`, `power ac`
    - [ ] `rotate()` — `rotate`
    - [ ] `fold() / unfold()` — for foldable emulators
    - [ ] `list_snapshots() / save_snapshot(name) / load_snapshot(name) / delete_snapshot(name)`
    - [ ] `kill_emulator()` — `kill` command on console
  - [ ] `take_screenshot(device_serial) -> Vec<u8>` — `adb shell screencap -p` piped to PNG data
- [ ] Register commands: `list_avds`, `launch_emulator`, `stop_emulator`, emulator control commands, `take_screenshot`

### Emulator Controls (Frontend)
- [ ] Implement `EmulatorControls.tsx`:
  - [ ] GPS location input: lat/lon fields + "Set Location" button + preset locations
  - [ ] Network simulation: speed dropdown (Full/4G/3G/2G/GPRS) + delay dropdown
  - [ ] Battery: level slider + charging state toggle
  - [ ] Rotation button (90° rotate)
  - [ ] Fold/Unfold toggle (shown only for foldable AVDs)
  - [ ] Snapshot management: list, save, load, delete
  - [ ] Kill emulator button
- [ ] Show screenshot preview in device panel (optional for beta)

**Phase 4 Done When:** Logcat streams with full filtering working better than Android Studio, and emulator can be fully controlled from the IDE.

---

## Phase 5 — AI Integration
**Goal:** AI chat panel, inline editing, MCP server, semantic codebase search.

### MCP Server (Rust)
- [ ] Implement `ai/tools.rs`: define all MCP tool schemas (see PLAN.md MCP Tools section)
- [ ] Implement `ai/mcp_server.rs`:
  - [ ] MCP protocol handler: `initialize`, `tools/list`, `tools/call`
  - [ ] stdio transport (for Claude Code CLI integration)
  - [ ] HTTP+SSE transport (for other MCP clients, bound to localhost)
  - [ ] Tool implementations wiring to services:
    - [ ] File tools → `fs_manager`
    - [ ] Build tools → `build_runner` + `variant_manager`
    - [ ] Logcat tools → `logcat` ring buffer
    - [ ] Device/emulator tools → `adb_manager` + `emulator_ctl`
    - [ ] Code intelligence tools → `lsp_client` + `treesitter`
    - [ ] Git tools → `git_service`
- [ ] Register command: `start_mcp_server(port?)`, `stop_mcp_server`, `get_mcp_server_status`
- [ ] Show MCP server status in status bar (port, connected clients)

### Context Assembler (Rust)
- [ ] Implement `ai/context.rs`:
  - [ ] Parse @-mentions from user message: `@file:path`, `@selection`, `@diagnostics`, `@logcat`, `@build`, `@codebase:query`, `@terminal`, `@git`
  - [ ] Resolve each @-mention to content:
    - [ ] `@file:path` → read file content (truncate large files by showing relevant sections)
    - [ ] `@selection` → get current editor selection from store state
    - [ ] `@diagnostics` → LSP diagnostics for all open files
    - [ ] `@logcat` → last 200 entries from ring buffer (or filtered subset)
    - [ ] `@build` → last build output (errors + summary)
    - [ ] `@codebase:query` → semantic search results (Phase 5 embedding feature)
    - [ ] `@terminal` → last N lines of active terminal
    - [ ] `@git` → current diff + recent commit log
  - [ ] Token budget management: estimate tokens per chunk, prioritize most relevant, truncate lowest priority
  - [ ] Assemble final context with clear section headers for the LLM

### Codebase Indexing (Rust)
- [ ] Implement `ai/embeddings.rs`:
  - [ ] Load `all-MiniLM-L6-v2` model via `candle` crate (download on first use, cache in `~/.androidide/models/`)
  - [ ] `embed_text(text) -> Vec<f32>` — run inference, return embedding vector
  - [ ] HNSW vector index via `instant-distance` crate
  - [ ] `index_chunk(chunk_id, text, metadata)` — embed + insert into index
  - [ ] `search_similar(query, top_k) -> Vec<SearchResult { chunk_id, score }>` — embed query + ANN search
  - [ ] Persist index to disk (serialize HNSW + chunk metadata to `~/.androidide/index/<project_hash>/`)
- [ ] Implement `services/indexer.rs`:
  - [ ] `index_project(root)` — walk all `.kt` and `.gradle.kts` files
  - [ ] Per file: Tree-sitter parse → extract chunks (each function, class, top-level declaration is one chunk)
  - [ ] For each chunk: embed + store in HNSW index with `{ file_path, name, kind, start_line, end_line }` metadata
  - [ ] Background indexing on project open (don't block UI)
  - [ ] Emit indexing progress events to frontend (show progress bar in status bar)
  - [ ] Incremental re-index: on file change, remove old chunks for that file, extract + embed new chunks
  - [ ] Show indexing status in status bar

### AI Chat Panel (Frontend)
- [ ] Implement `ai.store.ts`: chat history `Array<Message { role, content, timestamp }>`, streaming state, model config
- [ ] Implement `ChatMessage.tsx`: render user and assistant messages, markdown rendering with syntax-highlighted code blocks, copy button on code blocks
- [ ] Implement `ContextSelector.tsx`:
  - [ ] Listen for `@` typed in chat input
  - [ ] Show autocomplete popup with context options
  - [ ] File search for `@file:` prefix
  - [ ] Option+Enter or @ button to manually insert context
- [ ] Implement `AIChatPanel.tsx`:
  - [ ] Chat history with auto-scroll
  - [ ] Message input (textarea with @-mention support)
  - [ ] Send button + Cmd+Enter shortcut
  - [ ] Streaming response display (tokens appear as they arrive)
  - [ ] Cancel generation button
  - [ ] Clear conversation button
  - [ ] Model selector dropdown (configured providers)
  - [ ] Context pills showing active @-mentions
  - [ ] "New Chat" button
- [ ] Connect to Rust: send message + context → receive streamed response via Channel

### Inline AI (CodeMirror Extension)
- [ ] Implement `lib/codemirror/ai-extension.ts`:
  - [ ] **Ghost text**: display AI completion suggestions as gray inline decoration, accept with Tab
  - [ ] Trigger ghost text on pause (800ms idle), cancel on further typing
  - [ ] **Cmd+K inline edit**:
    - [ ] Press Cmd+K → open inline input bar at cursor position (or below selection)
    - [ ] User types instruction → stream AI response
    - [ ] Show diff: added lines in green, removed in red (inline diff decoration)
    - [ ] Accept diff: Cmd+Enter or "Accept" button
    - [ ] Reject diff: Escape or "Reject" button
    - [ ] Accept/reject individual hunks

### AI-Powered Contextual Actions
- [ ] "Explain crash" button in logcat: sends crash stack trace + relevant source files to AI chat
- [ ] "Fix error" button on build errors: sends error + file content to AI with fix request
- [ ] "Generate commit message" in git panel: sends diff to AI, inserts result in commit input
- [ ] "Review changes" button in git panel: AI reviews staged diff and reports issues

### LLM Provider Setup
- [ ] Implement `commands/ai.rs`:
  - [ ] `send_message(messages, context, provider_config) -> Stream<Token>` — stream LLM response
  - [ ] Anthropic API: `POST /v1/messages` with streaming (`stream: true`)
  - [ ] OpenAI API: `POST /v1/chat/completions` with streaming
  - [ ] Ollama: `POST /api/chat` with streaming (localhost)
  - [ ] Error handling: rate limits, API errors, network failures
- [ ] Add `reqwest` crate (async HTTP client) to Cargo.toml
- [ ] Implement settings storage: API keys persisted securely in macOS Keychain (via `security` CLI or `keyring` crate)
- [ ] Settings UI: provider selection, API key input, model selection, max tokens, temperature

**Phase 5 Done When:** AI chat understands the Android project, inline edits work with diff preview, MCP server allows Claude Code to control the IDE, and "Explain crash" / "Fix error" work reliably.

---

## Phase 6 — Git + Terminal + Polish
**Goal:** Complete feature set, beta-ready packaging.

### Git Integration (Rust)
- [ ] Add `git2` crate to `Cargo.toml`
- [ ] Implement `services/git_service.rs`:
  - [ ] `git_status() -> Vec<FileStatus { path, status: Added|Modified|Deleted|Renamed|Untracked|Staged }`
  - [ ] `git_diff(file?) -> String` — unified diff format
  - [ ] `git_diff_staged() -> String` — staged changes diff
  - [ ] `git_log(count) -> Vec<Commit { hash, short_hash, message, author, date }>`
  - [ ] `git_stage(paths)` — `git add`
  - [ ] `git_unstage(paths)` — `git reset HEAD`
  - [ ] `git_commit(message)` — create commit
  - [ ] `git_list_branches() -> Vec<Branch { name, is_current, is_remote }>`
  - [ ] `git_checkout_branch(name)`
  - [ ] `git_stash() / git_stash_pop()`
  - [ ] Watch for `.git/index` changes, emit `git:status_changed` event
- [ ] Register commands: all git operations

### Git Panel (Frontend)
- [ ] Implement `git.store.ts`: file statuses, staged files, current branch, commit history
- [ ] Implement `DiffViewer.tsx`: render unified diff with color coding (green=added, red=removed), line numbers
- [ ] Implement `GitPanel.tsx`:
  - [ ] Branch name display + branch switcher dropdown
  - [ ] Changed files list grouped by status: Staged Changes / Changes / Untracked
  - [ ] Stage/unstage individual files (click `+`/`-` buttons)
  - [ ] "Stage All" / "Unstage All" buttons
  - [ ] Inline diff view when clicking a file
  - [ ] Commit message textarea with character count
  - [ ] Commit button (disabled when no staged changes or empty message)
  - [ ] "Generate with AI" button for commit message
  - [ ] Recent commit log (last 10 commits)
- [ ] Editor gutter git decorations: green bar = added lines, yellow bar = modified, red triangle = deleted

### Terminal
- [ ] Add `xterm` npm dependency and `portable-pty` Rust crate
- [ ] Implement PTY backend in `commands/`: spawn `zsh` (or user's $SHELL), read/write via PTY fd
- [ ] Implement `TerminalPanel.tsx`:
  - [ ] Mount xterm.js `Terminal` instance
  - [ ] Bidirectional data via Tauri Channel (frontend → Rust → PTY, PTY → Rust → frontend)
  - [ ] Correct terminal resizing (update PTY size on panel resize)
  - [ ] Multiple terminal tabs
  - [ ] Keyboard shortcut: Ctrl+\` to toggle terminal panel
- [ ] Set terminal `cwd` to project root on open

### Settings UI
- [ ] Implement settings screen (Cmd+,):
  - [ ] **Editor**: font family, font size, tab size, soft tabs, word wrap, minimap toggle
  - [ ] **Theme**: dark/light toggle, syntax theme picker
  - [ ] **Keybindings**: view and customize shortcuts (future: full editor)
  - [ ] **AI**: provider selection (Anthropic/OpenAI/Ollama), API key management, model selection, max tokens
  - [ ] **Kotlin LSP**: path to custom LSP binary, JVM heap size
  - [ ] **Android SDK**: ANDROID_HOME path, Java home
  - [ ] **Build**: Gradle options (JVM args, parallel builds)
- [ ] Persist settings to `~/.androidide/settings.json`
- [ ] Apply settings changes live (font changes update editor immediately)

### Performance Optimization
- [ ] Profile app with real large Android project (100+ files, 5+ modules):
  - [ ] File tree initial load time — should be < 500ms
  - [ ] File tree FSEvents update latency — should be < 100ms
  - [ ] Logcat frame rate with 1000 lines/sec input — should maintain 60fps
  - [ ] Editor tab switch time — should be < 50ms
  - [ ] Build panel with 10,000 log lines — should scroll smoothly
- [ ] Profile with Instruments (macOS) if performance issues found
- [ ] Optimize VirtualList overscan and row measurement
- [ ] Optimize SolidJS stores for fine-grained updates (avoid unnecessary re-renders)
- [ ] Rust: profile with `cargo flamegraph` if CPU hotspots found
- [ ] Minimize Tauri IPC overhead: batch small frequent updates

### First-Run Experience
- [ ] Detect `ANDROID_HOME` on launch (check env var, then common paths: `~/Library/Android/sdk`)
- [ ] If not found: show setup wizard with link to Android Studio SDK download or `sdkmanager` instructions
- [ ] Verify required SDK components (platform-tools, build-tools, at least one platform)
- [ ] Detect Java/JDK installation (required for Kotlin LSP and Gradle)
- [ ] On first file open: show welcome tab with quick-start guide
- [ ] "Open Recent" list on empty state

### Distribution & Signing
- [ ] Set up Apple Developer account ($99/year)
- [ ] Create Developer ID Application certificate in Keychain
- [ ] Create `Entitlements.plist` with required permissions:
  - [ ] `com.apple.security.cs.allow-jit` (WKWebView JIT)
  - [ ] `com.apple.security.cs.allow-unsigned-executable-memory`
  - [ ] `com.apple.security.network.client` (API calls)
- [ ] Configure `tauri.conf.json`:
  - [ ] `bundle.macOS.signingIdentity` → Developer ID certificate
  - [ ] `bundle.macOS.entitlements` → path to Entitlements.plist
  - [ ] `bundle.externalBin` → kotlin-lsp sidecar paths
- [ ] Build universal binary: `npm run tauri build -- --target universal-apple-darwin`
- [ ] Notarize with `notarytool`:
  - [ ] Create App Store Connect API key
  - [ ] Store credentials: `xcrun notarytool store-credentials`
  - [ ] Submit: `xcrun notarytool submit --wait`
  - [ ] Staple: `xcrun stapler staple`
- [ ] Test on clean macOS VM (no developer tools installed)
- [ ] Set up auto-update: configure `@tauri-apps/plugin-updater`, host `latest.json` on update server

### Beta Prep
- [ ] Error reporting: integrate Sentry or similar (opt-in, with user consent)
- [ ] Basic analytics: project open, build success/failure rate, feature usage (opt-in, no code content)
- [ ] Write user-facing documentation:
  - [ ] Setup guide (SDK, Java, first project)
  - [ ] Feature overview
  - [ ] Known limitations and workarounds (especially LSP quality)
  - [ ] MCP server setup guide for Claude Code integration
- [ ] Create GitHub repository for issue tracking
- [ ] Set up beta distribution: TestFlight alternative (direct DMG download, or use GitHub releases)

**Phase 6 Done When:** App is signed, notarized, packaged, and installable on a clean Mac with no developer tools.

---

## Recommended Additions (Not Original Scope)

These are highly recommended before inviting beta users:

- [ ] **SDK Manager integration**: detect ANDROID_HOME, verify SDK components, prompt install
- [ ] **XML syntax highlighting**: `.xml` files (layouts, manifests, drawables) with CodeMirror
- [ ] **Gradle Sync action**: explicit "Sync Project with Gradle Files" that resolves dependencies and updates LSP classpath
- [ ] **Multi-module project support**: understand `:app`, `:core`, `:feature-x` module boundaries in file tree, LSP, and build
- [ ] **Code formatting**: run `ktfmt` or `ktlint` on save (configurable, off by default)
- [ ] **Rename refactoring**: Cmd+R or F2 to rename symbol across project via LSP `workspace/rename`

---

## Ongoing / Cross-Cutting

- [ ] Write Rust unit tests for each service module as implemented
- [ ] Write frontend component tests with vitest for each panel
- [ ] Write E2E tests with tauri-driver for critical flows (open project, build, logcat)
- [ ] Set up CI/CD (GitHub Actions):
  - [ ] `cargo test` on every PR
  - [ ] `cargo clippy` lint check
  - [ ] `npm run test` for frontend
  - [ ] Build macOS artifact on main branch
- [ ] Maintain a CHANGELOG.md
- [ ] Keep PLAN.md updated as architectural decisions evolve
