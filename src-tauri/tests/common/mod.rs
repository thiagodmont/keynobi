//! Helpers shared by the integration test crates.

use keynobi_lib::services::build_runner::BuildState;
use keynobi_lib::services::settings_manager;
use std::sync::Once;

/// Point all Keynobi persistence (settings, build history, build logs, MCP
/// activity) at a throwaway directory for this test process.
///
/// Integration tests link the library without `cfg(test)`, so they do not get
/// the automatic isolation unit tests have. Call this before constructing any
/// state that loads or saves persisted data.
pub fn isolate_data_dir() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = tempfile::Builder::new()
            .prefix("keynobi-itest-")
            .tempdir()
            .expect("create integration-test data dir")
            .keep();
        assert!(
            settings_manager::set_data_dir_override(dir),
            "data dir override was already installed"
        );
    });
}

/// A `BuildState` whose history is loaded from and persisted to the isolated
/// test data directory.
pub fn isolated_build_state() -> BuildState {
    isolate_data_dir();
    BuildState::new()
}
