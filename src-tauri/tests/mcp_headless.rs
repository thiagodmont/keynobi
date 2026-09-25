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
