# Changelog

All notable user-facing changes to Keynobi are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

## [0.1.29] — 2026-09-25

### Breaking Changes
- restart_app no longer clears app data by default (#268)

### Added
- require project trust before running project build code (#297)
- declare read-only, destructive, and open-world annotations on every tool (#271)

### Fixed
- share settings and build history safely between the app and MCP servers (#293)
- keep entry IDs unique for the life of the process (#296)
- scope app-changing device tools to the project and stop cutting off wireless adb (#295)
- resolve one JDK for builds, GUI Health, and MCP health (#294)
- follow only this window's build run and stop deploys when the project changes (#292)
- time out hung adb and SDK tool calls instead of blocking forever (#290)
- parse Kotlin 2, KSP, lint, R8, and AAPT2 diagnostics from real build output (#291)
- install only the requested variant's APK and never launch a guessed package (#289)
- report a build's exit even when a descendant keeps its output open (#288)
- a cancelled build that finishes late no longer takes over the next one (#287)
- export logcat from Rust and remove webview filesystem access (#285)
- bound the bytes kept per line of build and logcat output (#283)
- never rotate away the active log and flush it on exit (#282)
- show a load error instead of "No log saved" when a past build's log fails to load (#281)
- report a device whose state changes without its serial changing (#280)
- a failed device or variant selection no longer undoes a newer one (#279)
- close dialogs with Escape and keep focus inside them (#278)
- block Play track promotion tasks for agents (#273)
- reduce the webview's filesystem access to saving a chosen file (#272)
- block Gradle options and publish/upload/uninstall tasks for agents (#270)
- quote every adb shell argument for the device shell (#269)
- running the Rust test suite could overwrite local build history (#267)
- monotonic logcat IDs, wipe false success, dead auto-install setting, variant persistence, capped error buffer (#204)
- stale build-history race, wipe false success, MCP history gap, variant timeout, cache cap (#203)
- close five verified defects across MCP server, adb, and build flow (#191)
- install rustfmt and reformat with the locked prettier

### Changed
- Add sync-version.test.mjs drift guard for the three version files (#251)
- Make the MCP tool `find_apk_path` honour the build variant persisted… (#249)
- Fix setAgeInQuery/setPackageInQuery to strip negated tokens whole (#219)
- Fix ts-rs export path for LogEntry/LogLevel bindings (#217)
- Make the CI TypeScript-bindings staleness guard able to actually fail: s (#235)

---

## [0.1.28] — 2026-08-18

### Fixed
- close production defects found in self-review of the hardening work
- clear pre-existing clippy lints blocking --all-targets
- idempotent listeners, selection rollback, project-switch guard
- generation-scoped stream lifecycle, bounded reconnect, drop counter
- guard delayed SIGKILL against recycled PIDs
- harden build finalization, listener, and project-root lifecycles

### Changed
- shared validators, settings cache, stable project ids
- unify UI and MCP build paths behind shared runner

---

## [0.1.27] — 2026-07-01

### Fixed
- new improviments (#140)

### Changed
- fix improviments (#139)

---

## [0.1.26] — 2026-05-11

### Added
- expand log (#73)
- apply ds (#72)
- design system (#71)
- refactor logcat (#70)
- fix and improvements (#69)

---

## [0.1.25] — 2026-05-08

### Added
- add lifecycle visibility filter (#67)

### Changed
- switch condition and package mine fix (#68)
- log improvements (#66)

---

## [0.1.24] — 2026-05-06

### Added
- improviments (#65)

---

## [0.1.23] — 2026-05-04

### Added
- log mode (#46)

---

## [0.1.22] — 2026-05-03

### Added
- always on top (#45)

---

## [0.1.21] — 2026-04-30

### Added
- mcp and refactors (#44)
- logcat click filter (#43)

---

## [0.1.19] — 2026-04-29

### Added
- implement update notification system and status indicator (#29)

---

## [0.1.18] — 2026-04-24

### Added
- show enclosed group container in QueryBar when 2+ OR groups exist
- normalize group-boundary parens in buildQueryBarPillGroups
- support optional outer parens in parseFilterGroups

### Fixed
- cap group box width to prevent inline-flex expansion during inline edit
- depth-balance check in stripGroupParens

---

## [0.1.17] — 2026-04-23

---

## [0.1.16] — 2026-04-17

### Added
- enhance user experience with new features and documentation updates

---

## [0.1.15] — 2026-04-17

### Added
- implement code-signing and notarization for DMGs in CI

### Fixed
- enhance documentation and workflow for signing secrets

---

## [0.1.14] — 2026-04-17

### Added
- implement code-signing and notarization for DMGs in CI

---

## [0.1.13] — 2026-04-16

### Added
- add defaultVariant to VariantList and update related logic
- implement auto-scroll settings for log panels
- enhance logcat settings and UI with keyboard navigation and buffer management
- move to a design system (#3)
- persist and surface active build variant in set_active_variant / list_build_variants
- infer product flavors from APK output directory when not declared in build file
- fallback to convention plugin .kt files for SDK level detection

### Fixed
- surface ADB recovery hint when devices are offline in list_devices
- remove commandLog from UI hierarchy responses; trim layoutContext to wmSize+wmDensity

---

## [0.1.12] — 2026-04-13

### Added
- add Layout Viewer for UI Automator accessibility hierarchy

### Fixed
- update ESLint config and improve LayoutViewerPanel and LayoutWireframe components

---

## [0.1.11] — 2026-04-10

### Added
- auto-apply package:mine filter after successful deploy
- unified filter zone layout (Option B)

---

## [0.1.10] — 2026-04-10

### Added
- implement caching for loadVariants and add force refresh option

---

## [0.1.9] — 2026-04-10

### Added
- enhance build history management with project scoping

### Fixed
- initialize projectRoot in BuildRecord tests

---

## [0.1.8] — 2026-04-10

### Added
- add build log retention and folder size settings to Settings panel
- switch LogViewer to historical log when a history entry is selected
- add getBuildLogEntries API binding; export lineToLogEntry from store; fix BuildSettings defaults
- add getBuildLogEntries API binding; export lineToLogEntry from store
- add get_build_log_entries command; call rotate_build_logs at startup
- persist structured build log per build; rotate on finalization
- add build_log_retention_days and build_log_max_folder_mb to BuildSettings
- wire clear-history button in BuildPanel
- add onClear prop and trash button to BuildHistoryPanel header
- add clearBuildHistory store action and API binding
- add clear_build_history Tauri command
- add history side panel to BuildPanel — flex-row layout
- add BuildHistoryPanel component showing past builds in sidebar
- implement loadVariants function to coalesce concurrent calls and add reloadVariantsAndRestoreMeta for project state restoration

### Fixed
- cancel stale async log fetch on selection change; hide clear button in historical view
- use tokio::fs and proper NotFound handling in get_build_log_entries
- combine age+orphan rotation passes to prevent double-delete; guard retention_days=0
- handle error in handleClearHistory with toast
- reset next_id to 1 when clearing history
- initialize next_id from max history id to prevent collisions after restart
- resolve launcher activity from device instead of guessing
- prevent double finalizeBuild on cancel; gate VirtualList auto-scroll microtask
- snapshot errors before cancelBuildState to avoid fragile post-cancel read
- record cancelled builds in history, open Studio from errors, richer logging
- rename renderItem prop to renderRow per spec
- move clear_build_log before spawn to eliminate race condition

### Changed
- clean up code and improve readability
- remove outdated build panel design document; implement new build history and log management features
- replace For loop with VirtualList — only visible rows in DOM
- batch-flush build log lines every 50ms instead of one update per line
- replace 200ms polling loop in run_task with tokio oneshot channel
- remove unused BuildSettings.buildVariant and selectedDevice fields
- remove Gradle flags from settings and enhance error formatting

---

## [0.1.7] — 2026-04-08

### Added
- full 20K backfill and detail panel integration
- add Restart button to logcat toolbar
- add scroll compensation for smooth eviction
- wire VirtualList handle, fix cleared listener scroll, update ↓ button
- add LogEntryDetailPanel component for row detail inspection
- add scrollCompensate prop and VirtualListHandle imperative API
- raise backend backfill cap from 10K to 20K entries

### Fixed
- remove unused copyRow, fix solid/reactivity lint warnings
- fix LogEntryDetailPanel SolidJS lifecycle and reactivity patterns
- reset PipelineContext on clear to prevent stale enrichment

### Changed
- extract shared LEVEL_CONFIG to logcat-levels.ts
- remove editor settings and update related components

---

## [0.1.6] — 2026-04-08

### Changed
- remove editor settings and update related components

---

## [0.1.5] — 2026-04-08

### Added
- add TypeScript type checking step to CI workflow
- integrate changelog generation into npm run release
- add changelog generation module with full test coverage
- add toast notification system for user-visible errors

### Fixed
- use void operator for SolidJS reactive dependency access
- resolve TypeScript errors in App.tsx and sentry-web tests
- fix prependToChangelog double-blank and write CHANGELOG.md before diff display
- defer CHANGELOG.md write to just before git add to prevent split state
- apply release-commit exclusion regardless of commit type in filterUserFacing
- use proper error message extraction in toast notifications
- surface swallowed errors as toast notifications
- cancel toast timers on early dismiss to prevent accumulation

### Changed
- extend LazyLock optimization to extract functions
- compile version regexes once via LazyLock
- rename project from Android Dev Companion to Keynobi
- store telemetry setting in a variable for clarity

---

## [0.1.4] — 2026-04-08

### Added
- Sentry integration for browser and native crash reporting (opt-in via Settings)
- Memory usage monitor in the status bar
- Toast notifications for user-visible error feedback

### Fixed
- Renamed project from "Android Dev Companion" to Keynobi throughout

---

## [0.1.3] and earlier

See [GitHub Releases](https://github.com/thiagodmont/keynobi/releases) for earlier release notes.
