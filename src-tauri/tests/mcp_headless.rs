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
fn stop_avd_reports_a_stop_the_emulator_refused() {
    let sandbox = Sandbox::new();
    sandbox.write_adb(
        r#"case "$*" in
  "devices -l") printf 'List of devices attached\nemulator-5554\tdevice\n' ;;
  *"emu kill") echo "error: could not connect to TCP port 5554: Connection refused" >&2; exit 1 ;;
esac"#,
    );
    let mut client = sandbox.start();

    let out = client.call_tool("stop_avd", json!({ "serial": "emulator-5554" }));

    assert!(out.is_error, "{}", out.text);
    assert!(out.text.contains("Connection refused"), "{}", out.text);
    assert!(!out.text.contains("stopped."), "{}", out.text);
}

/// Two standalone servers on one data directory (like the app and a headless
/// server) do not build one project at once, and both builds are kept in the
/// shared history.
#[test]
fn two_standalone_servers_take_turns_building_one_project() {
    let sandbox = Sandbox::new();
    let started = sandbox.home.join("first-build-started");
    let release = sandbox.home.join("release-build");
    sandbox.write_gradlew(&format!(
        "touch '{}'\n\
         while [ ! -e '{}' ]; do sleep 0.05; done\n\
         echo 'BUILD SUCCESSFUL in 1s'",
        started.display(),
        release.display()
    ));
    let [first, mut second] = [sandbox.start(), sandbox.start()];

    let building = build_in_background(first, "assembleDebug");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !started.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "the first build never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let busy = second.call_tool("run_gradle_task", json!({ "task": "assembleRelease" }));
    assert!(busy.is_error, "{}", busy.text);
    assert!(
        busy.text.starts_with(
            "A Gradle build is already running for this project in another Keynobi process"
        ),
        "{}",
        busy.text
    );

    std::fs::write(&release, "").unwrap();
    let (_first, done) = building.join().unwrap();
    assert!(!done.is_error, "{}", done.text);
    let after = second.call_tool("run_gradle_task", json!({ "task": "assembleRelease" }));
    assert!(!after.is_error, "{}", after.text);

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
    for record in &history {
        assert_eq!(record["origin"]["kind"], "agent", "{record}");
        assert_eq!(record["origin"]["standalone"], true, "{record}");
        assert_eq!(record["origin"]["clientName"], "keynobi-headless-test");
    }
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
    assert_eq!(record["version"], env!("CARGO_PKG_VERSION"));
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
    // The app learns the attached binary's version from the handshake.
    assert_eq!(
        app.registry.sessions()[0].version,
        env!("CARGO_PKG_VERSION")
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

/// A fake gradlew that prints a task line, waits for `release` to exist,
/// then succeeds.
fn gradlew_waiting_for(sandbox: &Sandbox) -> std::path::PathBuf {
    let release = sandbox.home.join("release-build");
    sandbox.write_gradlew(&format!(
        "echo '> Task :app:compileDebugKotlin'\n\
         while [ ! -e '{}' ]; do sleep 0.05; done\n\
         echo 'BUILD SUCCESSFUL in 1s'",
        release.display()
    ));
    release
}

/// A client that disconnects while its build runs does not take the build
/// with it: the build finishes, is recorded, and frees the slot.
#[test]
fn a_build_outlives_the_attached_client_that_started_it() {
    let sandbox = Sandbox::new();
    let release = gradlew_waiting_for(&sandbox);
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    app.trust(&sandbox.project);
    let mut client = sandbox.start();
    app.wait_for_sessions(1);
    let task = format!("assembleOutlives{}", std::process::id());

    let pid = client.pid();
    let waiting = {
        let task = task.clone();
        std::thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                client.call_tool("run_gradle_task", json!({ "task": task }))
            }));
        })
    };
    app.wait_for_build("the agent's build to start", |bs| {
        bs.current_build.is_some()
    });
    // The client goes away mid-build.
    std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    let _ = waiting.join();
    app.wait_for_sessions(0);
    // The session gives its requests 5 s to finish after the client leaves,
    // then cancels them; the build must not take that as a cancellation.
    std::thread::sleep(std::time::Duration::from_secs(7));
    assert!(app
        .build_state
        .inner
        .blocking_lock()
        .current_build
        .is_some());

    std::fs::write(&release, "").unwrap();
    app.wait_for_build("the build to be recorded", |bs| {
        bs.history.iter().any(|r| r.task == task) && bs.current_build.is_none()
    });
    let status = app.build_state.inner.blocking_lock().status.clone();
    assert!(
        matches!(status, keynobi_lib::models::build::BuildStatus::Success(_)),
        "{status:?}"
    );
    let origin = app
        .build_state
        .inner
        .blocking_lock()
        .history
        .iter()
        .find(|r| r.task == task)
        .and_then(|r| r.origin.clone());
    assert!(
        matches!(
            origin,
            Some(keynobi_lib::models::build::BuildActor::Agent(_))
        ),
        "{origin:?}"
    );
}

/// Start `task` from `client` on another thread; returns the thread, which
/// hands back the client and the tool's result.
fn build_in_background(
    mut client: headless::McpClient,
    task: &str,
) -> std::thread::JoinHandle<(headless::McpClient, headless::ToolOutput)> {
    let task = task.to_string();
    std::thread::spawn(move || {
        let out = client.call_tool("run_gradle_task", json!({ "task": task }));
        (client, out)
    })
}

/// A build an agent starts is the app's build: same record, with the agent
/// as its origin, and the app can cancel it; the agent hears who did.
#[test]
fn the_app_sees_and_can_cancel_an_attached_agents_build() {
    use keynobi_lib::models::build::{BuildActor, BuildStatus};
    use keynobi_lib::services::build_runner;

    let sandbox = Sandbox::new();
    let _release = gradlew_waiting_for(&sandbox);
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    app.trust(&sandbox.project);
    let client = sandbox.start();
    let mut watcher = sandbox.start();
    app.wait_for_sessions(2);
    let session_id = app.registry.sessions()[0].id;
    let task = format!("assembleCancelled{}", std::process::id());

    let building = build_in_background(client, &task);
    app.wait_for_build("the agent's build to start", |bs| {
        bs.current_build.is_some()
    });
    let origin = app.build_state.inner.blocking_lock().status_origin.clone();
    let Some(BuildActor::Agent(agent)) = origin else {
        panic!("{origin:?}")
    };
    assert_eq!(agent.client_name.as_deref(), Some("keynobi-headless-test"));
    assert!(!agent.standalone);
    assert_eq!(agent.session_id, Some(session_id));
    let status = watcher.call_tool_json("get_build_status", json!({}));
    assert_eq!(status["status"], "running", "{status}");
    assert_eq!(status["origin"]["kind"], "agent", "{status}");

    // The user cancels it in the app.
    assert!(app.block_on(build_runner::cancel_build(
        &app.build_state,
        &app.process_manager,
        BuildActor::App,
    )));
    let (_client, out) = building.join().unwrap();
    assert!(out.is_error, "{}", out.text);
    assert!(
        out.text.contains("was cancelled in the Keynobi app"),
        "{}",
        out.text
    );
    app.wait_for_build("the build to be recorded", |bs| {
        bs.history.iter().any(|r| r.task == task)
    });
    let record = app
        .build_state
        .inner
        .blocking_lock()
        .history
        .iter()
        .find(|r| r.task == task)
        .cloned()
        .unwrap();
    assert!(matches!(record.status, BuildStatus::Cancelled));
    assert_eq!(record.cancelled_by, Some(BuildActor::App));
    let Some(BuildActor::Agent(agent)) = record.origin else {
        panic!("{:?}", record.origin)
    };
    assert_eq!(agent.session_id, Some(session_id));

    let status = watcher.call_tool_json("get_build_status", json!({}));
    assert_eq!(status["status"], "cancelled", "{status}");
    assert_eq!(status["cancelled_by"]["kind"], "app", "{status}");
}

/// Quitting the app cancels an agent's build (recorded as cancelled because
/// Keynobi quit), answers the agent, and the agent's next call is served
/// standalone.
#[test]
fn quitting_the_app_answers_the_agent_and_it_continues_standalone() {
    use keynobi_lib::models::build::BuildActor;

    let sandbox = Sandbox::new();
    let release = gradlew_waiting_for(&sandbox);
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    app.trust(&sandbox.project);
    let mut client = sandbox.start();
    app.wait_for_sessions(1);
    let task = format!("assembleQuit{}", std::process::id());

    let id = client.send_request(
        "tools/call",
        json!({ "name": "run_gradle_task", "arguments": { "task": task } }),
    );
    app.wait_for_build("the agent's build to start", |bs| {
        bs.current_build.is_some()
    });
    app.quit();
    // The build's own answer, or the app's quit error: either way an error
    // that says Keynobi quit.
    let answer = client.wait_response(id);
    let text = match &answer {
        Ok(result) => {
            assert_eq!(result["isError"], true, "{result}");
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        }
        Err(error) => error["message"].as_str().unwrap_or_default().to_string(),
    };
    assert!(
        text.contains("because Keynobi quit") || text.contains("Keynobi is quitting"),
        "{text}"
    );
    let record = app
        .build_state
        .inner
        .blocking_lock()
        .history
        .iter()
        .find(|r| r.task == task)
        .cloned()
        .expect("the cancelled build is recorded before quitting");
    assert_eq!(record.cancelled_by, Some(BuildActor::AppQuit));
    app.wait_for_sessions(0);

    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "standalone", "{info}");
    assert_eq!(info["standalone_reason"], "the Keynobi app quit", "{info}");
    let devices = client.call_tool("list_devices", json!({}));
    assert!(!devices.is_error, "{}", devices.text);
    std::fs::write(&release, "").unwrap();
}

/// When the app goes away without answering (it crashed), the relay answers
/// the request in flight itself and the session continues standalone.
#[test]
fn an_app_that_vanishes_mid_request_leaves_the_agent_an_error_and_a_standalone_server() {
    let sandbox = Sandbox::new();
    let release = gradlew_waiting_for(&sandbox);
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    app.trust(&sandbox.project);
    let mut client = sandbox.start();
    app.wait_for_sessions(1);

    let id = client.send_request(
        "tools/call",
        json!({ "name": "run_gradle_task", "arguments": { "task": "assembleDebug" } }),
    );
    app.wait_for_build("the agent's build to start", |bs| {
        bs.current_build.is_some()
    });
    drop(app);

    let error = client
        .wait_response(id)
        .expect_err("the relay answers with an error");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("closed the MCP session before answering"),
        "{message}"
    );
    let info = client.call_tool_json("get_project_info", json!({}));
    assert_eq!(info["mode"], "standalone", "{info}");
    assert_eq!(info["standalone_reason"], "the Keynobi app quit", "{info}");
    assert!(client.tool_names().contains(&"run_gradle_task".to_string()));
    // The app's orphaned build can end now.
    std::fs::write(&release, "").unwrap();
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

/// Call `run_gradle_task` with `meta` and collect the progress notifications
/// sent before its result. `on_progress` runs for each one.
fn build_noting_progress(
    client: &mut headless::McpClient,
    meta: Option<serde_json::Value>,
    mut on_progress: impl FnMut(&serde_json::Value),
) -> (serde_json::Value, Vec<serde_json::Value>) {
    let mut params = json!({
        "name": "run_gradle_task",
        "arguments": { "task": "assembleDebug" },
    });
    if let Some(meta) = meta {
        params["_meta"] = meta;
    }
    let id = client.send_request("tools/call", params);
    let mut progress = Vec::new();
    let result = client
        .wait_response_noting(id, |message| {
            if message["method"] == "notifications/progress" {
                on_progress(&message["params"]);
                progress.push(message["params"].clone());
            }
        })
        .expect("run_gradle_task");
    (result, progress)
}

/// A client that sends a progress token hears how its build is going: time
/// elapsed and the Gradle task running.
#[test]
fn a_build_reports_progress_to_a_client_that_asks() {
    let sandbox = Sandbox::new();
    let release = gradlew_waiting_for(&sandbox);
    let mut client = sandbox.start();

    const TASK: &str = "> Task :app:compileDebugKotlin";
    let names_task = |report: &serde_json::Value| {
        report["message"]
            .as_str()
            .is_some_and(|message| message.contains(TASK))
    };
    let (mut reports, mut with_task) = (0, 0);
    let (result, progress) = build_noting_progress(
        &mut client,
        Some(json!({ "progressToken": "build-1" })),
        |report| {
            reports += 1;
            with_task += usize::from(names_task(report));
            // Let the build finish once it has reported the task twice, or
            // after 30 s of reports without it, so the assertions below fail.
            if with_task == 2 || reports == 15 {
                std::fs::write(&release, "").unwrap();
            }
        },
    );

    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.starts_with("BUILD SUCCESSFUL"), "{result}");
    for report in &progress {
        assert_eq!(report["progressToken"], "build-1", "{report}");
        let message = report["message"].as_str().unwrap_or_default();
        assert!(message.contains("s elapsed"), "{report}");
    }
    // A report sent before Gradle printed a task (a slow start) has none;
    // every report after the first that names it does.
    let first = progress
        .iter()
        .position(names_task)
        .unwrap_or_else(|| panic!("no report named the task: {progress:?}"));
    assert!(progress[first..].len() >= 2, "{progress:?}");
    for report in &progress[first..] {
        assert!(names_task(report), "{report}");
    }
    let values: Vec<f64> = progress
        .iter()
        .map(|report| report["progress"].as_f64().unwrap())
        .collect();
    assert!(values.windows(2).all(|w| w[0] < w[1]), "{values:?}");
}

/// Without a progress token, a build sends no progress notifications.
#[test]
fn a_build_sends_no_progress_without_a_token() {
    let sandbox = Sandbox::new();
    let release = gradlew_waiting_for(&sandbox);
    let mut client = sandbox.start();
    // Long enough for two reports had they been asked for.
    let releasing = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        std::fs::write(&release, "").unwrap();
    });

    let (result, progress) = build_noting_progress(&mut client, None, |_| {});
    releasing.join().unwrap();

    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.starts_with("BUILD SUCCESSFUL"), "{result}");
    assert!(progress.is_empty(), "{progress:?}");
}

/// A client that cancels its `run_gradle_task` request cancels that build,
/// recorded as cancelled by that agent; the session keeps working.
#[test]
fn cancelling_a_build_request_cancels_its_build() {
    use keynobi_lib::models::build::{BuildActor, BuildStatus};

    let sandbox = Sandbox::new();
    let _release = gradlew_waiting_for(&sandbox);
    let app = TestApp::listen(&sandbox, Some(&sandbox.project));
    app.trust(&sandbox.project);
    let mut client = sandbox.start();
    app.wait_for_sessions(1);
    let session_id = app.registry.sessions()[0].id;
    let task = format!("assembleRequestCancelled{}", std::process::id());

    let id = client.send_request(
        "tools/call",
        json!({ "name": "run_gradle_task", "arguments": { "task": task } }),
    );
    app.wait_for_build("the agent's build to start", |bs| {
        bs.current_build.is_some()
    });
    client.notify(
        "notifications/cancelled",
        json!({ "requestId": id, "reason": "the user stopped the agent" }),
    );

    app.wait_for_build("the cancelled build to be recorded", |bs| {
        bs.history.iter().any(|r| r.task == task)
    });
    let record = app
        .build_state
        .inner
        .blocking_lock()
        .history
        .iter()
        .find(|r| r.task == task)
        .cloned()
        .unwrap();
    assert!(matches!(record.status, BuildStatus::Cancelled));
    let Some(BuildActor::Agent(agent)) = record.cancelled_by else {
        panic!("{:?}", record.cancelled_by)
    };
    assert_eq!(agent.client_name.as_deref(), Some("keynobi-headless-test"));
    assert_eq!(agent.session_id, Some(session_id));

    let status = client.call_tool_json("get_build_status", json!({}));
    assert_eq!(status["status"], "cancelled", "{status}");
    assert_eq!(status["cancelled_by"]["kind"], "agent", "{status}");
}

// ── Project boundaries ────────────────────────────────────────────────────────

fn write_file(path: &std::path::Path, contents: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn install_apk_refuses_an_apk_whose_build_directory_leaves_the_project() {
    let sandbox = Sandbox::new();
    let outside = sandbox.project.parent().unwrap().join("outside-build");
    write_file(&outside.join("outputs/apk/debug/app-debug.apk"), b"apk");
    std::fs::create_dir_all(sandbox.project.join("app")).unwrap();
    std::os::unix::fs::symlink(&outside, sandbox.project.join("app/build")).unwrap();
    let mut client = sandbox.start();

    let through_link = sandbox
        .project
        .join("app/build/outputs/apk/debug/app-debug.apk");
    let message = client.call_tool_rejected(
        "install_apk",
        json!({ "device_serial": "emulator-5554", "apk_path": through_link }),
    );
    assert!(message.contains("outside the project"), "{message}");

    let traversal = sandbox
        .project
        .join("app/../../outside-build/outputs/apk/debug/app-debug.apk");
    client.call_tool_rejected(
        "install_apk",
        json!({ "device_serial": "emulator-5554", "apk_path": traversal }),
    );

    assert!(
        sandbox.adb_calls().is_empty(),
        "adb must not run: {:?}",
        sandbox.adb_calls()
    );
}

#[test]
fn install_apk_installs_the_apk_the_path_resolves_to() {
    let sandbox = Sandbox::new();
    sandbox.write_adb("echo Success");
    let debug = sandbox.project.join("app/build/outputs/apk/debug");
    write_file(&debug.join("app-debug.apk"), b"apk");
    std::os::unix::fs::symlink("app-debug.apk", debug.join("latest.apk")).unwrap();
    let mut client = sandbox.start();

    let out = client.call_tool(
        "install_apk",
        json!({ "device_serial": "emulator-5554", "apk_path": debug.join("latest.apk") }),
    );

    assert!(!out.is_error, "{}", out.text);
    assert_eq!(
        sandbox.adb_calls(),
        vec![format!(
            "-s emulator-5554 install -r -t {}",
            debug.join("app-debug.apk").display()
        )]
    );
}

fn resource_uris(client: &mut headless::McpClient) -> Vec<String> {
    client.request("resources/list", json!({}))["resources"]
        .as_array()
        .expect("resources/list returned no resources array")
        .iter()
        .filter_map(|r| r["uri"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn resources_never_serve_a_project_file_linked_outside_the_project() {
    let sandbox = Sandbox::new();
    let secret = sandbox.project.parent().unwrap().join("secret.txt");
    write_file(&secret, b"TOP SECRET");
    std::os::unix::fs::symlink(&secret, sandbox.project.join("build.gradle.kts")).unwrap();
    let mut client = sandbox.start();

    let uris = resource_uris(&mut client);
    assert!(
        uris.iter().any(|u| u == "android://gradle-settings"),
        "{uris:?}"
    );
    assert!(
        !uris.iter().any(|u| u == "android://build-gradle"),
        "{uris:?}"
    );

    let error = client
        .request_result("resources/read", json!({ "uri": "android://build-gradle" }))
        .expect_err("a file outside the project must not be read");
    assert!(
        error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("outside the project"),
        "{error}"
    );
    assert!(!error.to_string().contains("TOP SECRET"), "{error}");
}

#[test]
fn an_oversized_resource_is_truncated_and_says_so() {
    let sandbox = Sandbox::new();
    let body = "// padding\n".repeat(100_000);
    std::fs::write(sandbox.project.join("settings.gradle.kts"), &body).unwrap();
    let mut client = sandbox.start();

    let result = client.request(
        "resources/read",
        json!({ "uri": "android://gradle-settings" }),
    );

    let text = result["contents"][0]["text"].as_str().unwrap_or_default();
    assert!(text.len() < body.len(), "{} bytes", text.len());
    assert!(
        text.contains(&format!("[truncated: the file is {} bytes", body.len())),
        "{}",
        &text[text.len().saturating_sub(200)..]
    );
}
