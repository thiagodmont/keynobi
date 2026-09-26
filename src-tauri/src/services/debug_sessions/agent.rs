//! Debug sessions as MCP agents read them (`list_debug_sessions`,
//! `get_debug_session`): filtered and bounded lists, one line per session, and
//! a session with a page of its timeline, its crashes, and a crash's log lines.

use super::*;
use crate::models::logcat::LogcatLevel;
use serde_json::{json, Value};

/// Sessions `list_debug_sessions` returns by default, and at most.
pub const DEFAULT_AGENT_SESSIONS: usize = 10;
pub const MAX_AGENT_SESSIONS: usize = MAX_SESSIONS;
/// Timeline events `get_debug_session` returns by default, and at most.
pub const DEFAULT_AGENT_EVENTS: usize = 100;
pub const MAX_AGENT_EVENTS: usize = MAX_EVENTS_RETURNED;
/// Crashes and ANRs `get_debug_session` returns, the newest.
pub const MAX_AGENT_CRASHES: usize = 20;
/// Log lines of a capture `get_debug_session` returns by default.
pub const DEFAULT_AGENT_LOG_LINES: usize = 100;

/// Which sessions to list, by state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    /// Superseded, ended, or idle.
    Closed,
    ClosedBecause(DebugSessionCloseReason),
}

impl StateFilter {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            "superseded" => Ok(Self::ClosedBecause(DebugSessionCloseReason::Superseded)),
            "ended" => Ok(Self::ClosedBecause(DebugSessionCloseReason::Ended)),
            "idle" => Ok(Self::ClosedBecause(DebugSessionCloseReason::Idle)),
            other => Err(format!(
                "Unknown state '{other}': use open, closed, superseded, ended, or idle"
            )),
        }
    }

    fn matches(self, s: &DebugSessionSummary) -> bool {
        match self {
            Self::Open => s.closed_at.is_none(),
            Self::Closed => s.closed_at.is_some(),
            Self::ClosedBecause(reason) => s.closed_at.is_some() && s.close_reason == Some(reason),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub package: Option<String>,
    /// A serial or an AVD name.
    pub device: Option<String>,
    pub state: Option<StateFilter>,
    /// Only sessions with a crash or an ANR.
    pub only_crashing: bool,
}

impl SessionFilter {
    fn matches(&self, s: &DebugSessionSummary) -> bool {
        self.package.as_deref().is_none_or(|p| p == s.package)
            && self
                .device
                .as_deref()
                .is_none_or(|d| d == s.device.serial || s.device.avd_name.as_deref() == Some(d))
            && self.state.is_none_or(|state| state.matches(s))
            && (!self.only_crashing || s.counts.crashes + s.counts.anrs > 0)
    }
}

/// The sessions `filter` matches, newest first, at most `limit`, and how
/// many matched.
pub fn list_for_agent(filter: &SessionFilter, limit: usize) -> (Vec<DebugSessionSummary>, usize) {
    filter_sessions(list_sessions(), filter, limit)
}

pub(super) fn filter_sessions(
    sessions: Vec<DebugSessionSummary>,
    filter: &SessionFilter,
    limit: usize,
) -> (Vec<DebugSessionSummary>, usize) {
    let matching: Vec<DebugSessionSummary> =
        sessions.into_iter().filter(|s| filter.matches(s)).collect();
    let total = matching.len();
    (
        matching
            .into_iter()
            .take(limit.clamp(1, MAX_AGENT_SESSIONS))
            .collect(),
        total,
    )
}

fn state_of(s: &DebugSessionSummary) -> &'static str {
    match (s.closed_at.is_some(), s.close_reason) {
        (false, _) => "open",
        (true, Some(DebugSessionCloseReason::Superseded)) => "superseded",
        (true, Some(DebugSessionCloseReason::Idle)) => "idle",
        (true, _) => "ended",
    }
}

fn device_label(device: &DebugSessionDevice) -> String {
    match &device.avd_name {
        Some(avd) => format!("{avd} ({})", device.serial),
        None => device.serial.clone(),
    }
}

/// One line describing a session:
/// `s-… | open | com.example on Pixel_7 (emulator-5554) | build #12 :app debug | apk 1a2b3c4d5e6f | …`.
pub fn agent_line(s: &DebugSessionSummary) -> String {
    let build = match (s.build_id, &s.module, &s.variant) {
        (Some(id), Some(module), Some(variant)) => format!("build #{id} {module} {variant}"),
        (Some(id), _, _) => format!("build #{id}"),
        (None, _, _) if s.apk_sha256.is_none() => "unattributed (no Keynobi install)".into(),
        (None, _, _) => "no build record".into(),
    };
    let mut parts = vec![
        s.id.clone(),
        state_of(s).to_string(),
        format!("{} on {}", s.package, device_label(&s.device)),
        build,
    ];
    if let Some(apk) = &s.apk_sha256 {
        parts.push(format!("apk {}", apk.chars().take(12).collect::<String>()));
    }
    parts.push(match &s.closed_at {
        Some(closed) => format!("opened {} closed {closed}", s.opened_at),
        None => format!("opened {} last event {}", s.opened_at, s.last_event_at),
    });
    let c = &s.counts;
    parts.push(format!(
        "{} crashes, {} ANRs, {} exits, {} launches",
        c.crashes, c.anrs, c.exits, c.launches
    ));
    if s.kept {
        parts.push("kept".into());
    }
    if s.recorded_by == DebugSessionRecorder::Standalone {
        parts.push("recorded by a standalone MCP server".into());
    }
    parts.join(" | ")
}

fn actor_label(actor: &Option<BuildActor>) -> Option<String> {
    actor.as_ref().map(|a| match a {
        BuildActor::App => "app".into(),
        BuildActor::AppQuit => "app quitting".into(),
        BuildActor::Agent(agent) => match &agent.client_name {
            Some(name) => format!("agent {name}"),
            None => "agent".into(),
        },
    })
}

fn launch_text(timing: &LaunchTiming) -> String {
    let mut text = format!("{} ms", timing.total_ms);
    if let Some(state) = timing.launch_state {
        if let Some(name) = serde_json::to_value(state)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
        {
            text.push_str(&format!(" ({name})"));
        }
    }
    if let Some(ms) = timing.displayed_ms {
        text.push_str(&format!(", displayed {ms} ms"));
    }
    if let Some(ms) = timing.fully_drawn_ms {
        text.push_str(&format!(", fully drawn {ms} ms"));
    }
    text
}

/// A one-line description of an event.
pub fn event_summary(event: &DebugSessionEventData) -> String {
    use DebugSessionEventData as E;
    match event {
        E::Build(b) => format!(
            "build #{} {} {} ({})",
            b.id, b.apk.module, b.apk.variant, b.task
        ),
        E::Install(i) => format!(
            "installed apk {}",
            i.apk_sha256.chars().take(12).collect::<String>()
        ),
        E::Launch(l) => {
            let verb = if l.restart { "restarted" } else { "launched" };
            match &l.timing {
                Some(t) => format!("{verb} in {}", launch_text(t)),
                None => format!("{verb}, no launch time reported"),
            }
        }
        E::LaunchTiming(t) => format!(
            "display times of the launch at {}: {}",
            t.measured_at,
            launch_text(t)
        ),
        E::LogcatReconnect(c) => format!("logcat reconnecting to {}", c.serial),
        E::LogcatStopped(c) => match &c.reason {
            Some(reason) => format!("logcat stopped: {reason}"),
            None => "logcat stopped".into(),
        },
        E::LogcatCleared(_) => "logcat cleared".into(),
        E::DeviceOffline(d) => format!("{} went offline", d.serial),
        E::DeviceOnline(d) => format!("{} came back online", d.serial),
        E::Bookmark(b) => format!("bookmark: {}", b.note),
        E::Crash(c) => format!("crash: {}", c.summary),
        E::Anr(c) => format!("ANR: {}", c.summary),
        E::Exit(e) => {
            let reason = serde_json::to_value(e.record.reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".into());
            let pid = e
                .record
                .pid
                .map(|p| format!(" pid {p}"))
                .unwrap_or_default();
            format!("process exit: {reason}{pid}")
        }
        E::AgentAction(a) => {
            let status = if a.ok { "ok" } else { "failed" };
            format!("agent ran {} ({status}, {} ms)", a.tool, a.duration_ms)
        }
    }
}

fn kind_of(event: &DebugSessionEvent) -> String {
    serde_json::to_value(event)
        .ok()
        .and_then(|v| v["kind"].as_str().map(str::to_string))
        .unwrap_or_default()
}

fn level_char(level: &LogcatLevel) -> char {
    match level {
        LogcatLevel::Verbose => 'V',
        LogcatLevel::Debug => 'D',
        LogcatLevel::Info => 'I',
        LogcatLevel::Warn => 'W',
        LogcatLevel::Error => 'E',
        LogcatLevel::Fatal => 'F',
        LogcatLevel::Unknown => '?',
    }
}

/// The newest `limit` events before `before_seq` (all when `None`), oldest
/// first, and the cursor for the page before them.
pub(super) fn timeline_page(
    events: &[DebugSessionEvent],
    before_seq: Option<u32>,
    limit: usize,
) -> (Vec<DebugSessionEvent>, Option<u32>) {
    let older: Vec<&DebugSessionEvent> = events
        .iter()
        .filter(|e| before_seq.is_none_or(|b| e.seq < b))
        .collect();
    let skip = older.len().saturating_sub(limit);
    let page: Vec<DebugSessionEvent> = older[skip..].iter().map(|e| (*e).clone()).collect();
    let next = (skip > 0).then(|| page.first().map(|e| e.seq)).flatten();
    (page, next)
}

/// What `get_debug_session` asks for.
#[derive(Debug, Clone)]
pub struct AgentSessionRequest {
    pub id: String,
    pub before_seq: Option<u32>,
    pub max_events: usize,
    /// The crash event whose kept log lines to include.
    pub capture_seq: Option<u32>,
    pub log_lines: usize,
}

/// A session for an agent: summary, a page of the timeline, the newest
/// crashes with how they were attributed, and a crash's log lines on request.
pub fn session_for_agent(request: &AgentSessionRequest) -> Result<Value, AppError> {
    session_for_agent_in(&data_dir(), request, Utc::now())
}

pub(super) fn session_for_agent_in(
    data_dir: &Path,
    request: &AgentSessionRequest,
    now: DateTime<Utc>,
) -> Result<Value, AppError> {
    checked_id(&request.id)?;
    let session = read_session_in(data_dir, &request.id, now)?;
    let events = read_events(data_dir, &request.id)?;
    let (page, next_before_seq) = timeline_page(
        &events,
        request.before_seq,
        request.max_events.min(MAX_AGENT_EVENTS),
    );
    let all_crashes = crashes::crash_events(&events);
    let crashes_truncated = all_crashes.len() > MAX_AGENT_CRASHES;
    let crashes: Vec<Value> = all_crashes
        .iter()
        .skip(all_crashes.len().saturating_sub(MAX_AGENT_CRASHES))
        .filter_map(|event| {
            let (kind, c) = match &event.event {
                DebugSessionEventData::Crash(c) => ("crash", c),
                DebugSessionEventData::Anr(c) => ("anr", c),
                _ => return None,
            };
            Some(json!({
                "seq": event.seq,
                "kind": kind,
                "summary": c.summary,
                "pid": c.pid,
                "signature": c.signature,
                "received_at": c.received_at,
                "device_time": c.device_time,
                "attribution": {
                    "method": match c.attribution.method {
                        DebugSessionAttributionMethod::InstallRecord => "install_record",
                        DebugSessionAttributionMethod::Unattributed => "unattributed",
                    },
                    "verified": c.attribution.verified,
                    "reason": c.attribution.reason,
                },
                "log_lines_kept": c.capture.as_ref().map(|k| k.entries),
                "dropped_lines": c.dropped_lines,
            }))
        })
        .collect();

    let capture = match request.capture_seq {
        Some(seq) => {
            let lines = request.log_lines.clamp(1, MAX_CAPTURE_ENTRIES);
            let capture = crashes::get_capture_in(data_dir, &request.id, seq, Some(lines as u32))?;
            let text: Vec<String> = capture
                .entries
                .iter()
                .map(|e| {
                    format!(
                        "{} {}/{}({}): {}",
                        e.timestamp,
                        level_char(&e.level),
                        e.tag,
                        e.pid,
                        e.message
                    )
                })
                .collect();
            json!({ "seq": seq, "lines": text, "truncated": capture.truncated })
        }
        None => Value::Null,
    };

    let build = session.build.as_ref().map(|b| {
        json!({
            "id": b.id,
            "task": b.task,
            "started_at": b.started_at,
            "module": b.apk.module,
            "variant": b.apk.variant,
            "version_code": b.apk.version_code,
            "apk_sha256": b.apk.sha256,
            "map_ids": b.mappings.iter().filter_map(|m| m.pg_map_id.clone()).collect::<Vec<_>>(),
        })
    });
    let install = session.install.as_ref().map(|i| {
        json!({
            "installed_at": i.installed_at,
            "apk_sha256": i.apk_sha256,
            "version_code": i.version_code,
            "by": actor_label(&Some(i.by.clone())),
        })
    });
    let summary = DebugSessionSummary::from(&session);
    let timeline: Vec<Value> = page
        .iter()
        .map(|e| {
            json!({
                "seq": e.seq,
                "at": e.at,
                "kind": kind_of(e),
                "by": actor_label(&e.actor),
                "summary": event_summary(&e.event),
            })
        })
        .collect();
    let c = &session.counts;
    Ok(json!({
        "session": {
            "id": session.id,
            "state": state_of(&summary),
            "package": session.package,
            "project_root": session.project_root,
            "device": {
                "serial": session.device.serial,
                "avd_name": session.device.avd_name,
                "model": session.device.model,
            },
            "build": build,
            "install": install,
            "opened_at": session.opened_at,
            "closed_at": session.closed_at,
            "last_event_at": session.last_event_at,
            "recorded_by": match session.recorded_by {
                DebugSessionRecorder::App => "app",
                DebugSessionRecorder::Standalone => "standalone",
            },
            "kept": session.kept,
            "counts": {
                "launches": c.launches,
                "crashes": c.crashes,
                "anrs": c.anrs,
                "exits": c.exits,
                "bookmarks": c.bookmarks,
                "agent_actions": c.agent_actions,
                "log_captures": c.captures,
            },
            "events": session.event_count,
            "dropped_events": session.dropped_events,
        },
        "timeline": timeline,
        "next_before_seq": next_before_seq,
        "crashes": crashes,
        "crashes_truncated": crashes_truncated,
        "capture": capture,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const RETAIN: Retention = Retention {
        days: 14,
        max_folder_mb: 200,
    };

    fn target(serial: &str, avd: Option<&str>) -> InstallTarget {
        InstallTarget {
            serial: serial.into(),
            avd_name: avd.map(str::to_string),
            model: None,
        }
    }

    fn open(dir: &Path, target: &InstallTarget, package: &str) -> DebugSession {
        let entry = InstalledBuild {
            serial: target.serial.clone(),
            avd_name: target.avd_name.clone(),
            model: None,
            package: package.into(),
            apk_sha256: "a".repeat(64),
            build_id: None,
            version_code: Some(7),
            mappings: vec![],
            installed_at: "2026-09-25T10:32:00+00:00".into(),
        };
        open_in(dir, target, &entry, BuildActor::App, RETAIN, Utc::now()).unwrap()
    }

    fn event(seq: u32) -> DebugSessionEvent {
        DebugSessionEvent {
            seq,
            at: "2026-09-25T10:32:00+00:00".into(),
            actor: None,
            event: DebugSessionEventData::DeviceOnline(DebugSessionDeviceChange {
                serial: "R5CT".into(),
            }),
        }
    }

    fn seqs(events: &[DebugSessionEvent]) -> Vec<u32> {
        events.iter().map(|e| e.seq).collect()
    }

    fn timeline_seqs(session: &Value) -> Vec<u64> {
        session["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["seq"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn the_timeline_pages_back_from_the_newest_events() {
        let events: Vec<DebugSessionEvent> = (1..=5).map(event).collect();

        let (page, next) = timeline_page(&events, None, 2);
        assert_eq!((seqs(&page), next), (vec![4, 5], Some(4)));
        let (page, next) = timeline_page(&events, next, 2);
        assert_eq!((seqs(&page), next), (vec![2, 3], Some(2)));
        let (page, next) = timeline_page(&events, next, 2);
        assert_eq!((seqs(&page), next), (vec![1], None));
        let (page, next) = timeline_page(&events, None, 5);
        assert_eq!((seqs(&page), next), (vec![1, 2, 3, 4, 5], None));
    }

    #[test]
    fn sessions_filter_by_package_device_state_and_crashes() {
        let dir = TempDir::new().unwrap();
        let phone = open(dir.path(), &target("R5CT", None), "com.a");
        let emulator = open(
            dir.path(),
            &target("emulator-5554", Some("Pixel_7")),
            "com.b",
        );
        end_session_in(dir.path(), &phone.id, RETAIN, Utc::now()).unwrap();
        let mut sessions = list_sessions_in(dir.path(), Utc::now());
        sessions[0].counts.anrs = 1;
        let ids = |filter: SessionFilter| -> (Vec<String>, usize) {
            let (listed, total) = filter_sessions(sessions.clone(), &filter, 10);
            (listed.into_iter().map(|s| s.id).collect(), total)
        };
        let only = |id: &str| (vec![id.to_string()], 1);

        assert_eq!(ids(SessionFilter::default()).1, 2);
        let by_package = SessionFilter {
            package: Some("com.a".into()),
            ..Default::default()
        };
        assert_eq!(ids(by_package), only(&phone.id));
        let by_avd = SessionFilter {
            device: Some("Pixel_7".into()),
            ..Default::default()
        };
        assert_eq!(ids(by_avd), only(&emulator.id));
        let by_serial = SessionFilter {
            device: Some("R5CT".into()),
            ..Default::default()
        };
        assert_eq!(ids(by_serial), only(&phone.id));
        let open_ones = SessionFilter {
            state: Some(StateFilter::Open),
            ..Default::default()
        };
        assert_eq!(ids(open_ones), only(&emulator.id));
        let ended = SessionFilter {
            state: Some(StateFilter::parse("ended").unwrap()),
            ..Default::default()
        };
        assert_eq!(ids(ended), only(&phone.id));
        let superseded = SessionFilter {
            state: Some(StateFilter::parse("superseded").unwrap()),
            ..Default::default()
        };
        assert_eq!(ids(superseded).1, 0);
        let crashing = SessionFilter {
            only_crashing: true,
            ..Default::default()
        };
        assert_eq!(ids(crashing), only(&emulator.id));
        assert!(StateFilter::parse("running").is_err());

        let (listed, total) = filter_sessions(sessions.clone(), &SessionFilter::default(), 1);
        assert_eq!((listed.len(), total), (1, 2));
        assert_eq!(listed[0].id, emulator.id);
    }

    #[test]
    fn a_session_line_names_its_state_device_and_build() {
        let dir = TempDir::new().unwrap();
        let session = open(
            dir.path(),
            &target("emulator-5554", Some("Pixel_7")),
            "com.b",
        );
        let mut listed = list_sessions_in(dir.path(), Utc::now()).remove(0);

        let line = agent_line(&listed);
        let start = format!(
            "{} | open | com.b on Pixel_7 (emulator-5554) | no build record | apk aaaaaaaaaaaa | opened ",
            session.id
        );
        assert!(line.starts_with(&start), "{line}");
        assert!(
            line.contains("0 crashes, 0 ANRs, 0 exits, 0 launches"),
            "{line}"
        );

        listed.apk_sha256 = None;
        listed.kept = true;
        let line = agent_line(&listed);
        assert!(
            line.contains("| unattributed (no Keynobi install) |"),
            "{line}"
        );
        assert!(line.ends_with("| kept"), "{line}");
    }

    #[test]
    fn a_session_for_an_agent_pages_its_timeline_with_a_cursor() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path(), &target("R5CT", None), "com.a");
        for note in ["one", "two", "three"] {
            add_bookmark_in(dir.path(), Some(&session.id), None, note, None, Utc::now()).unwrap();
        }
        let request = |before_seq: Option<u32>| AgentSessionRequest {
            id: session.id.clone(),
            before_seq,
            max_events: 2,
            capture_seq: None,
            log_lines: DEFAULT_AGENT_LOG_LINES,
        };

        let newest = session_for_agent_in(dir.path(), &request(None), Utc::now()).unwrap();
        assert_eq!(newest["session"]["state"], "open");
        assert_eq!(newest["session"]["counts"]["bookmarks"], 3);
        assert_eq!(newest["session"]["events"], 4);
        assert_eq!(timeline_seqs(&newest), [3, 4]);
        assert_eq!(newest["timeline"][0]["kind"], "bookmark");
        assert_eq!(newest["timeline"][1]["summary"], "bookmark: three");
        assert_eq!(newest["next_before_seq"], 3);
        assert_eq!(newest["crashes"], json!([]));
        assert_eq!(newest["capture"], Value::Null);

        let older = session_for_agent_in(dir.path(), &request(Some(3)), Utc::now()).unwrap();
        assert_eq!(timeline_seqs(&older), [1, 2]);
        assert_eq!(older["timeline"][0]["kind"], "install");
        assert_eq!(older["timeline"][0]["by"], "app");
        assert_eq!(older["next_before_seq"], Value::Null);
    }

    #[test]
    fn an_unknown_or_malformed_session_id_is_an_error() {
        let dir = TempDir::new().unwrap();
        open(dir.path(), &target("R5CT", None), "com.a");
        let request = |id: &str| AgentSessionRequest {
            id: id.into(),
            before_seq: None,
            max_events: DEFAULT_AGENT_EVENTS,
            capture_seq: None,
            log_lines: DEFAULT_AGENT_LOG_LINES,
        };

        assert!(matches!(
            session_for_agent_in(dir.path(), &request("../x"), Utc::now()),
            Err(AppError::InvalidInput(_))
        ));
        assert!(matches!(
            session_for_agent_in(
                dir.path(),
                &request("s-20260925T103200Z-000000000000"),
                Utc::now()
            ),
            Err(AppError::NotFound(_))
        ));
    }
}
