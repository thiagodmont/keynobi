//! End-to-end tests of the headless MCP server binary. See `headless/mod.rs`.

mod headless;

use headless::Sandbox;
use serde_json::json;

#[test]
fn server_completes_the_handshake_and_lists_its_tools() {
    let sandbox = Sandbox::new();
    let mut client = sandbox.start();

    let tools = client.tool_names();
    for expected in ["run_gradle_task", "get_build_errors", "list_devices"] {
        assert!(
            tools.iter().any(|t| t == expected),
            "{expected} missing from {tools:?}"
        );
    }
}

#[test]
fn server_keeps_its_data_in_the_sandbox_home() {
    let sandbox = Sandbox::new();
    let mut client = sandbox.start();
    client.tool_names();

    // The headless server logs its lifecycle to the data dir; finding the log
    // under the sandbox HOME shows the child never resolved the real one.
    let activity = sandbox.home.join(".keynobi").join("mcp-activity.jsonl");
    assert!(
        activity.is_file(),
        "no activity log in the sandbox data dir: {:?}",
        std::fs::read_dir(sandbox.home.join(".keynobi"))
            .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
    );
}

#[test]
fn list_devices_uses_the_adb_from_the_configured_sdk() {
    let sandbox = Sandbox::new();
    sandbox.write_adb(
        r#"case "$*" in
  "devices -l")
    echo 'List of devices attached'
    echo 'emulator-5554          device product:sdk_gphone64 model:Pixel_9 device:emu64a transport_id:1'
    ;;
  *"ro.build.version.sdk"*) echo 35 ;;
  *"ro.build.version.release"*) echo 15 ;;
esac"#,
    );
    let mut client = sandbox.start();

    let out = client.call_tool("list_devices", json!({}));

    assert!(!out.is_error, "{}", out.text);
    assert!(out.text.contains("emulator-5554"), "{}", out.text);
    assert!(
        sandbox.adb_calls().iter().any(|c| c == "devices -l"),
        "fake adb was not called: {:?}",
        sandbox.adb_calls()
    );
}

#[test]
fn run_gradle_task_runs_the_project_wrapper_and_reports_its_errors() {
    let sandbox = Sandbox::new();
    sandbox.write_gradlew(
        r#"echo "> Task :app:compileDebugKotlin FAILED"
echo "e: file:///project/app/src/main/java/com/example/Main.kt:10:5: Unresolved reference: foo"
echo ""
echo "FAILURE: Build failed with an exception."
echo "BUILD FAILED in 1s"
exit 1"#,
    );
    let mut client = sandbox.start();

    let build = client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));
    assert!(
        build.is_error,
        "a failed build must be a tool error: {}",
        build.text
    );

    let errors = client.call_tool("get_build_errors", json!({}));
    assert!(
        errors.text.contains("Main.kt") && errors.text.contains("Unresolved reference"),
        "{}",
        errors.text
    );
}

#[test]
fn health_project_info_and_builds_use_the_jdk_from_gradle_properties() {
    let sandbox = Sandbox::new();
    let jdk = sandbox.home.join("jdks").join("21");
    headless::write_fake_jdk(&jdk, "21.0.8");
    std::fs::write(
        sandbox.project.join("gradle.properties"),
        format!("org.gradle.java.home={}\n", jdk.display()),
    )
    .unwrap();
    let seen = sandbox.project.join("java-home.txt");
    sandbox.write_gradlew(&format!(
        "echo \"$JAVA_HOME\" > '{}'\necho 'BUILD SUCCESSFUL in 1s'",
        seen.display()
    ));
    let mut client = sandbox.start();

    let expected = json!({
        "java_home": jdk.to_string_lossy(),
        "source": "projectGradleProperties",
        "major_version": 21,
    });
    for (tool, pointer) in [
        ("run_health_check", "/checks/java"),
        ("get_project_info", "/java"),
    ] {
        let out = client.call_tool(tool, json!({}));
        assert!(!out.is_error, "{}", out.text);
        let value: serde_json::Value = serde_json::from_str(&out.text).unwrap();
        let java = value
            .pointer(pointer)
            .unwrap_or_else(|| panic!("{}", out.text));
        assert_eq!(java["ok"], true, "{tool}: {java}");
        for key in ["java_home", "source", "major_version"] {
            assert_eq!(java[key], expected[key], "{tool}.{key}: {java}");
        }
    }

    let build = client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));
    assert!(!build.is_error, "{}", build.text);
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap().trim(),
        jdk.to_string_lossy()
    );
}

#[test]
fn run_gradle_task_succeeds_when_the_wrapper_succeeds() {
    let sandbox = Sandbox::new();
    let mut client = sandbox.start();

    let build = client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));

    assert!(!build.is_error, "{}", build.text);
}

/// An app module whose applicationId is `com.example.app`, with a `.debug` suffix.
fn write_app_module(sandbox: &Sandbox) {
    let app = sandbox.project.join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("build.gradle.kts"),
        r#"android {
    defaultConfig {
        applicationId = "com.example.app"
    }
    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
    }
}
"#,
    )
    .unwrap();
}

#[test]
fn stop_app_refuses_a_foreign_package_without_touching_the_device() {
    let sandbox = Sandbox::new();
    write_app_module(&sandbox);
    let mut client = sandbox.start();

    for package in ["com.google.android.gms", "com.example.apple"] {
        let message = client.call_tool_rejected(
            "stop_app",
            json!({ "device_serial": "emulator-5554", "package": package }),
        );
        assert!(message.contains("allow_foreign_package: true"), "{message}");
    }

    assert!(
        sandbox.adb_calls().is_empty(),
        "adb must not run: {:?}",
        sandbox.adb_calls()
    );
}

#[test]
fn stop_app_acts_on_the_project_variant_and_on_opted_in_packages() {
    let sandbox = Sandbox::new();
    write_app_module(&sandbox);
    let mut client = sandbox.start();

    let own = client.call_tool(
        "stop_app",
        json!({ "device_serial": "emulator-5554", "package": "com.example.app.debug" }),
    );
    let foreign = client.call_tool(
        "stop_app",
        json!({
            "device_serial": "emulator-5554",
            "package": "com.other.app",
            "allow_foreign_package": true,
        }),
    );

    assert!(!own.is_error, "{}", own.text);
    assert!(!foreign.is_error, "{}", foreign.text);
    assert_eq!(
        sandbox.adb_calls(),
        vec![
            "-s emulator-5554 shell am force-stop com.example.app.debug",
            "-s emulator-5554 shell am force-stop com.other.app",
        ]
    );
}

#[test]
fn destructive_package_tools_refuse_when_the_project_id_is_unknown() {
    let sandbox = Sandbox::new();
    let mut client = sandbox.start();

    let restart = client.call_tool_rejected(
        "restart_app",
        json!({
            "device_serial": "emulator-5554",
            "package": "com.example.app",
            "clear_data": true,
        }),
    );
    let revoke = client.call_tool_rejected(
        "revoke_runtime_permission",
        json!({
            "deviceSerial": "emulator-5554",
            "package": "com.example.app",
            "permission": "android.permission.CAMERA",
        }),
    );

    for message in [restart, revoke] {
        assert!(message.contains("could not be determined"), "{message}");
        assert!(message.contains("allow_foreign_package: true"), "{message}");
    }
    assert!(sandbox.adb_calls().is_empty(), "{:?}", sandbox.adb_calls());
}

#[test]
fn set_network_state_refuses_to_cut_off_a_wireless_adb_device() {
    let sandbox = Sandbox::new();
    let mut client = sandbox.start();

    let out = client.call_tool(
        "set_network_state",
        json!({ "deviceSerial": "192.168.1.5:5555", "wifi": false }),
    );

    assert!(out.is_error, "{}", out.text);
    assert!(out.text.contains("wireless ADB"), "{}", out.text);
    assert!(sandbox.adb_calls().is_empty(), "{:?}", sandbox.adb_calls());
}

#[test]
fn open_deep_link_reports_an_intent_nothing_handles() {
    let sandbox = Sandbox::new();
    sandbox.write_adb(
        r#"echo 'Starting: Intent { act=android.intent.action.VIEW dat=myapp://missing }'
echo 'Error: Activity not started, unable to resolve Intent { act=android.intent.action.VIEW dat=myapp://missing flg=0x10000000 }'"#,
    );
    let mut client = sandbox.start();

    let out = client.call_tool(
        "open_deep_link",
        json!({ "deviceSerial": "emulator-5554", "uri": "myapp://missing" }),
    );

    assert!(out.is_error, "{}", out.text);
    assert!(
        out.text.contains("unable to resolve Intent"),
        "{}",
        out.text
    );

#[test]
fn two_servers_building_at_once_keep_both_builds_in_the_shared_history() {
    let sandbox = Sandbox::new();
    sandbox.write_gradlew("sleep 1\necho 'BUILD SUCCESSFUL in 1s'");
    // Two processes on one data directory, like the app and a headless server.
    let clients = [sandbox.start(), sandbox.start()];

    let builds: Vec<_> = clients
        .into_iter()
        .map(|mut client| {
            std::thread::spawn(move || {
                client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }))
            })
        })
        .collect();
    for build in builds {
        let build = build.join().unwrap();
        assert!(!build.is_error, "{}", build.text);
    }

    let data = sandbox.home.join(".keynobi");
    let history: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(data.join("build-history.json")).unwrap())
            .unwrap();
    let mut ids: Vec<u64> = history.iter().filter_map(|r| r["id"].as_u64()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        2,
        "both builds recorded with distinct IDs: {history:?}"
    );
    for id in ids {
        assert!(
            data.join("build-logs")
                .join(format!("build-{id}.jsonl"))
                .is_file(),
            "log of build {id} kept"
        );
    }
}
