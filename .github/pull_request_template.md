## Summary

<!-- What does this PR change and why? -->

## How to test

<!-- Steps or panels/shortcuts to verify (especially for UI). -->

## Checklist

- [ ] `npm run lint` and `npm run typescript:check` and `npm run test`
- [ ] `cd src-tauri && cargo test --lib --tests` and `cargo clippy -- -D warnings` (and telemetry matrix if you touched Rust: `cargo clippy --features telemetry -- -D warnings`, `cargo test --lib --tests --features telemetry`)
- [ ] If Rust models / `ts-rs` exports changed: `npm run generate:bindings` and committed `src/bindings/`
- [ ] User-visible behavior or shortcuts: updated [docs/USER_MANUAL.md](../docs/USER_MANUAL.md)
- [ ] New cross-cutting patterns: noted for maintainers in [CONTRIBUTING.md](../CONTRIBUTING.md) / [AGENTS.md](../AGENTS.md) session rules (`docs/CODE_PATTERN.md`, `docs/DOMAIN_PATTERNS.md`, or `docs/BEST_PRACTICES.md` as appropriate)
- [ ] UI change: screenshot(s) attached (if helpful for reviewers)
