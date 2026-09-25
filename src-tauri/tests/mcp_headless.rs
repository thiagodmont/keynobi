//! End-to-end tests of the `keynobi --mcp` binary, standalone and attached to
//! an app. See `headless/mod.rs`.

mod common;
mod headless;

use headless::{Sandbox, TestApp};
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
}

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

/// A fake `gradlew` that leaves a marker file when it runs.
fn gradlew_leaving_a_marker(sandbox: &Sandbox) -> std::path::PathBuf {
    let marker = sandbox.home.join("gradlew-ran");
    sandbox.write_gradlew(&format!(
        "touch '{}'\necho 'BUILD SUCCESSFUL in 1s'",
        marker.display()
    ));
    marker
}

#[test]
fn an_untrusted_project_cannot_build_and_its_gradlew_never_runs() {
    let sandbox = Sandbox::new();
    let marker = gradlew_leaving_a_marker(&sandbox);
    let registries = [
        json!([]),
        json!([headless::project_entry(&sandbox.project, json!(null))]),
        json!([headless::project_entry(&sandbox.project, json!(false))]),
    ];

    for registry in registries {
        sandbox.write_projects(registry.clone(), None);
        let mut client = sandbox.start();

        for (tool, args) in [
            ("run_gradle_task", json!({ "task": "assembleDebug" })),
            ("run_tests", json!({ "test_type": "unit" })),
        ] {
            let message = client.call_tool_rejected(tool, args);
            assert!(message.contains("not trusted"), "{tool}: {message}");
            assert!(message.contains("Keynobi app"), "{tool}: {message}");
        }
        // Read-only tools keep working.
        let info = client.call_tool("get_project_info", json!({}));
        assert!(!info.is_error, "{}", info.text);

        assert!(!marker.exists(), "gradlew ran for registry {registry}");
    }
}

#[test]
fn a_trusted_project_builds() {
    let sandbox = Sandbox::new();
    let marker = gradlew_leaving_a_marker(&sandbox);
    let mut client = sandbox.start();

    let build = client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));

    assert!(!build.is_error, "{}", build.text);
    assert!(marker.exists(), "gradlew did not run");
}

fn project_info(client: &mut headless::McpClient) -> serde_json::Value {
    let out = client.call_tool("get_project_info", json!({}));
    assert!(!out.is_error, "{}", out.text);
    serde_json::from_str(&out.text).unwrap()
}

#[test]
fn get_project_info_reports_how_the_project_was_selected_and_its_trust() {
    let sandbox = Sandbox::new();
    let info = project_info(&mut sandbox.start());
    assert_eq!(info["selected_by"], "argument", "{info}");
    assert_eq!(info["trusted"], true, "{info}");
    assert_eq!(info["trust_hint"], serde_json::Value::Null, "{info}");

    sandbox.write_projects(
        json!([headless::project_entry(&sandbox.project, json!(false))]),
        None,
    );
    let info = project_info(&mut sandbox.start());
    assert_eq!(info["trusted"], false, "{info}");
    assert!(
        info["trust_hint"].as_str().unwrap().contains("Trust"),
        "{info}"
    );
}

#[test]
fn project_info_never_runs_java_chosen_by_an_untrusted_project() {
    let sandbox = Sandbox::new();
    sandbox.write_projects(json!([]), None);
    let marker = sandbox.home.join("project-java-ran");
    let project_jdk = sandbox.project.join("tools");
    headless::write_script(
        &project_jdk.join("bin").join("java"),
        &format!("touch '{}'", marker.display()),
    );
    std::fs::write(
        sandbox.project.join("gradle.properties"),
        format!("org.gradle.java.home={}\n", project_jdk.display()),
    )
    .unwrap();
    let mut client = sandbox.start();

    for tool in ["get_project_info", "run_health_check"] {
        let out = client.call_tool(tool, json!({}));
        assert!(!out.is_error, "{tool}: {}", out.text);
    }

    assert!(!marker.exists(), "the project's java ran");
}

#[test]
fn the_working_directory_beats_a_stale_last_active_project() {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.project.join("app")).unwrap();
    let stale = sandbox.home.join("stale-project");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("settings.gradle"), "").unwrap();
    sandbox.write_projects(
        json!([
            headless::project_entry(&sandbox.project, json!(true)),
            headless::project_entry(&stale, json!(true)),
        ]),
        Some(&stale),
    );

    let info = project_info(&mut sandbox.start_in(&sandbox.project.join("app"), None));
    assert_eq!(info["path"], sandbox.project.to_string_lossy().as_ref());
    assert_eq!(info["selected_by"], "working_directory", "{info}");

    // Outside any Gradle build, the app's last active project is used.
    let info = project_info(&mut sandbox.start_in(&sandbox.home, None));
    assert_eq!(info["path"], stale.to_string_lossy().as_ref());
    assert_eq!(info["selected_by"], "last_active_project", "{info}");
}

/// A white `width`x`height` screen, as `adb exec-out screencap -p` prints it.
fn write_screencap(sandbox: &Sandbox, width: u32, height: u32) -> Vec<u8> {
    let mut png_bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![255; width as usize * height as usize * 4])
        .unwrap();
    writer.finish().unwrap();

    let path = sandbox.home.join("screen.png");
    std::fs::write(&path, &png_bytes).unwrap();
    sandbox.write_adb(&format!(
        "case \"$*\" in *\"exec-out screencap -p\") cat '{}' ;; esac",
        path.display()
    ));
    png_bytes
}

/// The screenshot's PNG bytes and its geometry JSON.
fn call_screenshot(
    client: &mut headless::McpClient,
    arguments: serde_json::Value,
) -> (Vec<u8>, serde_json::Value) {
    use base64::Engine as _;
    let result = client.request(
        "tools/call",
        json!({ "name": "screenshot", "arguments": arguments }),
    );
    assert_ne!(result["isError"], json!(true), "{result}");
    let content = result["content"].as_array().expect("content");
    assert_eq!(content[0]["type"], "image", "{result}");
    assert_eq!(content[0]["mimeType"], "image/png", "{result}");
    let image = base64::engine::general_purpose::STANDARD
        .decode(content[0]["data"].as_str().expect("image data"))
        .expect("base64 image");
    let geometry = serde_json::from_str(content[1]["text"].as_str().expect("geometry text"))
        .expect("geometry JSON");
    (image, geometry)
}

fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.read_info().expect("valid PNG").info().size()
}

#[test]
fn screenshot_is_downscaled_and_reports_the_scale_back_to_device_pixels() {
    let sandbox = Sandbox::new();
    let original = write_screencap(&sandbox, 1080, 2400);
    let mut client = sandbox.start();

    let (image, geometry) =
        call_screenshot(&mut client, json!({ "device_serial": "emulator-5554" }));
    assert_eq!(png_dimensions(&image), (576, 1280));
    assert_eq!(geometry["deviceWidth"], 1080, "{geometry}");
    assert_eq!(geometry["deviceHeight"], 2400, "{geometry}");
    assert_eq!(geometry["imageWidth"], 576, "{geometry}");
    assert_eq!(geometry["imageHeight"], 1280, "{geometry}");
    assert_eq!(geometry["scale"], 1.875, "{geometry}");
    assert!(
        geometry["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("ui_tap"),
        "{geometry}"
    );

    let (image, geometry) = call_screenshot(
        &mut client,
        json!({ "device_serial": "emulator-5554", "full_size": true }),
    );
    assert_eq!(image, original);
    assert_eq!(geometry["scale"], 1.0, "{geometry}");
    assert_eq!(geometry["imageWidth"], 1080, "{geometry}");
}

#[test]
fn screenshot_rejects_an_out_of_range_size_without_touching_the_device() {
    let sandbox = Sandbox::new();
    write_screencap(&sandbox, 1080, 2400);
    let mut client = sandbox.start();

    let message = client.call_tool_rejected(
        "screenshot",
        json!({ "device_serial": "emulator-5554", "max_dimension": 10 }),
    );

    assert!(message.contains("max_dimension"), "{message}");
    assert!(sandbox.adb_calls().is_empty(), "{:?}", sandbox.adb_calls());
}

#[test]
fn screenshot_reports_adb_output_that_is_not_an_image() {
    let sandbox = Sandbox::new();
    sandbox.write_adb("echo 'error: device unauthorized'");
    let mut client = sandbox.start();

    let out = client.call_tool("screenshot", json!({ "device_serial": "emulator-5554" }));

    assert!(out.is_error, "{}", out.text);
    assert!(out.text.contains("did not return a PNG"), "{}", out.text);
    assert!(out.text.contains("device unauthorized"), "{}", out.text);
}

// ── Attaching to the app ──────────────────────────────────────────────────────

const NOT_RUNNING: &str = "the Keynobi app is not running";

#[test]
fn without_the_app_the_server_runs_standalone_and_says_why() {
    let sandbox = Sandbox::new();
    let mut client = sandbox.start();

    assert!(
        client
            .instructions()
            .starts_with(&format!("Mode: standalone, because {NOT_RUNNING}.")),
        "{}",
        client.instructions()
    );
    assert!(client
        .instructions()
        .contains("not visible in the Keynobi app"));
    assert_eq!(client.init["serverInfo"]["name"], "keynobi");

    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "standalone", "{info}");
    assert_eq!(info["standalone_reason"], NOT_RUNNING, "{info}");

    let status = client.call_tool_json("get_build_status", json!({}));
    assert_eq!(status["mode"], "standalone", "{status}");

    let build = client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));
    assert!(!build.is_error, "{}", build.text);
    assert!(
        build
            .text
            .contains(&format!("mode: standalone ({NOT_RUNNING})")),
        "{}",
        build.text
    );

    assert!(
        sandbox
            .activity_log()
            .contains(&format!("Server started (standalone: {NOT_RUNNING})")),
        "{}",
        sandbox.activity_log()
    );
}

#[test]
fn a_standalone_server_records_itself_while_it_runs() {
    let sandbox = Sandbox::new();
    let client = sandbox.start();
    let records = sandbox.home.join(".keynobi").join("mcp-sessions");

    let names: Vec<_> = std::fs::read_dir(&records)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(records.join(&names[0])).unwrap()).unwrap();
    assert_eq!(record["reason"], NOT_RUNNING);
    drop(client);
}

#[test]
fn a_server_attaches_to_the_app_that_has_its_project_open() {
    let sandbox = Sandbox::new();
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    let mut client = sandbox.start();
    app.wait_for_sessions(1);

    assert!(
        client.instructions().starts_with("Mode: attached"),
        "{}",
        client.instructions()
    );
    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "attached", "{info}");
    assert_eq!(info["follows_app"], false, "{info}");
    assert_eq!(info["pinned_project"], json!(sandbox.project), "{info}");
    assert_eq!(info["path"], json!(sandbox.project), "{info}");
    assert_eq!(info["selected_by"], "argument", "{info}");

    // No standalone record: this process serves nothing itself.
    assert!(!sandbox.home.join(".keynobi").join("mcp-sessions").exists());
    assert_eq!(
        app.registry.sessions()[0].client_name.as_deref(),
        Some("keynobi-headless-test")
    );
}

#[test]
fn a_server_without_a_project_follows_the_app() {
    let sandbox = Sandbox::new();
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    // The home folder is not inside a Gradle build.
    let mut client = sandbox.start_in(&sandbox.home, None);

    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "attached", "{info}");
    assert_eq!(info["follows_app"], true, "{info}");
    assert_eq!(info["selected_by"], "app", "{info}");
    assert_eq!(info["path"], json!(sandbox.project), "{info}");

    // Switching projects in the app moves the session with it.
    let other = sandbox.home.join("other");
    std::fs::create_dir_all(&other).unwrap();
    app.open(Some(&other));
    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["path"], json!(other), "{info}");
}

#[test]
fn a_project_mismatch_runs_standalone_and_names_the_app_project() {
    let sandbox = Sandbox::new();
    let other = sandbox.home.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let app = TestApp::listen(&sandbox, Some(&other));
    let mut client = sandbox.start();

    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "standalone", "{info}");
    let reason = info["standalone_reason"].as_str().unwrap();
    assert!(
        reason.contains("another project open") && reason.contains(&other.display().to_string()),
        "{reason}"
    );
    assert_eq!(info["path"], json!(sandbox.project), "{info}");
    assert!(client.instructions().contains(reason));
    assert!(app.registry.sessions().is_empty());
}

#[test]
fn a_pinned_session_refuses_project_tools_after_the_app_switches_projects() {
    let sandbox = Sandbox::new();
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    let mut client = sandbox.start();

    let other = sandbox.home.join("other");
    std::fs::create_dir_all(&other).unwrap();
    app.open(Some(&other));

    let build = client.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));
    assert!(build.is_error, "{}", build.text);
    assert!(
        build
            .text
            .contains(&format!("Keynobi now has {} open", other.display()))
            && build.text.contains(&format!(
                "this session is for {}",
                sandbox.project.display()
            )),
        "{}",
        build.text
    );
    // Device tools do not depend on the project.
    let devices = client.call_tool("list_devices", json!({}));
    assert!(!devices.is_error, "{}", devices.text);
}

#[test]
fn attach_only_fails_instead_of_running_standalone() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command(&sandbox.project, Some(&sandbox.project))
        .arg("--attach-only")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(NOT_RUNNING), "{stderr}");
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        !sandbox.activity_log().contains("Server started"),
        "{}",
        sandbox.activity_log()
    );
}

#[test]
fn one_attached_client_leaving_does_not_affect_another() {
    let sandbox = Sandbox::new();
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    let first = sandbox.start();
    let mut second = sandbox.start();
    app.wait_for_sessions(2);

    drop(first);
    app.wait_for_sessions(1);

    let devices = second.call_tool("list_devices", json!({}));
    assert!(!devices.is_error, "{}", devices.text);
    let info = second.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "attached", "{info}");
}

#[test]
fn a_stale_socket_file_means_standalone_until_the_app_replaces_it() {
    let sandbox = Sandbox::new();
    // Left behind by an app that crashed.
    drop(std::os::unix::net::UnixListener::bind(sandbox.socket_path()).unwrap());

    let info = sandbox
        .start()
        .call_tool_json("get_project_info", json!({}));
    assert_eq!(info["standalone_reason"], NOT_RUNNING, "{info}");

    let _app = TestApp::listen(&sandbox, Some(&sandbox.project));
    let info = sandbox
        .start()
        .call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "attached", "{info}");
}

/// The app and every attached session share one build slot, and all of them
/// get the same answer while it is taken.
#[test]
fn the_app_and_attached_sessions_share_one_build_slot() {
    use keynobi_lib::services::build_runner::{self, BUILD_ALREADY_RUNNING};

    let sandbox = Sandbox::new();
    sandbox.write_gradlew("sleep 3\necho 'BUILD SUCCESSFUL in 3s'");
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    app.trust(&sandbox.project);
    let mut first = sandbox.start();
    let mut second = sandbox.start();
    app.wait_for_sessions(2);

    // The app's own build holds the slot: an agent is refused.
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(build_runner::try_reserve_build_slot(
        &app.build_state,
        "assembleDebug",
        "2026-01-01T00:00:00Z",
    ))
    .unwrap();
    let busy = first.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));
    assert!(busy.is_error, "{}", busy.text);
    assert!(busy.text.contains(BUILD_ALREADY_RUNNING), "{}", busy.text);
    rt.block_on(async {
        build_runner::mark_build_spawn_failed(&mut *app.build_state.inner.lock().await);
    });

    // An agent's build holds the slot: the other agent and the app are refused.
    let building = std::thread::spawn(move || {
        let out = first.call_tool("run_gradle_task", json!({ "task": "assembleDebug" }));
        (first, out)
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = second.call_tool_json("get_build_status", json!({}));
        if status["status"] == "running" {
            assert_eq!(status["mode"], "attached", "{status}");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{status}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let busy = second.call_tool("run_gradle_task", json!({ "task": "assembleRelease" }));
    assert!(busy.is_error, "{}", busy.text);
    assert!(busy.text.contains(BUILD_ALREADY_RUNNING), "{}", busy.text);
    let app_busy = rt.block_on(build_runner::try_reserve_build_slot(
        &app.build_state,
        "assembleDebug",
        "2026-01-01T00:00:01Z",
    ));
    assert_eq!(app_busy, Err(BUILD_ALREADY_RUNNING.to_string()));

    let (_first, done) = building.join().unwrap();
    assert!(!done.is_error, "{}", done.text);
    assert!(done.text.contains("mode: attached"), "{}", done.text);
}

#[test]
fn settings_saved_with_the_removed_mcp_auto_start_still_load() {
    let sandbox = Sandbox::new();
    let settings = sandbox.home.join(".keynobi").join("settings.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    value["mcp"] = json!({ "autoStart": true, "allowUnrestrictedGradle": true });
    std::fs::write(&settings, value.to_string()).unwrap();
    let mut client = sandbox.start();

    // Only allowed because the rest of the MCP settings loaded.
    let build = client.call_tool("run_gradle_task", json!({ "task": "publishRelease" }));
    assert!(!build.is_error, "{}", build.text);
    assert!(!sandbox
        .home
        .join(".keynobi")
        .join("settings.json.corrupt")
        .exists());
}
