use keynobi_lib::models::build::BuildStatus;
use keynobi_lib::models::error::AppError;
use keynobi_lib::models::settings::AppSettings;
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

fn create_app() -> tauri::App<MockRuntime> {
    mock_builder()
        .manage(BuildState::new())
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
        url: "http://tauri.localhost"
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
