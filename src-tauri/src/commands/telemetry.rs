//! Dev / support helpers for optional Sentry (Rust). Commands are always registered;
//! behavior depends on the `telemetry` Cargo feature and runtime settings.

#[cfg(feature = "telemetry")]
#[tauri::command]
pub fn send_native_sentry_test_event() -> Result<(), String> {
    use crate::services::settings_manager;
    use sentry::{Hub, Level};

    if option_env!("SENTRY_DSN").is_none() {
        return Err(
            "This build has no compile-time SENTRY_DSN. Rebuild with SENTRY_DSN set and --features telemetry."
                .into(),
        );
    }

    let (settings, _) = settings_manager::load_settings();
    if !settings.telemetry.enabled {
        return Err(
            "Turn on Anonymous crash reporting in Settings first. If you just enabled it, restart the app so the native client initializes."
                .into(),
        );
    }

    if Hub::main().client().is_none() {
        return Err(
            "Native Sentry is not active. Enable Anonymous crash reporting and restart the app (native Sentry only starts at launch when telemetry is on)."
                .into(),
        );
    }

    sentry::capture_message(
        "Keynobi native Sentry test (command palette)",
        Level::Info,
    );
    Ok(())
}

#[cfg(not(feature = "telemetry"))]
#[tauri::command]
pub fn send_native_sentry_test_event() -> Result<(), String> {
    Err("This binary was built without the telemetry Cargo feature.".into())
}
