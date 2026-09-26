//! Comparing two debug sessions (MCP `compare_debug_sessions`): what differs
//! between the build that ran cleanly and the one that crashed. Only what
//! sessions record today is compared; the build's provenance (source commit,
//! build files) is not recorded yet and is listed as such.

use super::*;
use crate::models::build::LaunchState;
use std::collections::BTreeMap;

/// Most crash signatures listed per session.
pub const MAX_COMPARED_SIGNATURES: usize = 20;

/// What a comparison cannot say, so a reader does not over-read the diff.
pub const NOT_RECORDED: &[&str] = &[
    "source commit and branch",
    "hashes of the dependency and build files",
    "resolved dependency versions",
    "Android Gradle Plugin version",
    "uncommitted changes",
];

/// One session, as the comparison describes it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparedSession {
    pub id: String,
    pub package: String,
    pub project_root: Option<String>,
    /// `open`, `superseded`, `ended`, or `idle`.
    pub state: &'static str,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub device: String,
    pub serial: String,
    pub device_model: Option<String>,
    pub build_id: Option<u32>,
    pub task: Option<String>,
    pub module: Option<String>,
    pub variant: Option<String>,
    pub version_code: Option<u32>,
    pub apk_sha256: Option<String>,
    pub mapping_sha256s: Vec<String>,
    pub map_ids: Vec<String>,
    pub installed_by: Option<String>,
    pub recorded_by: &'static str,
    pub launches: u32,
    pub crashes: u32,
    pub anrs: u32,
    pub exits: u32,
}

/// A field whose value differs between the two sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    pub field: &'static str,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// The latest launch of a session with a measured time.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaunchSnapshot {
    pub total_ms: u32,
    pub launch_state: Option<LaunchState>,
    pub displayed_ms: Option<u32>,
    pub fully_drawn_ms: Option<u32>,
    pub serial: String,
    pub avd_name: Option<String>,
    pub measured_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaunchComparison {
    pub from: Option<LaunchSnapshot>,
    pub to: Option<LaunchSnapshot>,
    /// `to` minus `from`; positive is slower. Only when `comparable`.
    pub delta_ms: Option<i64>,
    /// Same device and same launch state, as the Builds list compares.
    pub comparable: bool,
    pub note: Option<String>,
}

/// Crashes or ANRs that share their first line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrashSignature {
    pub signature: String,
    /// `crash` or `anr`.
    pub kind: &'static str,
    pub summary: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrashComparison {
    pub from: Vec<CrashSignature>,
    pub to: Vec<CrashSignature>,
    /// Signatures `to` has and `from` does not: the likely regression.
    pub new_in_to: Vec<String>,
    /// Signatures `from` has and `to` does not.
    pub gone_in_to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExitComparison {
    /// Exit records by reason.
    pub from: BTreeMap<String, u32>,
    pub to: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionComparison {
    /// `given`, or `last passing vs first crashing`.
    pub chosen_by: &'static str,
    pub from: ComparedSession,
    pub to: ComparedSession,
    pub differences: Vec<FieldChange>,
    pub launch: LaunchComparison,
    pub crashes: CrashComparison,
    pub exits: ExitComparison,
    /// The builds' provenance; not recorded yet, so always `None`.
    pub provenance: Option<serde_json::Value>,
    pub not_recorded: Vec<&'static str>,
}

fn state_name(session: &DebugSession) -> &'static str {
    match (session.closed_at.is_some(), session.close_reason) {
        (false, _) => "open",
        (true, Some(DebugSessionCloseReason::Superseded)) => "superseded",
        (true, Some(DebugSessionCloseReason::Idle)) => "idle",
        (true, _) => "ended",
    }
}

fn actor_name(actor: &BuildActor) -> String {
    match actor {
        BuildActor::App => "app".into(),
        BuildActor::AppQuit => "app quitting".into(),
        BuildActor::Agent(agent) => match (&agent.client_name, agent.standalone) {
            (Some(name), true) => format!("agent {name} (standalone)"),
            (Some(name), false) => format!("agent {name}"),
            (None, true) => "agent (standalone)".into(),
            (None, false) => "agent".into(),
        },
    }
}

fn compared(session: &DebugSession) -> ComparedSession {
    let build = session.build.as_ref();
    ComparedSession {
        id: session.id.clone(),
        package: session.package.clone(),
        project_root: session.project_root.clone(),
        state: state_name(session),
        opened_at: session.opened_at.clone(),
        closed_at: session.closed_at.clone(),
        device: session
            .device
            .avd_name
            .clone()
            .unwrap_or_else(|| session.device.serial.clone()),
        serial: session.device.serial.clone(),
        device_model: session.device.model.clone(),
        build_id: build.map(|b| b.id),
        task: build.map(|b| b.task.clone()),
        module: build.map(|b| b.apk.module.clone()),
        variant: build.map(|b| b.apk.variant.clone()),
        version_code: build
            .and_then(|b| b.apk.version_code)
            .or(session.install.as_ref().and_then(|i| i.version_code)),
        apk_sha256: session.install.as_ref().map(|i| i.apk_sha256.clone()),
        mapping_sha256s: build
            .map(|b| b.mappings.iter().map(|m| m.sha256.clone()).collect())
            .unwrap_or_default(),
        map_ids: build
            .map(|b| {
                b.mappings
                    .iter()
                    .filter_map(|m| m.pg_map_id.clone())
                    .collect()
            })
            .unwrap_or_default(),
        installed_by: session.install.as_ref().map(|i| actor_name(&i.by)),
        recorded_by: match session.recorded_by {
            DebugSessionRecorder::App => "app",
            DebugSessionRecorder::Standalone => "standalone",
        },
        launches: session.counts.launches,
        crashes: session.counts.crashes,
        anrs: session.counts.anrs,
        exits: session.counts.exits,
    }
}

fn differences(from: &ComparedSession, to: &ComparedSession) -> Vec<FieldChange> {
    let text = |v: &Option<u32>| v.map(|n| n.to_string());
    let list = |v: &[String]| (!v.is_empty()).then(|| v.join(", "));
    let fields: [(&'static str, Option<String>, Option<String>); 12] = [
        ("build_id", text(&from.build_id), text(&to.build_id)),
        ("task", from.task.clone(), to.task.clone()),
        ("module", from.module.clone(), to.module.clone()),
        ("variant", from.variant.clone(), to.variant.clone()),
        (
            "version_code",
            text(&from.version_code),
            text(&to.version_code),
        ),
        ("apk_sha256", from.apk_sha256.clone(), to.apk_sha256.clone()),
        (
            "mapping_sha256s",
            list(&from.mapping_sha256s),
            list(&to.mapping_sha256s),
        ),
        ("map_ids", list(&from.map_ids), list(&to.map_ids)),
        ("device", Some(from.device.clone()), Some(to.device.clone())),
        (
            "device_model",
            from.device_model.clone(),
            to.device_model.clone(),
        ),
        (
            "installed_by",
            from.installed_by.clone(),
            to.installed_by.clone(),
        ),
        (
            "recorded_by",
            Some(from.recorded_by.to_string()),
            Some(to.recorded_by.to_string()),
        ),
    ];
    fields
        .into_iter()
        .filter(|(_, a, b)| a != b)
        .map(|(field, from, to)| FieldChange { field, from, to })
        .collect()
}

/// The session's latest measured launch, with display times that arrived later.
fn latest_launch(events: &[DebugSessionEvent]) -> Option<LaunchSnapshot> {
    let mut latest: Option<LaunchTiming> = None;
    for event in events {
        match &event.event {
            DebugSessionEventData::Launch(DebugSessionLaunch {
                timing: Some(timing),
                ..
            }) => latest = Some(timing.clone()),
            DebugSessionEventData::LaunchTiming(timing)
                if latest
                    .as_ref()
                    .is_some_and(|l| l.measured_at == timing.measured_at) =>
            {
                latest = Some(timing.clone())
            }
            _ => {}
        }
    }
    latest.map(|t| LaunchSnapshot {
        total_ms: t.total_ms,
        launch_state: t.launch_state,
        displayed_ms: t.displayed_ms,
        fully_drawn_ms: t.fully_drawn_ms,
        serial: t.serial,
        avd_name: t.avd_name,
        measured_at: t.measured_at,
    })
}

fn compare_launches(from: Option<LaunchSnapshot>, to: Option<LaunchSnapshot>) -> LaunchComparison {
    let (comparable, note) = match (&from, &to) {
        (Some(a), Some(b)) => {
            let same_device = if a.avd_name.is_some() || b.avd_name.is_some() {
                a.avd_name == b.avd_name
            } else {
                a.serial == b.serial
            };
            if !same_device {
                (
                    false,
                    Some("The launches ran on different devices.".to_string()),
                )
            } else if a.launch_state != b.launch_state {
                (
                    false,
                    Some(format!(
                        "The launches started differently ({} vs {}).",
                        state_label(a.launch_state),
                        state_label(b.launch_state)
                    )),
                )
            } else {
                (true, None)
            }
        }
        (None, Some(_)) => (false, Some("`from` has no measured launch.".into())),
        (Some(_), None) => (false, Some("`to` has no measured launch.".into())),
        (None, None) => (false, Some("Neither session has a measured launch.".into())),
    };
    let delta_ms = match (&from, &to) {
        (Some(a), Some(b)) if comparable => Some(i64::from(b.total_ms) - i64::from(a.total_ms)),
        _ => None,
    };
    LaunchComparison {
        from,
        to,
        delta_ms,
        comparable,
        note,
    }
}

fn state_label(state: Option<LaunchState>) -> &'static str {
    match state {
        Some(LaunchState::Cold) => "cold",
        Some(LaunchState::Warm) => "warm",
        Some(LaunchState::Hot) => "hot",
        Some(LaunchState::Relaunch) => "relaunch",
        None => "unknown",
    }
}

fn signatures(crashes: &[DebugSessionEvent]) -> Vec<CrashSignature> {
    let mut by_signature: Vec<CrashSignature> = Vec::new();
    for event in crashes {
        let (kind, crash) = match &event.event {
            DebugSessionEventData::Crash(c) => ("crash", c),
            DebugSessionEventData::Anr(c) => ("anr", c),
            _ => continue,
        };
        match by_signature
            .iter_mut()
            .find(|s| s.signature == crash.signature && s.kind == kind)
        {
            Some(existing) => existing.count += 1,
            None => by_signature.push(CrashSignature {
                signature: crash.signature.clone(),
                kind,
                summary: crash.summary.clone(),
                count: 1,
            }),
        }
    }
    by_signature.sort_by(|a, b| b.count.cmp(&a.count).then(a.signature.cmp(&b.signature)));
    by_signature.truncate(MAX_COMPARED_SIGNATURES);
    by_signature
}

fn exit_reasons(events: &[DebugSessionEvent]) -> BTreeMap<String, u32> {
    let mut reasons = BTreeMap::new();
    for event in events {
        if let DebugSessionEventData::Exit(exit) = &event.event {
            let reason = serde_json::to_value(exit.record.reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".into());
            *reasons.entry(reason).or_insert(0) += 1;
        }
    }
    reasons
}

/// Compare two sessions as read with `get_session`.
pub fn compare_details(
    from: &DebugSessionDetail,
    to: &DebugSessionDetail,
    chosen_by: &'static str,
) -> SessionComparison {
    let a = compared(&from.session);
    let b = compared(&to.session);
    let from_signatures = signatures(&from.crashes);
    let to_signatures = signatures(&to.crashes);
    let only_in = |these: &[CrashSignature], those: &[CrashSignature]| -> Vec<String> {
        these
            .iter()
            .filter(|s| !those.iter().any(|o| o.signature == s.signature))
            .map(|s| s.signature.clone())
            .collect()
    };
    SessionComparison {
        chosen_by,
        differences: differences(&a, &b),
        launch: compare_launches(latest_launch(&from.events), latest_launch(&to.events)),
        crashes: CrashComparison {
            new_in_to: only_in(&to_signatures, &from_signatures),
            gone_in_to: only_in(&from_signatures, &to_signatures),
            from: from_signatures,
            to: to_signatures,
        },
        exits: ExitComparison {
            from: exit_reasons(&from.events),
            to: exit_reasons(&to.events),
        },
        from: a,
        to: b,
        provenance: None,
        not_recorded: NOT_RECORDED.to_vec(),
    }
}

fn is_passing(s: &DebugSessionSummary) -> bool {
    s.counts.launches > 0 && s.counts.crashes == 0 && s.counts.anrs == 0
}

fn is_crashing(s: &DebugSessionSummary) -> bool {
    s.counts.crashes > 0 || s.counts.anrs > 0
}

fn same_app(a: &DebugSessionSummary, b: &DebugSessionSummary) -> bool {
    a.project_root == b.project_root
        && a.package == b.package
        && a.module == b.module
        && a.variant == b.variant
}

/// The default pair: the newest session with a launch and no crash or ANR,
/// and the first later session of the same project, package, module, and
/// variant that crashed. `sessions` is newest first, as the list returns it.
pub fn default_pair(sessions: &[DebugSessionSummary]) -> Option<(String, String)> {
    let oldest_first: Vec<&DebugSessionSummary> = sessions.iter().rev().collect();
    for (i, passing) in oldest_first.iter().enumerate().rev() {
        if !is_passing(passing) {
            continue;
        }
        let crashing = oldest_first[i + 1..]
            .iter()
            .find(|s| is_crashing(s) && same_app(passing, s));
        if let Some(crashing) = crashing {
            return Some((passing.id.clone(), crashing.id.clone()));
        }
    }
    None
}

/// Compare sessions `from` and `to`, or, with neither, the default pair.
pub fn compare_sessions(
    from: Option<&str>,
    to: Option<&str>,
) -> Result<SessionComparison, AppError> {
    compare_sessions_in(&data_dir(), from, to, Utc::now())
}

pub(super) fn compare_sessions_in(
    data_dir: &Path,
    from: Option<&str>,
    to: Option<&str>,
    now: DateTime<Utc>,
) -> Result<SessionComparison, AppError> {
    let (from, to, chosen_by) = match (from, to) {
        (Some(from), Some(to)) => (from.to_string(), to.to_string(), "given"),
        (None, None) => {
            let (from, to) = default_pair(&list_sessions_in(data_dir, now)).ok_or_else(|| {
                AppError::NotFound(
                    "No two sessions to compare by default: that needs a session with a launch \
                     and no crash or ANR, and a later crashing session of the same app, module, \
                     and variant. Name the sessions with from and to."
                        .into(),
                )
            })?;
            (from, to, "last passing vs first crashing")
        }
        _ => {
            return Err(AppError::InvalidInput(
                "Pass both from and to, or neither for the default pair.".into(),
            ))
        }
    };
    if from == to {
        return Err(AppError::InvalidInput(
            "from and to name the same session.".into(),
        ));
    }
    let a = get_session_in(data_dir, &from, now)?;
    let b = get_session_in(data_dir, &to, now)?;
    Ok(compare_details(&a, &b, chosen_by))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::app_exit::{AppExitReason, AppExitRecord};
    use tempfile::TempDir;

    const AT: &str = "2026-09-25T10:32:00.000000Z";

    fn session(id: &str, build: u32, apk: &str, avd: &str) -> DebugSession {
        DebugSession {
            schema_version: DEBUG_SESSION_SCHEMA_VERSION,
            id: id.into(),
            project_root: Some("/work/app".into()),
            package: "com.example".into(),
            device: DebugSessionDevice {
                serial: "emulator-5554".into(),
                avd_name: Some(avd.into()),
                model: Some("sdk_gphone64_arm64".into()),
            },
            build: Some(DebugSessionBuild {
                id: build,
                task: ":app:assembleRelease".into(),
                started_at: AT.into(),
                origin: Some(BuildActor::App),
                apk: DebugSessionApk {
                    module: ":app".into(),
                    variant: "release".into(),
                    sha256: apk.into(),
                    version_code: Some(build),
                },
                mappings: vec![DebugSessionMapping {
                    sha256: format!("{build:064x}"),
                    pg_map_id: Some(format!("map{build}")),
                }],
            }),
            install: Some(DebugSessionInstall {
                apk_sha256: apk.into(),
                version_code: Some(build),
                installed_at: AT.into(),
                by: BuildActor::App,
            }),
            opened_at: AT.into(),
            closed_at: None,
            close_reason: None,
            recorded_by: DebugSessionRecorder::App,
            kept: false,
            counts: DebugSessionCounts::default(),
            last_event_at: AT.into(),
            event_count: 0,
            dropped_events: 0,
            bytes: 0,
        }
    }

    fn event(seq: u32, event: DebugSessionEventData) -> DebugSessionEvent {
        DebugSessionEvent {
            seq,
            at: AT.into(),
            actor: None,
            event,
        }
    }

    fn launch(total_ms: u32, state: LaunchState, avd: &str, measured_at: &str) -> LaunchTiming {
        LaunchTiming {
            total_ms,
            wait_ms: None,
            launch_state: Some(state),
            measured_at: measured_at.into(),
            serial: "emulator-5554".into(),
            avd_name: Some(avd.into()),
            model: None,
            displayed_ms: None,
            fully_drawn_ms: None,
        }
    }

    fn launched(seq: u32, timing: LaunchTiming) -> DebugSessionEvent {
        event(
            seq,
            DebugSessionEventData::Launch(DebugSessionLaunch {
                serial: "emulator-5554".into(),
                timing: Some(timing),
                restart: false,
            }),
        )
    }

    fn crash(seq: u32, signature: &str, summary: &str) -> DebugSessionEvent {
        event(
            seq,
            DebugSessionEventData::Crash(DebugSessionCrash {
                serial: "emulator-5554".into(),
                pid: Some(4242),
                summary: summary.into(),
                signature: signature.into(),
                received_at: AT.into(),
                device_time: "09-25 10:32:01.100".into(),
                attribution: DebugSessionAttribution {
                    method: DebugSessionAttributionMethod::InstallRecord,
                    verified: true,
                    reason: None,
                },
                capture: None,
                dropped_lines: 0,
            }),
        )
    }

    fn exit(seq: u32, reason: AppExitReason) -> DebugSessionEvent {
        event(
            seq,
            DebugSessionEventData::Exit(DebugSessionExit {
                serial: "emulator-5554".into(),
                exited_at: AT.into(),
                matched_by: DebugSessionExitMatch::Pid,
                record: AppExitRecord {
                    timestamp: None,
                    timestamp_local: None,
                    pid: Some(4242),
                    process_name: Some("com.example".into()),
                    reason,
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
            }),
        )
    }

    fn detail(session: DebugSession, events: Vec<DebugSessionEvent>) -> DebugSessionDetail {
        DebugSessionDetail {
            crashes: crashes::crash_events(&events),
            session,
            events,
            events_truncated: false,
        }
    }

    #[test]
    fn lists_what_changed_between_the_builds() {
        let from = detail(session("s-a", 11, &"a".repeat(64), "Pixel_7"), vec![]);
        let to = detail(session("s-b", 12, &"b".repeat(64), "Pixel_7"), vec![]);
        let c = compare_details(&from, &to, "given");

        let fields: Vec<&str> = c.differences.iter().map(|d| d.field).collect();
        assert_eq!(
            fields,
            [
                "build_id",
                "version_code",
                "apk_sha256",
                "mapping_sha256s",
                "map_ids"
            ]
        );
        assert_eq!(c.differences[0].from.as_deref(), Some("11"));
        assert_eq!(c.differences[0].to.as_deref(), Some("12"));
        assert_eq!(c.provenance, None);
        assert!(c.not_recorded.contains(&"source commit and branch"));
        assert_eq!(c.chosen_by, "given");
    }

    #[test]
    fn compares_launch_times_only_on_the_same_device_and_launch_state() {
        let from = detail(
            session("s-a", 11, "a", "Pixel_7"),
            vec![launched(1, launch(800, LaunchState::Cold, "Pixel_7", "t1"))],
        );
        // A later display time for the same launch is taken; an older launch is not.
        let mut shown = launch(854, LaunchState::Cold, "Pixel_7", "t3");
        shown.displayed_ms = Some(900);
        let to = detail(
            session("s-b", 12, "b", "Pixel_7"),
            vec![
                launched(1, launch(400, LaunchState::Cold, "Pixel_7", "t2")),
                launched(2, launch(854, LaunchState::Cold, "Pixel_7", "t3")),
                event(3, DebugSessionEventData::LaunchTiming(shown)),
            ],
        );
        let c = compare_details(&from, &to, "given");
        assert!(c.launch.comparable);
        assert_eq!(c.launch.delta_ms, Some(54));
        assert_eq!(c.launch.to.as_ref().and_then(|l| l.displayed_ms), Some(900));

        let warm = detail(
            session("s-c", 12, "b", "Pixel_7"),
            vec![launched(1, launch(120, LaunchState::Warm, "Pixel_7", "t4"))],
        );
        let c = compare_details(&from, &warm, "given");
        assert!(!c.launch.comparable);
        assert_eq!(c.launch.delta_ms, None);
        assert!(c.launch.note.unwrap().contains("cold vs warm"));

        let other_avd = detail(
            session("s-d", 12, "b", "Pixel_8"),
            vec![launched(1, launch(700, LaunchState::Cold, "Pixel_8", "t5"))],
        );
        let c = compare_details(&from, &other_avd, "given");
        assert!(!c.launch.comparable);
        assert!(c.differences.iter().any(|d| d.field == "device"));

        let none = detail(session("s-e", 12, "b", "Pixel_7"), vec![]);
        let c = compare_details(&from, &none, "given");
        assert!(!c.launch.comparable);
        assert!(c
            .launch
            .note
            .unwrap()
            .contains("`to` has no measured launch"));
    }

    #[test]
    fn groups_crash_signatures_and_says_which_are_new() {
        let from = detail(
            session("s-a", 11, "a", "Pixel_7"),
            vec![crash(1, "old", "java.lang.IllegalStateException: stale")],
        );
        let to = detail(
            session("s-b", 12, "b", "Pixel_7"),
            vec![
                crash(1, "new", "java.lang.NullPointerException"),
                crash(2, "new", "java.lang.NullPointerException"),
                exit(3, AppExitReason::Crash),
                exit(4, AppExitReason::Crash),
                exit(5, AppExitReason::LowMemory),
            ],
        );
        let c = compare_details(&from, &to, "given");
        assert_eq!(c.crashes.to[0].signature, "new");
        assert_eq!(c.crashes.to[0].count, 2);
        assert_eq!(c.crashes.to[0].kind, "crash");
        assert_eq!(c.crashes.new_in_to, ["new"]);
        assert_eq!(c.crashes.gone_in_to, ["old"]);
        assert_eq!(c.exits.to.get("crash"), Some(&2));
        assert_eq!(c.exits.to.get("lowMemory"), Some(&1));
        assert!(c.exits.from.is_empty());
    }

    fn summary(id: &str, variant: &str, launches: u32, crashes: u32) -> DebugSessionSummary {
        let mut s = session(id, 1, "a", "Pixel_7");
        if let Some(build) = &mut s.build {
            build.apk.variant = variant.into();
        }
        s.counts.launches = launches;
        s.counts.crashes = crashes;
        DebugSessionSummary::from(&s)
    }

    #[test]
    fn the_default_pair_is_the_last_passing_session_and_the_first_crash_after_it() {
        // Newest first, as the list returns them.
        let sessions = [
            summary("s-6", "release", 1, 1),
            summary("s-5", "release", 1, 3),
            summary("s-4", "debug", 1, 2),
            summary("s-3", "release", 1, 0),
            summary("s-2", "release", 1, 0),
            summary("s-1", "release", 0, 0),
        ];
        assert_eq!(
            default_pair(&sessions),
            Some(("s-3".to_string(), "s-5".to_string()))
        );

        // The newest passing session has no crash after it: an older one does.
        let sessions = [
            summary("s-3", "release", 2, 0),
            summary("s-2", "release", 1, 1),
            summary("s-1", "release", 1, 0),
        ];
        assert_eq!(
            default_pair(&sessions),
            Some(("s-1".to_string(), "s-2".to_string()))
        );

        let sessions = [
            summary("s-2", "debug", 1, 1),
            summary("s-1", "release", 1, 0),
        ];
        assert_eq!(default_pair(&sessions), None);
    }

    #[test]
    fn compares_sessions_on_disk_and_refuses_half_a_pair() {
        let dir = TempDir::new().unwrap();
        let one = "s-20260925T103200Z-000000000001";
        let two = "s-20260925T103200Z-000000000002";
        let err = compare_sessions_in(dir.path(), Some(one), None, Utc::now()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err}");
        let err = compare_sessions_in(dir.path(), Some(one), Some(one), Utc::now()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err}");
        let err = compare_sessions_in(dir.path(), None, None, Utc::now()).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
        let err = compare_sessions_in(dir.path(), Some(one), Some(two), Utc::now()).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
    }
}
