pub mod commands;
pub mod models;
pub mod services;
pub mod utils;

use commands::build::{
    cancel_build, clear_build_history, find_apk_path, get_build_errors, get_build_history,
    get_build_log_entries, get_build_status, get_package_name_from_apk, run_gradle_task,
};
use commands::device::{
    create_avd_device, delete_avd_device, download_system_image_cmd, get_exit_reasons,
    get_selected_device, install_apk_on_device, launch_app_on_device, launch_avd, list_adb_devices,
    list_available_system_images_cmd, list_avd_devices, list_device_definitions_cmd,
    list_installed_builds, list_system_images_cmd, refresh_devices, select_device,
    start_device_polling, stop_app_on_device, stop_avd, stop_device_polling, wipe_avd_data_cmd,
};
use commands::file_system::{
    get_application_id, get_gradle_root, get_last_active_project, get_project_app_info,
    get_project_root, list_projects, open_project, pin_project, remove_project, rename_project,
    save_project_app_info, set_project_trust, update_project_meta,
};
use commands::health::run_health_checks;
use commands::logcat::{
    clear_logcat, export_logcat, get_logcat_context_entries, get_logcat_entries, get_logcat_stats,
    get_logcat_status, list_logcat_packages, new_logcat_state, retrace_crash, set_logcat_filter,
    start_logcat, stop_logcat,
};
use commands::mcp::{
    clear_mcp_activity, get_mcp_activity, get_mcp_server_status, get_mcp_setup_status,
};
use commands::settings::*;
use commands::studio::open_in_studio;
use commands::telemetry::send_native_sentry_test_event;
use commands::ui_hierarchy::dump_ui_hierarchy;
use commands::variant::{get_variants_from_gradle, get_variants_preview, set_active_variant};
use models::log_entry::LogEntry;
use models::settings as app_settings_model;
use services::adb_manager::DeviceState;
use services::build_runner::BuildState;
use services::process_manager::ProcessManager;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

// ── Per-concern state ─────────────────────────────────────────────────────────

/// File-system state: the open project root and detected Gradle root.
///
/// Accessed by all file commands. The Mutex is held only for the brief
/// moment needed to read `project_root` — never during I/O operations.
pub struct FsState(pub Arc<Mutex<FsStateInner>>);

pub struct FsStateInner {
    pub project_root: Option<PathBuf>,
    /// The detected Gradle project root (ancestor with `settings.gradle(.kts)`).
    /// Used as the build workspace root and security boundary.
    /// Falls back to `project_root` when no Gradle root is found.
    pub gradle_root: Option<PathBuf>,
}

impl FsState {
    pub fn new() -> Self {
        FsState(Arc::new(Mutex::new(FsStateInner {
            project_root: None,
            gradle_root: None,
        })))
    }
}

impl Clone for FsState {
    fn clone(&self) -> Self {
        FsState(self.0.clone())
    }
}

impl Default for FsState {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded ring-buffer of structured log entries shared across all log sources
/// (Logcat, build output).  Protected by a `tokio::sync::Mutex` so it can be
/// safely written from async tasks and read from Tauri commands without holding
/// the lock across I/O.
pub type LogBuffer = Arc<Mutex<VecDeque<LogEntry>>>;

/// Maximum entries kept in [`LogBuffer`] before the oldest is evicted.
pub const MAX_LOG_ENTRIES: usize = 50_000;

fn cleanup_old_logs(log_dir: &std::path::Path, retention_days: u32) {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(
            u64::from(retention_days) * 86_400,
        ))
        .unwrap_or(std::time::UNIX_EPOCH);

    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return;
    };
    let active = services::monitor::active_log_file_name();
    for entry in entries.flatten() {
        let path = entry.path();
        // Only touch files matching app.log.* pattern.
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with(services::monitor::LOG_FILE_PREFIX) || name == active {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff && std::fs::remove_file(&path).is_ok() {
                    tracing::info!("Removed old log file: {}", path.display());
                }
            }
        }
    }
}

/// Drops the log writer guard on exit so lines still queued for the file are written.
fn release_log_guard_on_exit(
    event: &tauri::RunEvent,
    guard: &mut Option<tracing_appender::non_blocking::WorkerGuard>,
) {
    if let tauri::RunEvent::Exit = event {
        drop(guard.take());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── Logging setup ─────────────────────────────────────────────────────────
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".keynobi")
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // Daily rotating file appender. Old files are named app.log.YYYY-MM-DD.
    let file_appender =
        tracing_appender::rolling::daily(&log_dir, services::monitor::LOG_FILE_PREFIX);
    let (non_blocking_file, file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        tracing_subscriber::EnvFilter::try_from_env("KEYNOBI_LOG").unwrap_or_else(|_| {
            if cfg!(debug_assertions) {
                tracing_subscriber::EnvFilter::new("debug")
            } else {
                tracing_subscriber::EnvFilter::new("warn")
            }
        });

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking_file)
                .with_ansi(false)
                .with_target(true),
        )
        .with(
            // In debug builds, also log to stderr for developer convenience.
            #[cfg(debug_assertions)]
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(false),
            #[cfg(not(debug_assertions))]
            tracing_subscriber::layer::Identity::new(),
        )
        .with(env_filter)
        .init();

    // Held until RunEvent::Exit; the event loop exits the process without unwinding `run()`.
    let mut file_guard = Some(file_guard);

    // ── Sentry (optional) ─────────────────────────────────────────────────────
    // Initialized after logging so startup diagnostics still hit the log file first.
    // Requires `--features telemetry`, compile-time `SENTRY_DSN`, and
    // `settings.telemetry.enabled` (see `services::telemetry_sentry`).
    #[cfg(feature = "telemetry")]
    let _sentry_guard = {
        let (settings, _) = services::settings_manager::load_settings();
        services::telemetry_sentry::init_if_enabled(&settings)
    };

    let log_dir = log_dir.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(FsState::new())
        .manage(BuildState::new())
        .manage(ProcessManager::new())
        .manage(DeviceState::new())
        .manage(new_logcat_state())
        .manage(Arc::new(Mutex::new(VecDeque::<LogEntry>::new())) as LogBuffer)
        .setup(move |app| {
            // MCP clients attach over the app's socket; see services::mcp_attach.
            let mcp_sessions = {
                let handle = app.handle().clone();
                services::mcp_sessions::McpSessionRegistry::with_listener(move |sessions| {
                    let _ = handle.emit(services::mcp_sessions::SESSIONS_CHANGED_EVENT, sessions);
                })
            };
            app.manage(mcp_sessions.clone());
            services::mcp_sessions::remove_legacy_pid_file();
            tauri::async_runtime::spawn(services::mcp_attach::start_app_listener(
                app.handle().clone(),
                mcp_sessions,
            ));

            let (settings, settings_corrupted) = services::settings_manager::load_settings();

            let ring_cap = app_settings_model::clamp_logcat_ring_capacity_usize(
                settings.logcat.ring_max_entries,
            );
            let logcat_state = app.state::<services::logcat::LogcatState>();
            tauri::async_runtime::block_on(async {
                let mut st = logcat_state.lock().await;
                st.store.set_capacity(ring_cap);
            });

            if settings_corrupted {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Small delay to let the frontend finish mounting before showing Toast.
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    if let Some(win) = handle.get_webview_window("main") {
                        let _ = win.emit("settings:corrupted", ());
                    }
                });
            }

            // Clean up log files older than the configured retention period.
            // cleanup_old_logs is synchronous — no spawn needed.
            cleanup_old_logs(&log_dir, settings.advanced.log_retention_days);

            // Rotate build log files at startup: age, orphan, and size-cap passes.
            if let Err(e) = services::build_runner::rotate_persisted_build_logs(
                settings.build.build_log_retention_days,
                settings.build.build_log_max_folder_mb,
            ) {
                tracing::warn!("Failed to rotate build logs: {e}");
            }

            // Spawn monitor: polls memory + log folder size every 5s.
            {
                let handle = app.handle().clone();
                let log_dir_monitor = log_dir.clone();
                let log_max_bytes = u64::from(settings.advanced.log_max_size_mb) * 1024 * 1024;
                tauri::async_runtime::spawn(async move {
                    services::monitor::run_monitor(handle, log_dir_monitor, log_max_bytes).await;
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    // One overall shutdown budget: wait briefly for the
                    // frontend to persist any settings change still inside
                    // its 500 ms debounce window (it invokes
                    // notify_settings_flushed when done), then cancel the
                    // build and stop streaming. If the ack never arrives
                    // (webview already gone), don't stall the whole budget.
                    let shutdown = async {
                        let _ = tokio::time::timeout(
                            std::time::Duration::from_secs(2),
                            commands::settings::SETTINGS_FLUSH_ACK.notified(),
                        )
                        .await;

                        // Cancel any running Gradle build (whoever started
                        // it), then answer attached agents and close their
                        // sessions; they continue standalone.
                        let build_state = app.state::<BuildState>();
                        let process_manager = app.state::<ProcessManager>();
                        let registry = app
                            .state::<services::mcp_sessions::McpSessionRegistry>()
                            .inner()
                            .clone();
                        services::mcp_attach::quit_sessions(
                            &build_state,
                            &process_manager,
                            &registry,
                        )
                        .await;

                        // Stop logcat streaming (best-effort).
                        let logcat_state = app.state::<services::logcat::LogcatState>();
                        services::logcat::request_stop(&logcat_state).await;

                        // Stop ADB device polling.
                        let device_state = app.state::<DeviceState>();
                        device_state.0.lock().await.stop_polling();
                    };

                    if tokio::time::timeout(std::time::Duration::from_secs(3), shutdown)
                        .await
                        .is_err()
                    {
                        tracing::warn!("Graceful shutdown timed out after 3s — forcing close");
                    }

                    // Allow the window to actually close.
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.destroy();
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            // File system
            open_project,
            get_project_root,
            get_gradle_root,
            get_application_id,
            // Project registry
            list_projects,
            remove_project,
            pin_project,
            set_project_trust,
            get_last_active_project,
            update_project_meta,
            rename_project,
            // Project App Info
            get_project_app_info,
            save_project_app_info,
            // Settings
            get_settings,
            save_settings,
            notify_settings_flushed,
            get_default_settings,
            reset_settings,
            send_native_sentry_test_event,
            detect_sdk_path,
            detect_java_path,
            // Health
            run_health_checks,
            // Build
            run_gradle_task,
            cancel_build,
            get_build_status,
            get_build_errors,
            get_build_history,
            clear_build_history,
            get_build_log_entries,
            find_apk_path,
            get_package_name_from_apk,
            // Variants
            get_variants_preview,
            get_variants_from_gradle,
            set_active_variant,
            // Devices
            list_adb_devices,
            refresh_devices,
            select_device,
            get_selected_device,
            install_apk_on_device,
            list_installed_builds,
            launch_app_on_device,
            stop_app_on_device,
            get_exit_reasons,
            list_avd_devices,
            launch_avd,
            stop_avd,
            start_device_polling,
            stop_device_polling,
            list_system_images_cmd,
            list_device_definitions_cmd,
            create_avd_device,
            delete_avd_device,
            wipe_avd_data_cmd,
            list_available_system_images_cmd,
            download_system_image_cmd,
            dump_ui_hierarchy,
            // Logcat
            start_logcat,
            stop_logcat,
            clear_logcat,
            export_logcat,
            get_logcat_context_entries,
            get_logcat_entries,
            get_logcat_status,
            list_logcat_packages,
            set_logcat_filter,
            get_logcat_stats,
            retrace_crash,
            // MCP Server
            get_mcp_setup_status,
            get_mcp_activity,
            get_mcp_server_status,
            clear_mcp_activity,
            // Android Studio integration
            open_in_studio,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(registry) =
                    app.try_state::<services::mcp_sessions::McpSessionRegistry>()
                {
                    services::mcp_attach::remove_app_socket(&registry);
                }
                // Every exit path, including quit from the menu: stop the
                // processes still running, within a bound.
                if let Some(process_manager) = app.try_state::<ProcessManager>() {
                    tauri::async_runtime::block_on(
                        process_manager.shutdown_all(services::process_manager::SHUTDOWN_GRACE),
                    );
                }
            }
            release_log_guard_on_exit(&event, &mut file_guard)
        });
}

#[cfg(test)]
mod runtime_safety_tests {
    /// Enforces that `lib.rs` never uses `tokio::spawn` directly.
    ///
    /// ## Why this rule exists
    ///
    /// Tauri's `.setup()` and `on_window_event` callbacks run synchronously from
    /// macOS's `applicationDidFinishLaunching` — before the Tokio runtime is active.
    /// Calling `tokio::spawn` there panics at startup with:
    ///
    ///   "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
    ///
    /// Always use `tauri::async_runtime::spawn` in `lib.rs`. It delegates to Tauri's
    /// own static runtime handle, which is initialized before those callbacks fire.
    ///
    /// Services and commands that are called from within async contexts may continue
    /// using `tokio::spawn` freely — this constraint applies only to `lib.rs`.
    #[test]
    fn lib_rs_does_not_use_tokio_spawn_directly() {
        let source = include_str!("lib.rs");

        // Only scan production code — stop before #[cfg(test)] so the test's
        // own strings (which mention "tokio::spawn") don't trigger a false positive.
        let violations: Vec<(usize, &str)> = source
            .lines()
            .take_while(|l| !l.trim_start().starts_with("#[cfg(test)]"))
            .enumerate()
            .filter(|(_, line)| {
                let trimmed = line.trim();
                !trimmed.starts_with("//")              // skip comment lines
                    && trimmed.contains("tokio::spawn")
                    && !trimmed.contains("tokio::spawn_blocking") // allowed: blocking pool
            })
            .collect();

        assert!(
            violations.is_empty(),
            "lib.rs must not use tokio::spawn ({} violation(s) found):\n{}\n\n\
             Use tauri::async_runtime::spawn instead. See the doc comment on this \
             test for the full explanation.",
            violations.len(),
            violations
                .iter()
                .map(|(n, l)| format!("  line {}: {}", n + 1, l.trim()))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[cfg(test)]
mod log_file_tests {
    use super::*;
    use std::io::Write;

    /// Writer slow enough that queued lines are still pending right after they are sent.
    /// The guard waits at most 1 s on drop, so keep the total well under that even
    /// on CI runners that stretch short sleeps.
    struct SlowWriter(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for SlowWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(std::time::Duration::from_millis(25));
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn exit_event_drops_log_guard_and_flushes_queued_lines() {
        let written = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (mut writer, guard) = tracing_appender::non_blocking(SlowWriter(written.clone()));
        let mut guard = Some(guard);

        for i in 0..2 {
            writeln!(writer, "line {i}").unwrap();
        }

        release_log_guard_on_exit(&tauri::RunEvent::Ready, &mut guard);
        assert!(guard.is_some(), "non-exit events must keep the guard");

        release_log_guard_on_exit(&tauri::RunEvent::Exit, &mut guard);
        assert!(guard.is_none());
        let text = String::from_utf8(written.lock().unwrap().clone()).unwrap();
        assert_eq!(
            text.lines().count(),
            2,
            "queued lines were not flushed: {text:?}"
        );
    }

    #[test]
    fn cleanup_old_logs_keeps_the_active_log() {
        let dir = tempfile::tempdir().unwrap();
        let active = services::monitor::active_log_file_name();
        std::fs::write(dir.path().join(&active), b"active").unwrap();
        std::fs::write(dir.path().join("app.log.2000-01-01"), b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Zero retention puts the cutoff at now, so every existing file is past it.
        cleanup_old_logs(dir.path(), 0);

        assert!(dir.path().join(&active).exists());
        assert!(!dir.path().join("app.log.2000-01-01").exists());
    }
}
