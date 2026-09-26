//! Comparing two debug sessions (MCP `compare_debug_sessions`): what differs
//! between the build that ran cleanly and the one that crashed, including
//! the builds' provenance (source commit and build files). What sessions do
//! not record is listed as such.

use super::*;
use crate::models::build::{BuildProvenance, LaunchState};
use std::collections::BTreeMap;

/// Most crash signatures listed per session.
pub const MAX_COMPARED_SIGNATURES: usize = 20;

/// What a comparison cannot say, so a reader does not over-read the diff.
pub const NOT_RECORDED: &[&str] = &[
    "resolved dependency versions",
    "Android Gradle Plugin version",
    "the content of uncommitted changes (only whether there were any)",
];
/// Listed too when neither build recorded its provenance.
pub const PROVENANCE_NOT_RECORDED: &str =
    "source commit, branch, and build-file hashes (neither build recorded them)";

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

/// One build's provenance, as the comparison shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceSide {
    pub commit: Option<String>,
    pub branch: Option<String>,
    pub dirty: Option<bool>,
    pub changed_files: Option<u32>,
    /// Why no commit was recorded.
    pub git_unavailable: Option<String>,
    pub gradle_version: Option<String>,
    pub jdk_version: Option<String>,
    pub build_files: usize,
}

/// A build file whose content differs between the builds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildFileChange {
    /// Relative to the Gradle root.
    pub path: String,
    /// `changed`, `added` (only `to` has it), or `removed` (only `from` has it).
    pub change: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceComparison {
    /// `None` when that session's build recorded no provenance.
    pub from: Option<ProvenanceSide>,
    pub to: Option<ProvenanceSide>,
    /// Whether both builds are of the same commit; `None` unless both name one.
    pub same_commit: Option<bool>,
    /// Commit, branch, uncommitted changes, Gradle and JDK versions that differ.
    pub differences: Vec<FieldChange>,
    /// Build files whose SHA-256 differs, or that only one build has.
    pub changed_build_files: Vec<BuildFileChange>,
    /// What to keep in mind reading this.
    pub notes: Vec<String>,
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
    /// The builds' provenance; `None` when neither recorded it.
    pub provenance: Option<ProvenanceComparison>,
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
            DebugSessionRecorder::Imported => "imported",
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

fn provenance_of(session: &DebugSession) -> Option<&BuildProvenance> {
    session.build.as_ref()?.provenance.as_ref()
}

fn provenance_side(p: &BuildProvenance) -> ProvenanceSide {
    ProvenanceSide {
        commit: p.commit.clone(),
        branch: p.branch.clone(),
        dirty: p.dirty,
        changed_files: p.changed_files,
        git_unavailable: p.git_unavailable.clone(),
        gradle_version: p.gradle_version.clone(),
        jdk_version: p.jdk_version.clone(),
        build_files: p.build_files.len(),
    }
}

fn changed_build_files(from: &BuildProvenance, to: &BuildProvenance) -> Vec<BuildFileChange> {
    let hashes = |p: &BuildProvenance| -> BTreeMap<String, String> {
        p.build_files
            .iter()
            .map(|f| (f.path.clone(), f.sha256.clone()))
            .collect()
    };
    let (a, b) = (hashes(from), hashes(to));
    let mut paths: Vec<&String> = a.keys().chain(b.keys()).collect();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|path| {
            let change = match (a.get(path), b.get(path)) {
                (Some(x), Some(y)) if x == y => return None,
                (Some(_), Some(_)) => "changed",
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (None, None) => return None,
            };
            Some(BuildFileChange {
                path: path.clone(),
                change,
            })
        })
        .collect()
}

fn dirty_text(p: &BuildProvenance) -> Option<String> {
    p.dirty.map(|dirty| match (dirty, p.changed_files) {
        (false, _) => "none".to_string(),
        (true, Some(n)) => format!("{n} files"),
        (true, None) => "some".to_string(),
    })
}

/// Compare the provenance of the two sessions' builds; `None` when neither
/// recorded one.
fn compare_provenance(from: &DebugSession, to: &DebugSession) -> Option<ProvenanceComparison> {
    let (a, b) = (provenance_of(from), provenance_of(to));
    if a.is_none() && b.is_none() {
        return None;
    }
    let mut notes = Vec::new();
    for (name, side) in [("from", a), ("to", b)] {
        match side.map(|p| &p.git_unavailable) {
            None => notes.push(format!(
                "`{name}`'s build recorded no provenance (no build record, or built before \
                 Keynobi recorded it)."
            )),
            Some(Some(reason)) => notes.push(format!("`{name}` has no commit: {reason}.")),
            Some(None) => {}
        }
    }
    let (mut same_commit, mut differences, mut files) = (None, Vec::new(), Vec::new());
    if let (Some(a), Some(b)) = (a, b) {
        if let (Some(x), Some(y)) = (&a.commit, &b.commit) {
            same_commit = Some(x == y);
            if x == y && (a.dirty == Some(true) || b.dirty == Some(true)) {
                notes.push(
                    "Same commit, but built with uncommitted changes: the sources may differ."
                        .into(),
                );
            }
        }
        let fields: [(&'static str, Option<String>, Option<String>); 5] = [
            ("commit", a.commit.clone(), b.commit.clone()),
            ("branch", a.branch.clone(), b.branch.clone()),
            ("uncommitted_changes", dirty_text(a), dirty_text(b)),
            (
                "gradle_version",
                a.gradle_version.clone(),
                b.gradle_version.clone(),
            ),
            ("jdk_version", a.jdk_version.clone(), b.jdk_version.clone()),
        ];
        differences = fields
            .into_iter()
            .filter(|(_, x, y)| x != y)
            .map(|(field, from, to)| FieldChange { field, from, to })
            .collect();
        files = changed_build_files(a, b);
    }
    Some(ProvenanceComparison {
        from: a.map(provenance_side),
        to: b.map(provenance_side),
        same_commit,
        differences,
        changed_build_files: files,
        notes,
    })
}

/// Compare two sessions as read with `get_session`.
pub fn compare_details(
    from: &DebugSessionDetail,
    to: &DebugSessionDetail,
    chosen_by: &'static str,
) -> SessionComparison {
    let a = compared(&from.session);
    let b = compared(&to.session);
    let provenance = compare_provenance(&from.session, &to.session);
    let mut not_recorded = Vec::new();
    if provenance.is_none() {
        not_recorded.push(PROVENANCE_NOT_RECORDED);
    }
    not_recorded.extend_from_slice(NOT_RECORDED);
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
        provenance,
        not_recorded,
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
                provenance: None,
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
            imported: None,
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
        assert!(c.not_recorded.contains(&PROVENANCE_NOT_RECORDED));
        assert_eq!(c.chosen_by, "given");
    }

    fn provenance(commit: &str, dirty: bool, files: &[(&str, &str)]) -> BuildProvenance {
        BuildProvenance {
            commit: Some(commit.repeat(40)),
            branch: Some("main".into()),
            dirty: Some(dirty),
            changed_files: Some(u32::from(dirty) * 2),
            git_unavailable: None,
            build_files: files
                .iter()
                .map(|(path, sha)| crate::models::build::BuildFileHash {
                    path: path.to_string(),
                    sha256: sha.repeat(64),
                })
                .collect(),
            gradle_version: Some("8.7".into()),
            jdk_version: Some("17.0.9".into()),
        }
    }

    fn with_provenance(mut session: DebugSession, p: Option<BuildProvenance>) -> DebugSession {
        if let Some(build) = &mut session.build {
            build.provenance = p;
        }
        session
    }

    #[test]
    fn compares_the_builds_commits_and_build_files() {
        let from = detail(
            with_provenance(
                session("s-a", 11, "a", "Pixel_7"),
                Some(provenance(
                    "a",
                    false,
                    &[
                        ("app/build.gradle.kts", "1"),
                        ("gradle/libs.versions.toml", "2"),
                        ("gradle.properties", "3"),
                    ],
                )),
            ),
            vec![],
        );
        let mut to_provenance = provenance(
            "b",
            true,
            &[
                ("app/build.gradle.kts", "1"),
                ("gradle/libs.versions.toml", "9"),
                ("settings.gradle.kts", "4"),
            ],
        );
        to_provenance.gradle_version = Some("8.9".into());
        let to = detail(
            with_provenance(session("s-b", 12, "b", "Pixel_7"), Some(to_provenance)),
            vec![],
        );
        let c = compare_details(&from, &to, "given");
        let p = c.provenance.expect("provenance compared");
        assert_eq!(p.same_commit, Some(false));
        let fields: Vec<&str> = p.differences.iter().map(|d| d.field).collect();
        assert_eq!(fields, ["commit", "uncommitted_changes", "gradle_version"]);
        assert_eq!(p.differences[1].from.as_deref(), Some("none"));
        assert_eq!(p.differences[1].to.as_deref(), Some("2 files"));
        let files: Vec<(&str, &str)> = p
            .changed_build_files
            .iter()
            .map(|f| (f.path.as_str(), f.change))
            .collect();
        assert_eq!(
            files,
            [
                ("gradle.properties", "removed"),
                ("gradle/libs.versions.toml", "changed"),
                ("settings.gradle.kts", "added"),
            ]
        );
        assert_eq!(p.to.as_ref().and_then(|s| s.dirty), Some(true));
        assert!(p.notes.is_empty(), "{:?}", p.notes);
        assert!(!c.not_recorded.contains(&PROVENANCE_NOT_RECORDED));
        assert!(!c.not_recorded.iter().any(|n| n.contains("source commit")));
    }

    #[test]
    fn the_same_commit_with_uncommitted_changes_is_flagged() {
        let files = [("gradle/libs.versions.toml", "2")];
        let from = detail(
            with_provenance(
                session("s-a", 11, "a", "Pixel_7"),
                Some(provenance("a", false, &files)),
            ),
            vec![],
        );
        let to = detail(
            with_provenance(
                session("s-b", 12, "b", "Pixel_7"),
                Some(provenance("a", true, &files)),
            ),
            vec![],
        );
        let p = compare_details(&from, &to, "given").provenance.unwrap();
        assert_eq!(p.same_commit, Some(true));
        assert!(p.changed_build_files.is_empty());
        assert!(p.notes[0].contains("uncommitted changes"), "{:?}", p.notes);
    }

    #[test]
    fn a_build_without_provenance_or_git_is_noted() {
        let mut no_git = provenance("a", false, &[]);
        no_git.commit = None;
        no_git.dirty = None;
        no_git.git_unavailable = Some("not a git repository".into());
        let from = detail(session("s-a", 11, "a", "Pixel_7"), vec![]);
        let to = detail(
            with_provenance(session("s-b", 12, "b", "Pixel_7"), Some(no_git)),
            vec![],
        );
        let p = compare_details(&from, &to, "given").provenance.unwrap();
        assert_eq!(p.from, None);
        assert_eq!(p.same_commit, None);
        assert!(p.differences.is_empty());
        assert_eq!(p.notes.len(), 2, "{:?}", p.notes);
        assert!(p.notes[0].contains("`from`'s build recorded no provenance"));
        assert!(p.notes[1].contains("not a git repository"));
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
