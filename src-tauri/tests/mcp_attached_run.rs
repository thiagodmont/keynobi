//! An agent runs a run configuration through a session attached to the app.
//!
//! The app's side of an attached session runs in this test process
//! ([`TestApp`]), so its adb, aapt2, and JDK come from this process's
//! settings. This crate gives them the sandbox's fake SDK
//! ([`use_sandbox_settings`]); it runs in its own process so that no other
//! test shares, or rewrites, those settings.

mod common;
#[allow(dead_code)]
mod headless;

use headless::{
    gradlew_writing_the_debug_apk, one_emulator_adb_that_installs_and_launches,
    write_run_configurations, Sandbox, TestApp,
};
use keynobi_lib::models::build::{AgentActor, BuildActor};
use keynobi_lib::models::run_configuration::DeployPhase;
use keynobi_lib::services::{adb_manager, settings_manager};
use serde_json::json;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Give this test process `sandbox`'s settings: its fake SDK and JDK, and
/// its project registry. The data dir is one per test process, so every test
/// here must use one sandbox.
fn use_sandbox_settings(sandbox: &Sandbox) {
    static SDK: OnceLock<PathBuf> = OnceLock::new();
    assert_eq!(
        SDK.get_or_init(|| sandbox.sdk().to_path_buf()),
        sandbox.sdk(),
        "this test process already uses another sandbox's settings"
    );
    common::isolate_data_dir();
    let settings = std::fs::read(sandbox.home.join(".keynobi").join("settings.json"))
        .expect("the sandbox has settings");
    settings_manager::with_data_lock(|| {
        std::fs::write(settings_manager::data_dir().join("settings.json"), settings)
    })
    .expect("take the data lock")
    .expect("write this process's settings");
    let (loaded, _) = settings_manager::load_settings();
    assert_eq!(
        adb_manager::get_adb_path(&loaded),
        sandbox.sdk().join("platform-tools").join("adb"),
        "the app would not run the sandbox's adb"
    );
}

#[test]
fn an_attached_agents_run_goes_through_the_app_and_shows_its_phases_there() {
    let sandbox = Sandbox::new();
    write_run_configurations(
        &sandbox,
        json!(true),
        json!([{ "name": "Default", "module": ":app", "variant": "debug" }]),
        json!({ "Default": { "target": { "kind": "serial", "serial": "emulator-5554" } } }),
    );
    one_emulator_adb_that_installs_and_launches(&sandbox);
    let gradlew_args = gradlew_writing_the_debug_apk(&sandbox);
    use_sandbox_settings(&sandbox);
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    let mut client = sandbox.start();
    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "attached", "{info}");
    let session_id = app.registry.sessions()[0].id;

    let ran = client.call_tool_json("run_run_configuration", json!({ "name": "Default" }));

    assert_eq!(ran["outcome"], "done", "{ran}");
    assert_eq!(ran["package"], "com.example.sandbox", "{ran}");
    assert!(
        std::fs::read_to_string(&gradlew_args)
            .unwrap()
            .contains(":app:assembleDebug"),
        "gradlew did not run the configuration's task"
    );
    let agent = BuildActor::Agent(AgentActor {
        session_id: Some(session_id),
        client_name: Some("keynobi-headless-test".into()),
        standalone: false,
    });

    // The app ran the build, as the agent's.
    let build_id = ran["build_id"].as_u64().expect("a build id") as u32;
    let record = app
        .block_on(async { app.build_state.inner.lock().await.history.clone() })
        .into_iter()
        .find(|r| r.id == build_id)
        .expect("the build is in the app's history");
    assert_eq!(record.origin, Some(agent.clone()));
    assert!(
        !sandbox.home.join(".keynobi/build-history.json").exists(),
        "the relaying process recorded a build"
    );

    // The app installed and launched with the sandbox's adb.
    let calls = sandbox.adb_calls();
    assert!(
        calls
            .iter()
            .any(|c| c.contains("-s emulator-5554") && c.contains(" install ")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("am start -W -n com.example.sandbox/")),
        "{calls:?}"
    );

    // The app got every phase, as the agent's.
    let phases = app.deploy_phases.lock().unwrap().clone();
    assert_eq!(
        phases.iter().map(|e| e.phase).collect::<Vec<_>>(),
        [
            DeployPhase::Building,
            DeployPhase::Installing,
            DeployPhase::Launching,
            DeployPhase::Done
        ],
        "{phases:?}"
    );
    assert!(phases.iter().all(|e| e.origin == agent), "{phases:?}");
    assert!(phases.iter().all(|e| e.name == "Default"), "{phases:?}");
    assert_eq!(phases[1].build_id, Some(build_id));
    assert!(
        phases[1]
            .steps
            .iter()
            .any(|s| s.starts_with("adb install ")),
        "{phases:?}"
    );
    assert!(
        phases[2]
            .steps
            .contains(&"adb shell am start -W (package: com.example.sandbox)".to_string()),
        "{phases:?}"
    );
}
