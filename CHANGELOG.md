# Changelog

All notable user-facing changes to Keynobi are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

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
