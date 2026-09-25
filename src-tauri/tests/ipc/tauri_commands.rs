use keynobi_lib::models::build::BuildStatus;
use keynobi_lib::models::error::AppError;
use keynobi_lib::models::settings::{AppSettings, ProjectEntry};
use keynobi_lib::models::variant::VariantList;
use keynobi_lib::services::{adb_manager::DeviceState, build_runner::BuildState};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::State;

#[tauri::command]
async fn get_default_settings() -> Result<AppSettings, String> {
    keynobi_lib::commands::settings::get_default_settings().await
}

#[tauri::command]
async fn get_build_status(build_state: State<'_, BuildState>) -> Result<BuildStatus, String> {
    keynobi_lib::commands::build::get_build_status(build_state).await
}

#[tauri::command]
async fn select_device(
    serial: String,
    device_state: State<'_, DeviceState>,
) -> Result<(), AppError> {
    keynobi_lib::commands::device::select_device(serial, device_state).await
}

#[tauri::command]
async fn get_selected_device(
    device_state: State<'_, DeviceState>,
) -> Result<Option<String>, String> {
    keynobi_lib::commands::device::get_selected_device(device_state).await
}

#[tauri::command]
async fn get_variants_from_gradle(
    fs_state: State<'_, keynobi_lib::FsState>,
) -> Result<VariantList, String> {
    keynobi_lib::commands::variant::get_variants_from_gradle(fs_state).await
}

fn create_app() -> tauri::App<MockRuntime> {
    mock_builder()
        .manage(crate::common::isolated_build_state())
        .manage(DeviceState::new())
        .invoke_handler(tauri::generate_handler![
            get_default_settings,
            get_build_status,
            select_device,
            get_selected_device,
        ])
        .build(mock_context(noop_assets()))
        .expect("failed to build mock Tauri app")
}

fn request(cmd: &str, body: Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .expect("valid Tauri test URL"),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

fn deserialize<T: DeserializeOwned>(body: InvokeResponseBody) -> T {
    body.deserialize::<T>()
        .expect("IPC response should deserialize to expected type")
}

#[test]
fn tauri_ipc_get_default_settings_returns_camel_case_settings() {
    let app = create_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");

    let value: Value = deserialize(
        get_ipc_response(&webview, request("get_default_settings", json!({})))
            .expect("get_default_settings should succeed"),
    );

    assert_eq!(value["onboardingCompleted"], false);
    assert_eq!(value["appearance"]["uiFontSize"], 12);
}

#[test]
fn tauri_ipc_device_selection_round_trips_through_managed_state() {
    let app = create_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");

    get_ipc_response(
        &webview,
        request("select_device", json!({ "serial": "emulator-5554" })),
    )
    .expect("select_device should accept a valid serial");

    let selected: Option<String> = deserialize(
        get_ipc_response(&webview, request("get_selected_device", json!({})))
            .expect("get_selected_device should succeed"),
    );

    assert_eq!(selected.as_deref(), Some("emulator-5554"));
}

#[test]
fn tauri_ipc_device_selection_rejects_invalid_serial() {
    let app = create_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");

    let err = get_ipc_response(
        &webview,
        request("select_device", json!({ "serial": "../bad serial" })),
    )
    .expect_err("select_device should reject invalid serials");

    assert!(
        err.to_string().contains("Invalid device serial"),
        "unexpected IPC error payload: {err}"
    );
}

#[test]
fn tauri_ipc_get_build_status_reads_managed_state() {
    let app = create_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");

    let value: Value = deserialize(
        get_ipc_response(&webview, request("get_build_status", json!({})))
            .expect("get_build_status should succeed"),
    );

    assert_eq!(value["state"], "idle");
}

#[test]
fn tauri_ipc_unregistered_command_returns_error() {
    let app = create_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");

    let err = get_ipc_response(&webview, request("missing_command", json!({})))
        .expect_err("unregistered command should fail through the IPC harness");

    assert!(
        err.to_string().contains("missing_command"),
        "unexpected IPC error payload: {err}"
    );
}

/// An app whose open project is `project`.
fn create_app_with_project(project: &std::path::Path) -> tauri::App<MockRuntime> {
    let fs_state = keynobi_lib::FsState::new();
    {
        let mut fs = fs_state.0.blocking_lock();
        fs.project_root = Some(project.to_path_buf());
        fs.gradle_root = Some(project.to_path_buf());
    }
    mock_builder()
        .manage(fs_state)
        .invoke_handler(tauri::generate_handler![get_variants_from_gradle])
        .build(mock_context(noop_assets()))
        .expect("failed to build mock Tauri app")
}

/// A Gradle project whose `gradlew` leaves `marker` behind when it runs.
fn project_with_marker_gradlew(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let project = dir.canonicalize().unwrap().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("settings.gradle.kts"), "").unwrap();
    let marker = project.join("gradlew-ran");
    let gradlew = project.join("gradlew");
    std::fs::write(
        &gradlew,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&gradlew, std::fs::Permissions::from_mode(0o755)).unwrap();
    (project, marker)
}

#[test]
fn tauri_ipc_variant_discovery_runs_gradle_only_for_a_trusted_project() {
    crate::common::isolate_data_dir();
    let dir = tempfile::tempdir().unwrap();
    let (project, marker) = project_with_marker_gradlew(dir.path());
    let app = create_app_with_project(&project);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to build mock webview");

    let err = get_ipc_response(&webview, request("get_variants_from_gradle", json!({})))
        .expect_err("an untrusted project must be refused");
    assert!(err.to_string().contains("not trusted"), "{err}");
    assert!(!marker.exists(), "gradlew ran for an untrusted project");

    keynobi_lib::services::settings_manager::mutate_settings(|s| {
        s.recent_projects.push(ProjectEntry {
            id: project.to_string_lossy().into_owned(),
            path: project.to_string_lossy().into_owned(),
            trusted: Some(true),
            ..Default::default()
        })
    })
    .unwrap();
    // The fake gradlew prints no variants, so discovery still fails, but only
    // after running it.
    let _ = get_ipc_response(&webview, request("get_variants_from_gradle", json!({})));
    assert!(marker.exists(), "gradlew did not run for a trusted project");
}
