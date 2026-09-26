//! Serialized samples of the values the backend sends the frontend.
//!
//! Each sample is a real Rust value run through serde, written to
//! `src/test/ipc-fixtures/fixtures.ts`. The frontend checks them against the
//! generated bindings (at compile time and at test time) and checks the mock
//! backend against them, so a serializer change that the bindings or the mock
//! do not follow fails a test.
//!
//! This test fails when the file is stale. Regenerate it with
//! `npm run generate:ipc-fixtures`.

use keynobi_lib::commands::device::DeviceListChangedEvent;
use keynobi_lib::commands::mcp::{McpClientSetupStatus, McpSetupStatus};
use keynobi_lib::models::ui_hierarchy::UiLayoutContext;
use keynobi_lib::models::*;
use keynobi_lib::services::agent_skill::{AgentSkillState, AgentSkillStatus};
use keynobi_lib::services::build_runner::{
    BUILD_COMPLETE_EVENT, BUILD_LINES_EVENT, BUILD_STARTED_EVENT,
};
use keynobi_lib::services::launch_display::BUILD_LAUNCH_TIMING_EVENT;
use keynobi_lib::services::mcp_activity::McpActivityEntry;
use keynobi_lib::services::mcp_sessions::{
    McpAttachedSession, McpServerStatus, McpStandaloneServer, SESSIONS_CHANGED_EVENT,
};
use keynobi_lib::services::monitor::MonitorStats;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

const UPDATE_ENV: &str = "UPDATE_IPC_FIXTURES";

#[derive(Default)]
struct Fixtures {
    /// TypeScript type name and its samples.
    types: Vec<(String, Vec<Value>)>,
    /// Event name, TypeScript payload type, and payload samples.
    events: Vec<(String, String, Vec<Value>)>,
}

fn to_values<T: Serialize>(samples: &[T]) -> Vec<Value> {
    samples
        .iter()
        .map(|s| serde_json::to_value(s).expect("IPC sample serializes"))
        .collect()
}

impl Fixtures {
    /// Samples of `T`, whose binding is named `ts_type`.
    fn add<T: Serialize>(&mut self, ts_type: &str, samples: &[T]) {
        assert!(!samples.is_empty(), "{ts_type} needs at least one sample");
        self.types.push((ts_type.to_string(), to_values(samples)));
    }

    /// Payload samples of the event `name`, typed `ts_type` in TypeScript.
    fn event<T: Serialize>(&mut self, name: &str, ts_type: &str, samples: &[T]) {
        assert!(!samples.is_empty(), "{name} needs at least one sample");
        self.events
            .push((name.to_string(), ts_type.to_string(), to_values(samples)));
    }

    fn render(&self) -> String {
        let mut imports: Vec<&str> = self
            .types
            .iter()
            .map(|(name, _)| name.as_str())
            .chain(self.events.iter().map(|(_, ty, _)| base_type(ty)))
            .filter(|name| name.chars().next().is_some_and(char::is_uppercase))
            .collect();
        imports.sort_unstable();
        imports.dedup();

        let mut out = String::new();
        out.push_str(
            "// Generated from the Rust IPC types by `npm run generate:ipc-fixtures`. Do not edit.\n",
        );
        out.push_str(&format!(
            "import type {{\n{}}} from \"@/bindings\";\n",
            imports
                .iter()
                .map(|name| format!("  {name},\n"))
                .collect::<String>()
        ));
        out.push_str("import type { Wire } from \"./wire\";\n\n");

        out.push_str("/** Serialized samples of each IPC type, as the backend sends them. */\n");
        out.push_str("export const typeFixtures = {\n");
        for (name, samples) in &self.types {
            out.push_str(&format!(
                "  {name}: {} satisfies Wire<{name}>[],\n",
                indent(&pretty(samples), 2)
            ));
        }
        out.push_str("};\n\n");

        out.push_str("/** Payload type and serialized payload samples of each backend event. */\n");
        out.push_str("export const eventFixtures = {\n");
        for (name, ty, samples) in &self.events {
            out.push_str(&format!(
                "  {}: {{\n    type: {},\n    samples: {} satisfies Wire<{ty}>[],\n  }},\n",
                serde_json::to_string(name).expect("event name serializes"),
                serde_json::to_string(ty).expect("type name serializes"),
                indent(&pretty(samples), 4)
            ));
        }
        out.push_str("};\n");
        out
    }
}

/// `Foo` for `Foo[]` or `Foo | null`.
fn base_type(ty: &str) -> &str {
    ty.trim_end_matches("[]")
        .split(" | ")
        .next()
        .unwrap_or(ty)
        .trim()
}

fn pretty(samples: &[Value]) -> String {
    serde_json::to_string_pretty(samples).expect("samples serialize")
}

fn indent(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line.to_string()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Samples ───────────────────────────────────────────────────────────────────
//
// Each type gets a sample with every optional value present and, where it has
// any, one with them absent: the frontend and the mock must handle both.

const PATH: &str = "/p/app/src/main/java/Main.kt";
const TIME: &str = "2026-04-23T10:00:00Z";

fn agent() -> BuildActor {
    BuildActor::Agent(AgentActor {
        session_id: Some(2),
        client_name: Some("Claude Code".into()),
        standalone: false,
    })
}

fn standalone_agent() -> BuildActor {
    BuildActor::Agent(AgentActor {
        session_id: None,
        client_name: None,
        standalone: true,
    })
}

/// Every kind of build actor.
fn actors() -> Vec<BuildActor> {
    vec![
        BuildActor::App,
        BuildActor::AppQuit,
        agent(),
        standalone_agent(),
    ]
}

fn build_lines() -> Vec<BuildLine> {
    vec![
        BuildLine {
            kind: BuildLineKind::Error,
            content: format!("e: file://{PATH}:10:5 Unresolved reference: foo"),
            file: Some(PATH.into()),
            line: Some(10),
            col: Some(5),
        },
        BuildLine::output("> Task :app:assembleDebug"),
    ]
}

fn build_errors() -> Vec<BuildError> {
    vec![
        BuildError {
            message: "Unresolved reference: foo".into(),
            file: Some(PATH.into()),
            line: Some(10),
            col: Some(5),
            severity: BuildErrorSeverity::Error,
        },
        BuildError {
            message: "Could not resolve com.example:lib:1.0".into(),
            file: None,
            line: None,
            col: None,
            severity: BuildErrorSeverity::Warning,
        },
    ]
}

/// A launch on an emulator with every value reported, and one on a device
/// that reported only the total time.
fn launch_timings() -> Vec<LaunchTiming> {
    vec![
        LaunchTiming {
            total_ms: 812,
            wait_ms: Some(815),
            launch_state: Some(LaunchState::Cold),
            measured_at: TIME.into(),
            serial: "emulator-5554".into(),
            avd_name: Some("Pixel_7_API_34".into()),
            model: Some("sdk_gphone64_arm64".into()),
            displayed_ms: Some(790),
            fully_drawn_ms: Some(1_400),
        },
        LaunchTiming {
            total_ms: 640,
            wait_ms: None,
            launch_state: None,
            measured_at: TIME.into(),
            serial: "28151FDH2000Q4".into(),
            avd_name: None,
            model: None,
            displayed_ms: None,
            fully_drawn_ms: None,
        },
    ]
}

fn mapping_snapshots() -> Vec<MappingSnapshot> {
    vec![
        MappingSnapshot {
            module: ":app".into(),
            variant: "release".into(),
            sha256: "6b1c2f0a".repeat(8),
            bytes: 48_213_771,
            pg_map_id: Some("6b1c2f0".into()),
        },
        MappingSnapshot {
            module: ":wear".into(),
            variant: "paidRelease".into(),
            sha256: "0f".repeat(32),
            bytes: 1_024,
            pg_map_id: None,
        },
    ]
}

fn built_apks() -> Vec<BuiltApk> {
    vec![
        BuiltApk {
            module: ":app".into(),
            variant: "release".into(),
            application_id: Some("com.example.app".into()),
            version_code: Some(42),
            sha256: "a1".repeat(32),
            bytes: 12_582_912,
            path: "app/build/outputs/apk/release/app-release.apk".into(),
        },
        BuiltApk {
            module: ":wear".into(),
            variant: "paidRelease".into(),
            application_id: None,
            version_code: None,
            sha256: "b2".repeat(32),
            bytes: 4_096,
            path: "wear/build/outputs/apk/paid/release/wear-paid-release.apk".into(),
        },
    ]
}

fn installed_builds() -> Vec<InstalledBuild> {
    vec![
        InstalledBuild {
            serial: "emulator-5554".into(),
            avd_name: Some("Pixel_7".into()),
            model: Some("sdk_gphone64_arm64".into()),
            package: "com.example.app".into(),
            apk_sha256: "a1".repeat(32),
            build_id: Some(20),
            version_code: Some(42),
            mappings: mapping_snapshots().into_iter().take(1).collect(),
            installed_at: TIME.into(),
        },
        InstalledBuild {
            serial: "R5CT1234ABC".into(),
            avd_name: None,
            model: None,
            package: "com.example.app.debug".into(),
            apk_sha256: "c3".repeat(32),
            build_id: None,
            version_code: None,
            mappings: vec![],
            installed_at: TIME.into(),
        },
    ]
}

fn retrace_outcomes() -> Vec<RetraceOutcome> {
    let obfuscated = "java.lang.RuntimeException: boom\n\tat a.a.onCreate(SourceFile:1)\n";
    vec![
        RetraceOutcome {
            status: RetraceStatus::Retraced,
            trace: "java.lang.RuntimeException: boom\n\
                    \tat com.example.app.MainActivity.onCreate(MainActivity.kt:24)\n"
                .into(),
            build_id: Some(20),
            mapping: mapping_snapshots().into_iter().next(),
            matched_by: Some(MappingMatch::DeviceHash),
            device: Some("Pixel_7".into()),
            package: Some("com.example.app".into()),
            reason: None,
            summary: "Deobfuscated with the R8 mapping of build #20 (:app release, map id \
                      6b1c2f0), matched by the SHA-256 of the APK on Pixel_7 (a1a1a1a1a1a1…), \
                      which build #20 wrote."
                .into(),
        },
        RetraceOutcome {
            status: RetraceStatus::Retraced,
            trace: "java.lang.RuntimeException: boom\n\
                    \tat com.example.app.MainActivity.onCreate(MainActivity.kt:24)\n"
                .into(),
            build_id: Some(20),
            mapping: mapping_snapshots().into_iter().next(),
            matched_by: Some(MappingMatch::MapId),
            device: Some("R5CT1234ABC".into()),
            package: None,
            reason: None,
            summary: "Deobfuscated with the R8 mapping of build #20 (:app release, map id \
                      6b1c2f0), matched by map id."
                .into(),
        },
        RetraceOutcome {
            status: RetraceStatus::Refused,
            trace: obfuscated.into(),
            build_id: None,
            mapping: None,
            matched_by: None,
            device: None,
            package: None,
            reason: Some(
                "logcat did not attribute the crash to a package, so its build is unknown".into(),
            ),
            summary: "Not deobfuscated: logcat did not attribute the crash to a package, so its \
                      build is unknown."
                .into(),
        },
    ]
}

fn build_result(success: bool) -> BuildResult {
    BuildResult {
        success,
        duration_ms: 4200,
        error_count: u32::from(!success),
        warning_count: 2,
    }
}

fn processed_entries() -> Vec<ProcessedEntry> {
    vec![
        ProcessedEntry {
            id: 41,
            timestamp: "2026-04-23T10:00:02.000Z".into(),
            pid: 1234,
            tid: 1236,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: "FATAL EXCEPTION: main".into(),
            package: Some("com.example.app".into()),
            kind: LogcatKind::Normal,
            is_crash: true,
            flags: EntryFlags::CRASH | EntryFlags::JSON_BODY,
            category: EntryCategory::General,
            crash_group_id: Some(41),
            json_body: Some("{\"ok\":false}".into()),
        },
        ProcessedEntry {
            id: 42,
            timestamp: "2026-04-23T10:00:03.000Z".into(),
            pid: 1300,
            tid: 1300,
            level: LogcatLevel::Info,
            tag: "ActivityManager".into(),
            message: "Process com.example.app has died".into(),
            package: None,
            kind: LogcatKind::ProcessDied,
            is_crash: false,
            flags: 0,
            category: EntryCategory::Lifecycle,
            crash_group_id: None,
            json_body: None,
        },
    ]
}

fn devices() -> Vec<Device> {
    vec![
        Device {
            serial: "emulator-5554".into(),
            name: "Pixel 7".into(),
            model: Some("sdk_gphone64_arm64".into()),
            device_kind: DeviceKind::Emulator,
            connection_state: DeviceConnectionState::Online,
            api_level: Some(34),
            android_version: Some("14".into()),
            avd_name: Some("Pixel_7_API_34".into()),
        },
        Device {
            serial: "28151FDH2000Q4".into(),
            name: "Pixel 7".into(),
            model: Some("Pixel 7".into()),
            device_kind: DeviceKind::Physical,
            connection_state: DeviceConnectionState::Online,
            api_level: Some(35),
            android_version: Some("15".into()),
            avd_name: None,
        },
        Device {
            serial: "ZX1G22ABCD".into(),
            name: "ZX1G22ABCD".into(),
            model: None,
            device_kind: DeviceKind::Physical,
            connection_state: DeviceConnectionState::Unauthorized,
            api_level: None,
            android_version: None,
            avd_name: None,
        },
    ]
}

fn project_entries() -> Vec<ProjectEntry> {
    vec![
        ProjectEntry {
            id: "3f2a".into(),
            path: "/p".into(),
            name: "Sample".into(),
            gradle_root: Some("/p".into()),
            last_opened: TIME.into(),
            pinned: true,
            last_build_variant: Some("debug".into()),
            last_device: Some("emulator-5554".into()),
            trusted: Some(true),
        },
        ProjectEntry {
            id: "9c1d".into(),
            path: "/q".into(),
            name: "Other".into(),
            gradle_root: None,
            last_opened: TIME.into(),
            pinned: false,
            last_build_variant: None,
            last_device: None,
            trusted: None,
        },
    ]
}

fn attached_sessions() -> Vec<McpAttachedSession> {
    vec![
        McpAttachedSession {
            id: 2,
            pid: Some(4321),
            project: Some("/p".into()),
            connected_at: TIME.into(),
            client_name: Some("Claude Code".into()),
            version: "0.1.29".into(),
        },
        McpAttachedSession {
            id: 3,
            pid: None,
            project: None,
            connected_at: TIME.into(),
            client_name: None,
            version: "0.1.28".into(),
        },
    ]
}

fn client_setup(configured: bool) -> McpClientSetupStatus {
    McpClientSetupStatus {
        client_found: configured,
        is_configured: configured,
        configured_command: configured
            .then(|| "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp".into()),
        configured_scope: configured.then(|| "user".into()),
        setup_command: configured.then(|| {
            "claude mcp add --scope user --transport stdio keynobi -- keynobi --mcp".into()
        }),
    }
}

fn ui_node(children: Vec<UiNode>) -> UiNode {
    UiNode {
        class: "android.widget.FrameLayout".into(),
        resource_id: "com.example.app:id/root".into(),
        text: "Sign in".into(),
        content_desc: "".into(),
        package: "com.example.app".into(),
        bounds: "[0,0][1080,2400]".into(),
        clickable: true,
        enabled: true,
        focusable: false,
        focused: false,
        scrollable: false,
        long_clickable: false,
        password: false,
        checkable: false,
        checked: false,
        editable: false,
        selected: false,
        is_compose_heuristic: false,
        children,
    }
}

fn fixtures() -> Fixtures {
    let mut f = Fixtures::default();

    // Errors every `Result<_, AppError>` command rejects with.
    f.add(
        "AppError",
        &[
            AppError::NotFound("settings.json".into()),
            AppError::PermissionDenied("/p".into()),
            AppError::InvalidInput("bad task name".into()),
            AppError::Io("'/p': disk full".into()),
            AppError::ProcessFailed("adb exited with 1".into()),
            AppError::SettingsError("unreadable".into()),
            AppError::McpError("not listening".into()),
            AppError::Other("unexpected".into()),
        ],
    );

    // Projects and settings.
    f.add("ProjectEntry", &project_entries());
    f.add(
        "ProjectAppInfo",
        &[
            ProjectAppInfo {
                application_id: Some("com.example.app".into()),
                version_name: Some("1.2.3".into()),
                version_code: Some(42),
                version_name_unavailable: None,
                version_code_unavailable: None,
            },
            ProjectAppInfo {
                application_id: None,
                version_name: None,
                version_code: None,
                version_name_unavailable: Some("No versionName assignment found in app/build.gradle.kts".into()),
                version_code_unavailable: Some(
                    "versionCode in app/build.gradle.kts (line 7) is set by `libs.versions.code.get().toInt()`".into(),
                ),
            },
        ],
    );
    let mut configured = AppSettings::default();
    configured.android.sdk_path = Some("/sdk".into());
    configured.java.home = Some("/jdk".into());
    configured.recent_projects = project_entries();
    configured.last_active_project = Some("/p".into());
    f.add("AppSettings", &[configured, AppSettings::default()]);
    f.add(
        "SystemHealthReport",
        &[
            SystemHealthReport {
                java_executable_found: true,
                java_version: Some("openjdk version \"17.0.9\"".into()),
                java_bin_used: "/jdk/bin/java".into(),
                java_major_version: Some(17),
                java_home: Some("/jdk".into()),
                java_source: Some(JdkSource::AndroidStudio),
                android_sdk_valid: true,
                adb_found: true,
                adb_version: Some("Android Debug Bridge version 1.0.41".into()),
                emulator_found: true,
                gradle_wrapper_found: true,
                lsp_system_dir_ok: true,
                studio_command_found: false,
                app_location_problem: None,
                retrace_version: Some("22.0".into()),
                android_cli_path: Some("/opt/homebrew/Cellar/android-cli/1.0/bin/android".into()),
                android_cli_version: Some("1.0.16406183".into()),
            },
            SystemHealthReport {
                java_executable_found: false,
                java_version: None,
                java_bin_used: "java".into(),
                java_major_version: None,
                java_home: None,
                java_source: None,
                android_sdk_valid: false,
                adb_found: false,
                adb_version: None,
                emulator_found: false,
                gradle_wrapper_found: false,
                lsp_system_dir_ok: true,
                studio_command_found: false,
                app_location_problem: Some("Keynobi is running from a disk image.".into()),
                retrace_version: None,
                android_cli_path: None,
                android_cli_version: None,
            },
        ],
    );

    // Builds.
    f.add(
        "BuildStatus",
        &[
            BuildStatus::Idle,
            BuildStatus::Running {
                task: "assembleDebug".into(),
                started_at: TIME.into(),
            },
            BuildStatus::Success(build_result(true)),
            BuildStatus::Failed(build_result(false)),
            BuildStatus::Cancelled,
        ],
    );
    f.add("BuildLine", &build_lines());
    f.add("BuildError", &build_errors());
    let records: Vec<BuildRecord> = actors()
        .into_iter()
        .map(Some)
        .chain([None])
        .enumerate()
        .map(|(i, actor)| BuildRecord {
            id: 7 + i as u32,
            task: "assembleDebug".into(),
            status: match &actor {
                Some(_) => BuildStatus::Cancelled,
                None => BuildStatus::Failed(build_result(false)),
            },
            errors: if actor.is_none() {
                build_errors()
            } else {
                vec![]
            },
            started_at: TIME.into(),
            project_root: actor.as_ref().map(|_| "/p".to_string()),
            origin: actor.clone(),
            cancelled_by: actor,
            launch: None,
            mappings: vec![],
            apks: vec![],
        })
        .chain(launch_timings().into_iter().map(|launch| BuildRecord {
            id: 20,
            task: "assembleDebug".into(),
            status: BuildStatus::Success(build_result(true)),
            errors: vec![],
            started_at: TIME.into(),
            project_root: Some("/p".into()),
            origin: Some(BuildActor::App),
            cancelled_by: None,
            launch: Some(launch),
            mappings: mapping_snapshots(),
            apks: built_apks(),
        }))
        .collect();
    f.add("BuildRecord", &records);
    f.add("MappingSnapshot", &mapping_snapshots());
    f.add("BuiltApk", &built_apks());
    f.add(
        "RunApk",
        &[
            RunApk {
                path: "/work/app/build/outputs/apk/debug/app-debug.apk".into(),
                build_id: Some(21),
                from_this_build: true,
            },
            RunApk {
                path: "/work/app/build/outputs/apk/debug/app-debug.apk".into(),
                build_id: None,
                from_this_build: false,
            },
        ],
    );
    f.add("InstalledBuild", &installed_builds());
    f.add(
        "LaunchState",
        &[
            LaunchState::Cold,
            LaunchState::Warm,
            LaunchState::Hot,
            LaunchState::Relaunch,
        ],
    );
    f.add("LaunchTiming", &launch_timings());
    let launch_events: Vec<LaunchTimingEvent> = launch_timings()
        .into_iter()
        .map(|launch| LaunchTimingEvent {
            record_id: 12,
            launch,
        })
        .collect();
    f.add("LaunchTimingEvent", &launch_events);
    f.add(
        "LaunchResult",
        &[
            LaunchResult {
                output: "am start OK: Status: ok".into(),
                timing: launch_timings().into_iter().next(),
            },
            // Returned before the app reported it was fully drawn.
            LaunchResult {
                output: "am start OK: Status: ok".into(),
                timing: launch_timings().into_iter().next().map(|t| LaunchTiming {
                    fully_drawn_ms: None,
                    ..t
                }),
            },
            LaunchResult {
                output: "monkey OK: Events injected: 1".into(),
                timing: None,
            },
        ],
    );
    f.add(
        "VariantList",
        &[
            VariantList {
                variants: vec![BuildVariant {
                    name: "freeDebug".into(),
                    build_type: "debug".into(),
                    flavors: vec!["free".into()],
                    assemble_task: "assembleFreeDebug".into(),
                    install_task: "installFreeDebug".into(),
                }],
                active: Some("freeDebug".into()),
                default_variant: Some("freeDebug".into()),
            },
            VariantList::default(),
        ],
    );
    let started: Vec<BuildStartedEvent> = actors()
        .into_iter()
        .zip([Some("/p".to_string()), None].into_iter().cycle())
        .enumerate()
        .map(|(i, (origin, project_root))| BuildStartedEvent {
            run_id: 9001 + i as u32,
            task: "assembleDebug".into(),
            origin,
            started_at: TIME.into(),
            project_root,
        })
        .collect();
    let lines = [BuildLinesEvent {
        run_id: 9001,
        lines: build_lines(),
    }];
    let complete: Vec<BuildCompleteEvent> = actors()
        .into_iter()
        .map(Some)
        .chain([None])
        .enumerate()
        .map(|(i, actor)| BuildCompleteEvent {
            run_id: 9001 + i as u32,
            record_id: 7 + i as u32,
            success: actor.is_none(),
            cancelled: actor.is_some(),
            duration_ms: 4200,
            error_count: 0,
            warning_count: 1,
            task: "assembleDebug".into(),
            origin: actor.clone(),
            cancelled_by: actor,
        })
        .collect();
    f.add("BuildStartedEvent", &started);
    f.add("BuildLinesEvent", &lines);
    f.add("BuildCompleteEvent", &complete);

    // Devices and emulators.
    f.add("Device", &devices());
    f.add("RetraceOutcome", &retrace_outcomes());
    f.add(
        "RetraceStatus",
        &[
            RetraceStatus::Retraced,
            RetraceStatus::Unavailable,
            RetraceStatus::Refused,
            RetraceStatus::Failed,
        ],
    );
    f.add(
        "MappingMatch",
        &[
            MappingMatch::MapId,
            MappingMatch::DeviceHash,
            MappingMatch::InstallRecord,
        ],
    );
    let device_list = [DeviceListChangedEvent { devices: devices() }];
    f.add("DeviceListChangedEvent", &device_list);
    f.add(
        "AppExitReasons",
        &[
            AppExitReasons {
                serial: "emulator-5554".into(),
                package: "com.example.app.debug".into(),
                api_level: Some(34),
                supported: true,
                message: None,
                records: vec![
                    AppExitRecord {
                        timestamp: Some("2024-01-09 08:12:44.310".into()),
                        timestamp_local: Some("2024-01-09T08:12:44.310".into()),
                        pid: Some(31020),
                        process_name: Some("com.example.app.debug".into()),
                        reason: AppExitReason::Crash,
                        reason_code: Some(4),
                        reason_label: Some("APP CRASH(EXCEPTION)".into()),
                        sub_reason_code: Some(0),
                        sub_reason: Some("UNKNOWN".into()),
                        status: Some(0),
                        importance: Some(100),
                        importance_name: Some("foreground".into()),
                        pss_kb: Some(56_320),
                        rss_kb: Some(130_048),
                        description: Some("crash".into()),
                    },
                    AppExitRecord {
                        timestamp: None,
                        timestamp_local: None,
                        pid: None,
                        process_name: None,
                        reason: AppExitReason::Unknown,
                        reason_code: None,
                        reason_label: None,
                        sub_reason_code: None,
                        sub_reason: None,
                        status: None,
                        importance: None,
                        importance_name: None,
                        pss_kb: None,
                        rss_kb: None,
                        description: None,
                    },
                ],
                total_records: 2,
            },
            AppExitReasons {
                serial: "emulator-5556".into(),
                package: "com.example.app".into(),
                api_level: None,
                supported: false,
                message: Some(
                    "Process exit reasons need Android 11 (API 30) or later; emulator-5556 runs API 29."
                        .into(),
                ),
                records: vec![],
                total_records: 0,
            },
        ],
    );
    f.add(
        "AvdInfo",
        &[
            AvdInfo {
                name: "Pixel_7_API_34".into(),
                display_name: "Pixel 7 API 34".into(),
                target: Some("android-34".into()),
                api_level: Some(34),
                abi: Some("arm64-v8a".into()),
                path: "/home/.android/avd/Pixel_7_API_34.avd".into(),
            },
            AvdInfo {
                name: "Broken".into(),
                display_name: "Broken".into(),
                target: None,
                api_level: None,
                abi: None,
                path: "/home/.android/avd/Broken.avd".into(),
            },
        ],
    );
    f.add(
        "SystemImageInfo",
        &[SystemImageInfo {
            sdk_id: "system-images;android-34;google_apis;arm64-v8a".into(),
            api_level: 34,
            variant: "google_apis".into(),
            abi: "arm64-v8a".into(),
            display_name: "Android 14 (Google APIs) · arm64-v8a".into(),
        }],
    );
    f.add(
        "DeviceDefinition",
        &[DeviceDefinition {
            id: "pixel_7".into(),
            name: "Pixel 7".into(),
            manufacturer: "Google".into(),
        }],
    );
    f.add(
        "AvailableSystemImage",
        &[AvailableSystemImage {
            sdk_id: "system-images;android-35;google_apis;arm64-v8a".into(),
            api_level: 35,
            variant: "google_apis".into(),
            abi: "arm64-v8a".into(),
            display_name: "Android 15 (Google APIs) · arm64-v8a".into(),
            installed: false,
        }],
    );
    f.add(
        "SdkDownloadProgress",
        &[
            SdkDownloadProgress {
                percent: Some(40),
                message: "Downloading...".into(),
                done: false,
                error: false,
            },
            SdkDownloadProgress {
                percent: None,
                message: "Installing...".into(),
                done: true,
                error: false,
            },
        ],
    );
    f.add(
        "UiHierarchySnapshot",
        &[
            UiHierarchySnapshot {
                captured_at: TIME.into(),
                truncated: false,
                warnings: vec!["compose semantics missing".into()],
                root: ui_node(vec![ui_node(vec![])]),
                screen_hash: "ab12".into(),
                interactive_count: 1,
                foreground_activity: Some("com.example.app/.MainActivity".into()),
                layout_context: UiLayoutContext {
                    window_excerpt: Some("mCurrentFocus=Window{…}".into()),
                    display_excerpt: Some("mBaseDisplayInfo=…".into()),
                    wm_size: Some("Physical size: 1080x2400".into()),
                    wm_density: Some("Physical density: 420".into()),
                },
                command_log: vec!["adb -s emulator-5554 shell uiautomator dump".into()],
                screenshot_b64: Some("iVBORw0KGgo=".into()),
            },
            UiHierarchySnapshot {
                captured_at: TIME.into(),
                truncated: true,
                warnings: vec![],
                root: ui_node(vec![]),
                screen_hash: "cd34".into(),
                interactive_count: 0,
                foreground_activity: None,
                layout_context: UiLayoutContext::default(),
                command_log: vec![],
                screenshot_b64: None,
            },
        ],
    );

    // Logcat.
    f.add("ProcessedEntry", &processed_entries());
    f.add(
        "LogStats",
        &[LogStats {
            total_ingested: 120,
            counts_by_level: [1, 2, 3, 4, 5, 6, 7],
            crash_count: 1,
            json_count: 2,
            packages_seen: 3,
            buffer_usage_pct: 0.5,
            buffer_entry_count: 100,
            dropped_lines: 4,
            backlog_lines: 5,
        }],
    );

    // MCP.
    f.add("McpAttachedSession", &attached_sessions());
    f.add(
        "McpServerStatus",
        &[McpServerStatus {
            listening: true,
            app_version: "0.1.29".into(),
            attached: attached_sessions(),
            standalone: vec![
                McpStandaloneServer {
                    pid: 5555,
                    started_at: TIME.into(),
                    project: Some("/p".into()),
                    reason: "the Keynobi app is not running".into(),
                    version: Some("0.1.29".into()),
                    exe: None,
                },
                McpStandaloneServer {
                    pid: 5556,
                    started_at: TIME.into(),
                    project: None,
                    reason: "the Keynobi app is not running".into(),
                    version: None,
                    exe: None,
                },
            ],
        }],
    );
    f.add(
        "McpActivityEntry",
        &[
            McpActivityEntry {
                timestamp: TIME.into(),
                kind: "tool_call".into(),
                name: "get_project_info".into(),
                duration_ms: Some(12),
                status: "ok".into(),
                summary: Some("project open".into()),
            },
            McpActivityEntry {
                timestamp: TIME.into(),
                kind: "lifecycle".into(),
                name: "Server started".into(),
                duration_ms: None,
                status: "ok".into(),
                summary: None,
            },
        ],
    );
    f.add(
        "McpSetupStatus",
        &[
            McpSetupStatus {
                exe_path: "/Applications/Keynobi.app/Contents/MacOS/keynobi".into(),
                setup_command: Some(
                    "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp".into(),
                ),
                location_problem: None,
                claude: client_setup(true),
                codex: client_setup(true),
            },
            McpSetupStatus {
                exe_path: "/Volumes/Keynobi/Keynobi.app/Contents/MacOS/keynobi".into(),
                setup_command: None,
                location_problem: Some("Keynobi is running from a disk image.".into()),
                claude: client_setup(false),
                codex: client_setup(false),
            },
        ],
    );
    f.add(
        "AgentSkillStatus",
        &[
            AgentSkillStatus {
                path: "/Users/me/.claude/skills/keynobi/SKILL.md".into(),
                state: AgentSkillState::NotInstalled,
                content: "---\nname: keynobi\ndescription: When to use Keynobi.\n---\n".into(),
                resource_uri: "keynobi://skill".into(),
            },
            AgentSkillStatus {
                path: "/Users/me/.claude/skills/keynobi/SKILL.md".into(),
                state: AgentSkillState::Installed,
                content: "---\nname: keynobi\ndescription: When to use Keynobi.\n---\n".into(),
                resource_uri: "keynobi://skill".into(),
            },
            AgentSkillStatus {
                path: "/Users/me/.claude/skills/keynobi/SKILL.md".into(),
                state: AgentSkillState::Different,
                content: "---\nname: keynobi\ndescription: When to use Keynobi.\n---\n".into(),
                resource_uri: "keynobi://skill".into(),
            },
        ],
    );
    let stats = [MonitorStats {
        app_memory_bytes: 123_456_789,
        log_folder_bytes: 4096,
        rotation_triggered: false,
    }];
    f.add("MonitorStats", &stats);

    // Events, with the payload each emit site sends.
    f.event(BUILD_STARTED_EVENT, "BuildStartedEvent", &started);
    f.event(BUILD_LINES_EVENT, "BuildLinesEvent", &lines);
    f.event(BUILD_COMPLETE_EVENT, "BuildCompleteEvent", &complete);
    f.event(
        BUILD_LAUNCH_TIMING_EVENT,
        "LaunchTimingEvent",
        &launch_events,
    );
    f.event(
        "device:list_changed",
        "DeviceListChangedEvent",
        &device_list,
    );
    f.event("logcat:entries", "ProcessedEntry[]", &[processed_entries()]);
    f.event("logcat:cleared", "null", &[()]);
    f.event("logcat:reconnecting", "null", &[()]);
    f.event(
        "logcat:stopped",
        "string",
        &["Logcat stopped: adb is not responding"],
    );
    f.event(
        SESSIONS_CHANGED_EVENT,
        "McpAttachedSession[]",
        &[attached_sessions()],
    );
    f.event("settings:corrupted", "null", &[()]);
    f.event("monitor://stats", "MonitorStats", &stats);

    f
}

#[test]
fn ipc_fixtures_are_up_to_date() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/test/ipc-fixtures/fixtures.ts");
    let rendered = fixtures().render();

    if std::env::var_os(UPDATE_ENV).is_some() {
        std::fs::write(&path, &rendered).expect("write IPC fixtures");
        return;
    }

    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current != rendered {
        let first_diff = current
            .lines()
            .zip(rendered.lines())
            .position(|(a, b)| a != b)
            .map(|i| format!("first difference at line {}", i + 1))
            .unwrap_or_else(|| "the files differ in length".to_string());
        panic!(
            "{} is stale ({first_diff}): an IPC type's serialized form changed. \
             Run `npm run generate:ipc-fixtures`, then fix the bindings, the frontend, \
             and the mock backend until the frontend tests pass.",
            path.display()
        );
    }
}
