//! Debug sessions: one install epoch of one build on one device, per package.
//!
//! A session opens when Keynobi records a successful install
//! (`installed_builds::install_and_record`, from Run App and MCP
//! `install_apk`) and closes when a later install on the same device and
//! package supersedes it, when the user ends it, or after
//! [`SESSION_IDLE_SECS`] without an event (computed when sessions are read,
//! not by a timer). Its timeline records launches, logcat stream changes,
//! device connection changes, bookmarks, and crashes, ANRs, and process exits
//! (see [`crashes`]).
//!
//! Storage, under `<data dir>/sessions/`:
//! - `index.json`: a summary of every session, the only file the list reads.
//! - `<id>/session.json`: the manifest ([`DebugSession`]).
//! - `<id>/events.jsonl`: the timeline, appended.
//! - `<id>/captures/crash-<seq>.jsonl`: the log lines kept with crash event `seq`.
//!
//! The app and standalone MCP processes write the same files, so every
//! read-modify-write and every append runs under the data lock, re-reading
//! inside it and writing with `unique_tmp_path` and rename. Hooks on the
//! logcat stream and the device poll never do file I/O themselves: they queue
//! the event on a bounded channel ([`MAX_PENDING_SESSION_EVENTS`]) that one
//! writer thread drains, and count what does not fit.

use crate::models::build::{BuildActor, BuildRecord, InstalledBuild, LaunchTiming};
use crate::models::debug_session::*;
use crate::models::device::{Device, DeviceConnectionState, DeviceKind};
use crate::models::error::AppError;
use crate::services::adb_manager::{AmStartTiming, DeviceState};
use crate::services::app_inspector::RestartResult;
use crate::services::build_runner;
use crate::services::installed_builds::{self, InstallTarget};
use crate::services::settings_manager::{self, data_dir, unique_tmp_path, with_data_lock_in};
use crate::utils::validation::validate_debug_session_id;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{LazyLock, Mutex as StdMutex};
use std::time::{Duration, SystemTime};

mod crashes;
pub use crashes::{
    dropped_crashes, get_capture, refresh_exit_reasons, CrashSeen, CrashSource, CRASH_SETTLE,
    EXIT_READ_DELAY, MAX_CAPTURES_PER_SESSION, MAX_CAPTURE_BYTES, MAX_CAPTURE_ENTRIES,
    MAX_CRASHES_RETURNED, MAX_PENDING_CRASHES,
};

// ── Caps ──────────────────────────────────────────────────────────────────────

/// Most sessions kept; the oldest unkept are removed first.
pub const MAX_SESSIONS: usize = 50;
/// Most events in one session's timeline; later ones are counted as dropped.
pub const MAX_EVENTS_PER_SESSION: u32 = 2_000;
/// Most bookmarks in one session.
pub const MAX_BOOKMARKS_PER_SESSION: u32 = 100;
/// Largest event log of one session.
pub const MAX_SESSION_BYTES: u64 = 16 * 1024 * 1024;
/// Most sessions marked Keep. Each pins its R8 mappings.
pub const MAX_KEPT_SESSIONS: usize = 5;
/// Longest bookmark note, in characters.
pub const MAX_BOOKMARK_NOTE_CHARS: usize = 500;
/// Longest free text in any other event (a logcat stop reason).
pub const MAX_EVENT_TEXT_CHARS: usize = 500;
/// Most events `get_debug_session` returns (the newest).
pub const MAX_EVENTS_RETURNED: usize = 500;
/// Events waiting for the writer thread; more are dropped and counted.
pub const MAX_PENDING_SESSION_EVENTS: usize = 256;
/// Emulator serials whose AVD name is remembered for logcat events.
pub const MAX_KNOWN_EMULATORS: usize = 64;
/// A session with no event for this long is closed as idle.
pub const SESSION_IDLE_SECS: i64 = 24 * 60 * 60;
/// Session folders the index does not name, and temporary files, older than
/// this are left by a process that died mid-write and are removed.
const ORPHAN_AGE: Duration = Duration::from_secs(60 * 60);

const SESSIONS_DIR: &str = "sessions";
const INDEX_FILE: &str = "index.json";
const SESSION_FILE: &str = "session.json";
const EVENTS_FILE: &str = "events.jsonl";

// ── Storage ───────────────────────────────────────────────────────────────────

pub fn sessions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(SESSIONS_DIR)
}

/// Callers validated `id` with `validate_debug_session_id`.
fn session_dir(data_dir: &Path, id: &str) -> PathBuf {
    sessions_dir(data_dir).join(id)
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexFile {
    schema_version: u32,
    sessions: Vec<DebugSessionSummary>,
}

/// The session summaries, oldest opened first. A missing or unreadable index
/// is rebuilt from the manifests. Reads only.
pub fn load_index_from(data_dir: &Path) -> Vec<DebugSessionSummary> {
    let path = sessions_dir(data_dir).join(INDEX_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<IndexFile>(&text) {
            Ok(index) => return index.sessions,
            Err(e) => tracing::warn!("Rebuilding {}: {e}", path.display()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("Rebuilding {}: {e}", path.display()),
    }
    rebuild_index(data_dir)
}

/// Summaries of every readable manifest, oldest opened first.
fn rebuild_index(data_dir: &Path) -> Vec<DebugSessionSummary> {
    let Ok(entries) = std::fs::read_dir(sessions_dir(data_dir)) else {
        return Vec::new();
    };
    let mut sessions: Vec<DebugSessionSummary> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|name| validate_debug_session_id(name).is_ok())
        .take(MAX_SESSIONS * 2)
        .filter_map(|id| read_manifest(data_dir, &id).ok())
        .map(|session| DebugSessionSummary::from(&session))
        .collect();
    sessions.sort_by(|a, b| a.opened_at.cmp(&b.opened_at));
    sessions
}

fn read_manifest(data_dir: &Path, id: &str) -> Result<DebugSession, String> {
    let path = session_dir(data_dir, id).join(SESSION_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("Cannot read {}: {e}", path.display()))
}

/// Write `value` to `path` through a unique temporary file and a rename.
/// Callers hold the data lock.
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| format!("Cannot serialize: {e}"))?;
    let tmp = unique_tmp_path(path);
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("Cannot write {}: {e}", path.display())
        })
}

fn save_manifest(data_dir: &Path, session: &DebugSession) -> Result<(), String> {
    write_json_atomic(
        &session_dir(data_dir, &session.id).join(SESSION_FILE),
        session,
    )
}

fn save_index(data_dir: &Path, sessions: &[DebugSessionSummary]) -> Result<(), String> {
    let dir = sessions_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    write_json_atomic(
        &dir.join(INDEX_FILE),
        &IndexFile {
            schema_version: DEBUG_SESSION_SCHEMA_VERSION,
            sessions: sessions.to_vec(),
        },
    )
}

/// A new, empty session folder named by a fresh id.
fn create_session_dir(data_dir: &Path, now: DateTime<Utc>) -> Result<String, String> {
    let root = sessions_dir(data_dir);
    std::fs::create_dir_all(&root).map_err(|e| format!("Cannot create {}: {e}", root.display()))?;
    for _ in 0..8 {
        let id = new_session_id(now);
        match std::fs::create_dir(root.join(&id)) {
            Ok(()) => return Ok(id),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Cannot create a session folder: {e}")),
        }
    }
    Err("Cannot allocate a session id".into())
}

/// `s-<UTC time to the second>-<48 random bits>`: unique across processes
/// without coordination (the folder is also created exclusively).
pub fn new_session_id(now: DateTime<Utc>) -> String {
    format!("s-{}-{}", now.format("%Y%m%dT%H%M%SZ"), random_suffix())
}

fn random_suffix() -> String {
    let mut bytes = [0u8; 6];
    let from_os = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !from_os {
        use sha2::{Digest, Sha256};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let seed = format!(
            "{}:{nanos}:{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        bytes.copy_from_slice(&Sha256::digest(seed.as_bytes())[..6]);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ── Open, idle, and closed ────────────────────────────────────────────────────

/// RFC 3339 in UTC with a fixed width, so stored times sort as text.
fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// When a session whose last event was at `last_event_at` closed for being
/// idle, if it has.
fn idle_closed_at(last_event_at: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let last = DateTime::parse_from_rfc3339(last_event_at)
        .ok()?
        .with_timezone(&Utc);
    let closes = last + chrono::Duration::seconds(SESSION_IDLE_SECS);
    (closes <= now).then_some(closes)
}

fn is_open(summary: &DebugSessionSummary, now: DateTime<Utc>) -> bool {
    summary.closed_at.is_none() && idle_closed_at(&summary.last_event_at, now).is_none()
}

/// `summary` as a reader sees it: an idle session is closed.
fn with_idle_view(mut summary: DebugSessionSummary, now: DateTime<Utc>) -> DebugSessionSummary {
    if summary.closed_at.is_none() {
        if let Some(at) = idle_closed_at(&summary.last_event_at, now) {
            summary.closed_at = Some(stamp(at));
            summary.close_reason = Some(DebugSessionCloseReason::Idle);
        }
    }
    summary
}

/// Close `index[i]`, in its manifest too. Callers hold the data lock.
fn close_locked(
    data_dir: &Path,
    index: &mut [DebugSessionSummary],
    i: usize,
    reason: DebugSessionCloseReason,
    closed_at: String,
) {
    match read_manifest(data_dir, &index[i].id) {
        Ok(mut session) => {
            session.closed_at = Some(closed_at);
            session.close_reason = Some(reason);
            if let Err(e) = save_manifest(data_dir, &session) {
                tracing::warn!("Debug session {} not closed on disk: {e}", session.id);
            }
            index[i] = DebugSessionSummary::from(&session);
        }
        Err(e) => {
            tracing::warn!("{e}");
            index[i].closed_at = Some(closed_at);
            index[i].close_reason = Some(reason);
        }
    }
}

/// The index, with idle sessions closed on disk. Callers hold the data lock
/// and save the index.
fn load_index_locked(data_dir: &Path, now: DateTime<Utc>) -> Vec<DebugSessionSummary> {
    load_index_changed(data_dir, now).0
}

/// [`load_index_locked`], and whether it closed an idle session.
fn load_index_changed(data_dir: &Path, now: DateTime<Utc>) -> (Vec<DebugSessionSummary>, bool) {
    let mut index = load_index_from(data_dir);
    let mut changed = false;
    for i in 0..index.len() {
        if index[i].closed_at.is_none() {
            if let Some(at) = idle_closed_at(&index[i].last_event_at, now) {
                close_locked(
                    data_dir,
                    &mut index,
                    i,
                    DebugSessionCloseReason::Idle,
                    stamp(at),
                );
                changed = true;
            }
        }
    }
    (index, changed)
}

fn recorder_of(by: &BuildActor) -> DebugSessionRecorder {
    match by {
        BuildActor::Agent(agent) if agent.standalone => DebugSessionRecorder::Standalone,
        _ => DebugSessionRecorder::App,
    }
}

/// The digest of `record` a session keeps: the installed APK and its mappings.
fn build_digest(record: &BuildRecord, entry: &InstalledBuild) -> Option<DebugSessionBuild> {
    let apk = record.apks.iter().find(|a| a.sha256 == entry.apk_sha256)?;
    Some(DebugSessionBuild {
        id: record.id,
        task: record.task.clone(),
        started_at: record.started_at.clone(),
        origin: record.origin.clone(),
        apk: DebugSessionApk {
            module: apk.module.clone(),
            variant: apk.variant.clone(),
            sha256: apk.sha256.clone(),
            version_code: apk.version_code,
        },
        mappings: entry
            .mappings
            .iter()
            .map(|m| DebugSessionMapping {
                sha256: m.sha256.clone(),
                pg_map_id: m.pg_map_id.clone(),
            })
            .collect(),
    })
}

// ── Retention ─────────────────────────────────────────────────────────────────

/// The retention settings (`sessions.retentionDays`, `sessions.maxFolderMb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// 0 disables age pruning.
    pub days: u32,
    pub max_folder_mb: u32,
}

impl Retention {
    pub fn from_settings() -> Self {
        let sessions = settings_manager::load_settings().0.sessions;
        Retention {
            days: sessions.retention_days,
            max_folder_mb: sessions.max_folder_mb,
        }
    }
}

/// Total size of the regular files under `dir`, two levels deep, without
/// following symlinks.
fn dir_size(dir: &Path, depth: u32) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match std::fs::symlink_metadata(entry.path()) {
            Ok(meta) if meta.is_file() => meta.len(),
            Ok(meta) if meta.is_dir() && depth > 0 => dir_size(&entry.path(), depth - 1),
            _ => 0,
        })
        .sum()
}

/// The next session retention removes: unkept, not `protect`, closed before
/// open, then least recently active.
fn prune_victim(
    index: &[DebugSessionSummary],
    now: DateTime<Utc>,
    protect: Option<&str>,
) -> Option<usize> {
    index
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.kept && Some(s.id.as_str()) != protect)
        .min_by(|(_, a), (_, b)| {
            (is_open(a, now), &a.last_event_at).cmp(&(is_open(b, now), &b.last_event_at))
        })
        .map(|(i, _)| i)
}

fn remove_session(data_dir: &Path, index: &mut Vec<DebugSessionSummary>, i: usize) {
    let removed = index.remove(i);
    let dir = session_dir(data_dir, &removed.id);
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("Cannot remove debug session {}: {e}", dir.display());
        }
    }
}

/// Apply retention: at most [`MAX_KEPT_SESSIONS`] kept; then unkept sessions
/// idle longer than `retention.days`; then the oldest until the folder is
/// under `retention.max_folder_mb`; then the oldest past [`MAX_SESSIONS`].
/// Kept sessions and `protect` (a session being opened) are never removed.
/// Also removes folders the index does not name, and temporary files, left
/// more than [`ORPHAN_AGE`] ago. Returns how many sessions were removed.
/// Callers hold the data lock and save the index afterwards.
fn prune_locked(
    data_dir: &Path,
    index: &mut Vec<DebugSessionSummary>,
    retention: Retention,
    now: DateTime<Utc>,
    protect: Option<&str>,
) -> usize {
    let before = index.len();

    // Kept cap, newest kept first.
    let mut kept: Vec<usize> = (0..index.len()).filter(|&i| index[i].kept).collect();
    kept.sort_by(|&a, &b| index[b].opened_at.cmp(&index[a].opened_at));
    for &i in kept.iter().skip(MAX_KEPT_SESSIONS) {
        if let Ok(mut session) = read_manifest(data_dir, &index[i].id) {
            session.kept = false;
            let _ = save_manifest(data_dir, &session);
        }
        index[i].kept = false;
    }

    // Age.
    if retention.days > 0 {
        let cutoff = now - chrono::Duration::days(i64::from(retention.days));
        let aged = |s: &DebugSessionSummary| {
            !s.kept
                && Some(s.id.as_str()) != protect
                && DateTime::parse_from_rfc3339(&s.last_event_at)
                    .is_ok_and(|at| at.with_timezone(&Utc) < cutoff)
        };
        while let Some(i) = index.iter().position(aged) {
            remove_session(data_dir, index, i);
        }
    }

    // Size.
    let max_bytes = u64::from(retention.max_folder_mb) * 1024 * 1024;
    let mut sizes: HashMap<String, u64> = index
        .iter()
        .map(|s| (s.id.clone(), dir_size(&session_dir(data_dir, &s.id), 2)))
        .collect();
    let mut total: u64 = sizes.values().sum();
    while total > max_bytes {
        let Some(i) = prune_victim(index, now, protect) else {
            break;
        };
        total = total.saturating_sub(sizes.remove(&index[i].id).unwrap_or(0));
        remove_session(data_dir, index, i);
    }

    // Count.
    while index.len() > MAX_SESSIONS {
        let Some(i) = prune_victim(index, now, protect) else {
            break;
        };
        remove_session(data_dir, index, i);
    }

    remove_orphans(data_dir, index);
    before - index.len()
}

fn remove_orphans(data_dir: &Path, index: &[DebugSessionSummary]) {
    let Ok(entries) = std::fs::read_dir(sessions_dir(data_dir)) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age > ORPHAN_AGE);
        if !old {
            continue;
        }
        if meta.is_dir()
            && validate_debug_session_id(&name).is_ok()
            && !index.iter().any(|s| s.id == name)
        {
            let _ = std::fs::remove_dir_all(entry.path());
        } else if meta.is_file()
            && (name.starts_with(INDEX_FILE) || name.starts_with(crashes::CAPTURE_TMP))
            && name.ends_with(".tmp")
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Apply retention to the saved sessions (at startup).
pub fn prune_persisted(retention: Retention) -> Result<usize, String> {
    prune_persisted_in(&data_dir(), retention, Utc::now())
}

fn prune_persisted_in(
    data_dir: &Path,
    retention: Retention,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    if !sessions_dir(data_dir).is_dir() {
        return Ok(0);
    }
    with_data_lock_in(data_dir, || {
        let mut index = load_index_locked(data_dir, now);
        let removed = prune_locked(data_dir, &mut index, retention, now, None);
        save_index(data_dir, &index)?;
        Ok(removed)
    })?
}

// ── Events ────────────────────────────────────────────────────────────────────

enum Append {
    Recorded(Box<DebugSessionEvent>),
    /// Not recorded: the named cap was reached.
    Dropped(&'static str),
}

/// Append an event to `session`'s log and update its counts. Callers hold the
/// data lock and save the manifest afterwards.
fn append_locked(
    data_dir: &Path,
    session: &mut DebugSession,
    actor: Option<BuildActor>,
    event: DebugSessionEventData,
    now: DateTime<Utc>,
) -> Result<Append, String> {
    let bookmark = matches!(event, DebugSessionEventData::Bookmark(_));
    if session.event_count >= MAX_EVENTS_PER_SESSION {
        session.dropped_events += 1;
        return Ok(Append::Dropped("MAX_EVENTS_PER_SESSION"));
    }
    if bookmark && session.counts.bookmarks >= MAX_BOOKMARKS_PER_SESSION {
        session.dropped_events += 1;
        return Ok(Append::Dropped("MAX_BOOKMARKS_PER_SESSION"));
    }
    let at = stamp(now);
    let recorded = DebugSessionEvent {
        seq: session.event_count + 1,
        at: at.clone(),
        actor,
        event,
    };
    let mut line =
        serde_json::to_string(&recorded).map_err(|e| format!("Cannot serialize: {e}"))?;
    line.push('\n');
    if session.bytes + line.len() as u64 > MAX_SESSION_BYTES {
        session.dropped_events += 1;
        return Ok(Append::Dropped("MAX_SESSION_BYTES"));
    }
    let path = session_dir(data_dir, &session.id).join(EVENTS_FILE);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(line.as_bytes()))
        .map_err(|e| format!("Cannot append to {}: {e}", path.display()))?;
    session.event_count += 1;
    session.bytes += line.len() as u64;
    session.last_event_at = at;
    let counts = &mut session.counts;
    match &recorded.event {
        DebugSessionEventData::Launch(_) => counts.launches += 1,
        DebugSessionEventData::Bookmark(_) => counts.bookmarks += 1,
        DebugSessionEventData::Crash(crash) => {
            counts.crashes += 1;
            counts.captures += u32::from(crash.capture.is_some());
        }
        DebugSessionEventData::Anr(anr) => {
            counts.anrs += 1;
            counts.captures += u32::from(anr.capture.is_some());
        }
        DebugSessionEventData::Exit(_) => counts.exits += 1,
        _ => {}
    }
    Ok(Append::Recorded(Box::new(recorded)))
}

/// Append `event` to the open sessions on `target` (of `package`, when
/// given). Returns how many recorded it.
fn record_for_device_in(
    data_dir: &Path,
    target: &InstallTarget,
    package: Option<&str>,
    actor: Option<BuildActor>,
    event: DebugSessionEventData,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    if !sessions_dir(data_dir).is_dir() {
        return Ok(0);
    }
    with_data_lock_in(data_dir, || {
        let (mut index, mut changed) = load_index_changed(data_dir, now);
        let mut recorded = 0;
        for summary in index.iter_mut() {
            let matches = is_open(summary, now)
                && package.is_none_or(|p| p == summary.package)
                && target
                    .is_same_device(&summary.device.serial, summary.device.avd_name.as_deref());
            if !matches {
                continue;
            }
            let mut session = match read_manifest(data_dir, &summary.id) {
                Ok(session) => session,
                Err(e) => {
                    tracing::warn!("{e}");
                    continue;
                }
            };
            if let Append::Recorded(_) =
                append_locked(data_dir, &mut session, actor.clone(), event.clone(), now)?
            {
                recorded += 1;
            }
            save_manifest(data_dir, &session)?;
            *summary = DebugSessionSummary::from(&session);
            changed = true;
        }
        if changed {
            save_index(data_dir, &index)?;
        }
        Ok(recorded)
    })?
}

// ── Opening a session on install ──────────────────────────────────────────────

/// Open a session for the install `entry` just recorded on `target`, closing
/// the open one on the same device and package as superseded, then apply
/// retention. A failure is logged and never fails the install.
pub fn open_for_install_in(
    data_dir: &Path,
    target: &InstallTarget,
    entry: &InstalledBuild,
    by: BuildActor,
    retention: Retention,
) -> Option<DebugSession> {
    remember_device(target);
    match open_in(data_dir, target, entry, by, retention, Utc::now()) {
        Ok(session) => Some(session),
        Err(e) => {
            tracing::warn!(
                "Debug session for {} on {} not opened: {e}",
                entry.package,
                target.serial
            );
            None
        }
    }
}

fn open_in(
    data_dir: &Path,
    target: &InstallTarget,
    entry: &InstalledBuild,
    by: BuildActor,
    retention: Retention,
    now: DateTime<Utc>,
) -> Result<DebugSession, String> {
    with_data_lock_in(data_dir, || {
        let history = build_runner::load_build_history_from(data_dir);
        let record = entry
            .build_id
            .and_then(|id| history.iter().find(|r| r.id == id));
        let build = record.and_then(|r| build_digest(r, entry));
        let at = stamp(now);

        let mut index = load_index_locked(data_dir, now);
        for i in 0..index.len() {
            let superseded = is_open(&index[i], now)
                && index[i].package == entry.package
                && target
                    .is_same_device(&index[i].device.serial, index[i].device.avd_name.as_deref());
            if superseded {
                close_locked(
                    data_dir,
                    &mut index,
                    i,
                    DebugSessionCloseReason::Superseded,
                    at.clone(),
                );
            }
        }

        let id = create_session_dir(data_dir, now)?;
        let install = DebugSessionInstall {
            apk_sha256: entry.apk_sha256.clone(),
            version_code: entry.version_code,
            installed_at: entry.installed_at.clone(),
            by: by.clone(),
        };
        let mut session = DebugSession {
            schema_version: DEBUG_SESSION_SCHEMA_VERSION,
            id: id.clone(),
            project_root: record.and_then(|r| r.project_root.clone()),
            package: entry.package.clone(),
            device: DebugSessionDevice {
                serial: target.serial.clone(),
                avd_name: target.avd_name.clone(),
                model: target.model.clone(),
            },
            build: build.clone(),
            install: Some(install.clone()),
            opened_at: at.clone(),
            closed_at: None,
            close_reason: None,
            recorded_by: recorder_of(&by),
            kept: false,
            counts: DebugSessionCounts::default(),
            last_event_at: at,
            event_count: 0,
            dropped_events: 0,
            bytes: 0,
        };
        if let Some(build) = build {
            let actor = build.origin.clone();
            append_locked(
                data_dir,
                &mut session,
                actor,
                DebugSessionEventData::Build(build),
                now,
            )?;
        }
        append_locked(
            data_dir,
            &mut session,
            Some(by),
            DebugSessionEventData::Install(install),
            now,
        )?;
        save_manifest(data_dir, &session)?;
        index.push(DebugSessionSummary::from(&session));
        prune_locked(data_dir, &mut index, retention, now, Some(&id));
        save_index(data_dir, &index)?;
        Ok(session)
    })?
}

// ── Queued events ─────────────────────────────────────────────────────────────

/// An event for the open sessions on a device, waiting for the writer.
#[derive(Debug, Clone)]
struct PendingEvent {
    data_dir: PathBuf,
    target: InstallTarget,
    package: Option<String>,
    actor: Option<BuildActor>,
    event: DebugSessionEventData,
}

enum Pending {
    Event(Box<PendingEvent>),
    /// Answered once everything queued before it was written.
    #[cfg(test)]
    Flush(std::sync::mpsc::Sender<()>),
}

/// A bounded queue drained by one thread, so hooks never wait for disk.
struct EventQueue {
    tx: SyncSender<Pending>,
    dropped: AtomicU64,
}

impl EventQueue {
    fn start(capacity: usize, write: impl Fn(PendingEvent) + Send + 'static) -> Self {
        let (tx, rx) = sync_channel::<Pending>(capacity);
        let spawned = std::thread::Builder::new()
            .name("debug-sessions".into())
            .spawn(move || {
                for pending in rx {
                    match pending {
                        Pending::Event(event) => write(*event),
                        #[cfg(test)]
                        Pending::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            });
        if let Err(e) = spawned {
            // The receiver is gone with the closure, so every push is dropped.
            tracing::warn!("Debug session events will not be recorded: {e}");
        }
        EventQueue {
            tx,
            dropped: AtomicU64::new(0),
        }
    }

    /// Queue `event`; count it as dropped when the queue is full.
    fn push(&self, event: PendingEvent) -> bool {
        match self.tx.try_send(Pending::Event(Box::new(event))) {
            Ok(()) => true,
            Err(_) => {
                let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if dropped == 1 || dropped.is_multiple_of(100) {
                    tracing::warn!(
                        "{dropped} debug session events dropped: more than \
                         {MAX_PENDING_SESSION_EVENTS} waiting to be written"
                    );
                }
                false
            }
        }
    }

    #[cfg(test)]
    fn flush(&self) {
        let (done, wait) = std::sync::mpsc::channel();
        self.tx.send(Pending::Flush(done)).expect("writer running");
        wait.recv_timeout(Duration::from_secs(30))
            .expect("writer flushed");
    }
}

static QUEUE: LazyLock<EventQueue> =
    LazyLock::new(|| EventQueue::start(MAX_PENDING_SESSION_EVENTS, write_pending));

fn write_pending(pending: PendingEvent) {
    if let Err(e) = record_for_device_in(
        &pending.data_dir,
        &pending.target,
        pending.package.as_deref(),
        pending.actor,
        pending.event,
        Utc::now(),
    ) {
        tracing::warn!("Debug session event not recorded: {e}");
    }
}

/// Debug session events dropped because the queue was full, in this process.
pub fn dropped_session_events() -> u64 {
    QUEUE.dropped.load(Ordering::Relaxed)
}

fn enqueue(pending: PendingEvent) -> bool {
    QUEUE.push(pending)
}

// ── Device identity for events ────────────────────────────────────────────────

/// The AVD name of each emulator serial this process last saw (serial, AVD),
/// newest last, at most [`MAX_KNOWN_EMULATORS`], so a logcat event (which
/// only knows the serial) finds its sessions.
#[derive(Default)]
struct KnownEmulators(StdMutex<VecDeque<(String, String)>>);

impl KnownEmulators {
    fn entries(&self) -> std::sync::MutexGuard<'_, VecDeque<(String, String)>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Remember which AVD an emulator serial runs, or forget it when unknown.
    fn remember(&self, target: &InstallTarget) {
        if !target.serial.starts_with("emulator-") {
            return;
        }
        let mut known = self.entries();
        known.retain(|(serial, _)| serial != &target.serial);
        if let Some(avd) = &target.avd_name {
            known.push_back((target.serial.clone(), avd.clone()));
            while known.len() > MAX_KNOWN_EMULATORS {
                known.pop_front();
            }
        }
    }

    fn avd_of(&self, serial: &str) -> Option<String> {
        self.entries()
            .iter()
            .find(|(s, _)| s == serial)
            .map(|(_, avd)| avd.clone())
    }
}

static KNOWN_EMULATORS: LazyLock<KnownEmulators> = LazyLock::new(KnownEmulators::default);

fn remember_device(target: &InstallTarget) {
    KNOWN_EMULATORS.remember(target);
}

/// The device `serial` names, with the AVD name this process last saw on it.
fn target_of_serial(serial: &str) -> InstallTarget {
    InstallTarget {
        serial: serial.to_string(),
        avd_name: KNOWN_EMULATORS.avd_of(serial),
        model: None,
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

// ── Hooks ─────────────────────────────────────────────────────────────────────

/// An app launch to add to the session of its device and package.
#[derive(Debug, Clone)]
pub struct LaunchRecord {
    pub serial: String,
    pub package: String,
    pub timing: Option<LaunchTiming>,
    pub restart: bool,
    pub by: BuildActor,
}

impl LaunchRecord {
    /// A launch whose timing `am start -W` printed.
    pub fn from_am_start(
        serial: &str,
        package: &str,
        measured: Option<AmStartTiming>,
        by: BuildActor,
    ) -> Self {
        LaunchRecord {
            serial: serial.to_string(),
            package: package.to_string(),
            timing: measured.map(|m| LaunchTiming {
                total_ms: m.total_ms,
                wait_ms: m.wait_ms,
                launch_state: m.launch_state,
                measured_at: Utc::now().to_rfc3339(),
                serial: serial.to_string(),
                avd_name: None,
                model: None,
                displayed_ms: None,
                fully_drawn_ms: None,
            }),
            restart: false,
            by,
        }
    }
}

/// Record a launch on its session, in the background: an emulator's AVD name
/// is asked first. Never fails or delays the caller.
pub fn record_launch(adb: PathBuf, device_state: DeviceState, launch: LaunchRecord) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        tracing::warn!(
            "Launch of {} not added to its debug session",
            launch.package
        );
        return;
    };
    runtime.spawn(async move {
        let target = installed_builds::resolve_target(&adb, &launch.serial, &device_state).await;
        remember_device(&target);
        record_launch_in(&data_dir(), target, launch);
    });
}

/// Record `restart_app`'s relaunch; see [`record_launch`].
pub fn record_restart(
    adb: PathBuf,
    device_state: DeviceState,
    serial: &str,
    package: &str,
    result: &RestartResult,
    by: BuildActor,
) {
    let measured = result.total_time_ms.map(|total_ms| AmStartTiming {
        total_ms,
        wait_ms: result.wait_time_ms,
        launch_state: result.launch_state,
    });
    let mut launch = LaunchRecord {
        restart: true,
        ..LaunchRecord::from_am_start(serial, package, measured, by)
    };
    if let Some(timing) = &mut launch.timing {
        timing.displayed_ms = result.display_time_ms.and_then(|ms| u32::try_from(ms).ok());
    }
    record_launch(adb, device_state, launch);
}

/// Record display times that arrived after a launch was recorded, on the
/// session of the launch's device and package. Only queues.
pub fn record_late_launch_timing(package: &str, timing: LaunchTiming, by: BuildActor) {
    record_late_launch_timing_in(&data_dir(), package, timing, by);
}

fn record_late_launch_timing_in(
    data_dir: &Path,
    package: &str,
    timing: LaunchTiming,
    by: BuildActor,
) -> bool {
    let target = InstallTarget {
        serial: timing.serial.clone(),
        avd_name: timing.avd_name.clone(),
        model: timing.model.clone(),
    };
    enqueue(PendingEvent {
        data_dir: data_dir.to_path_buf(),
        target,
        package: Some(package.to_string()),
        actor: Some(by),
        event: DebugSessionEventData::LaunchTiming(timing),
    })
}

fn record_launch_in(data_dir: &Path, target: InstallTarget, launch: LaunchRecord) -> bool {
    let timing = launch.timing.map(|mut timing| {
        timing.avd_name = timing.avd_name.or_else(|| target.avd_name.clone());
        timing.model = timing.model.or_else(|| target.model.clone());
        timing
    });
    enqueue(PendingEvent {
        data_dir: data_dir.to_path_buf(),
        target,
        package: Some(launch.package),
        actor: Some(launch.by),
        event: DebugSessionEventData::Launch(DebugSessionLaunch {
            serial: launch.serial,
            timing,
            restart: launch.restart,
        }),
    })
}

/// A change of the logcat stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogcatChange {
    Reconnect,
    /// The stream gave up, with the reason.
    Stopped(String),
    Cleared,
}

/// Record a logcat stream change on the open sessions of the stream's
/// device. A stream without a serial is not attributed. Only queues.
pub fn record_logcat(serial: Option<&str>, change: LogcatChange) {
    record_logcat_in(&data_dir(), serial, change);
}

fn record_logcat_in(data_dir: &Path, serial: Option<&str>, change: LogcatChange) -> bool {
    let Some(serial) = serial else {
        return false;
    };
    let data = |reason| DebugSessionLogcatChange {
        serial: serial.to_string(),
        reason,
    };
    let event = match change {
        LogcatChange::Reconnect => DebugSessionEventData::LogcatReconnect(data(None)),
        LogcatChange::Stopped(reason) => DebugSessionEventData::LogcatStopped(data(Some(
            truncate_chars(&reason, MAX_EVENT_TEXT_CHARS),
        ))),
        LogcatChange::Cleared => DebugSessionEventData::LogcatCleared(data(None)),
    };
    enqueue(PendingEvent {
        data_dir: data_dir.to_path_buf(),
        target: target_of_serial(serial),
        package: None,
        actor: None,
        event,
    })
}

/// Tracks which devices the GUI device poll sees online, and records
/// `deviceOffline` / `deviceOnline` on their open sessions when that changes.
/// The first list observed is the baseline.
#[derive(Default)]
pub struct DevicePresence {
    online: StdMutex<Option<Vec<InstallTarget>>>,
}

impl DevicePresence {
    /// Compare `devices` with the last list. Only queues.
    pub fn observe(&self, devices: &[Device]) {
        self.observe_in(&data_dir(), devices);
    }

    fn observe_in(&self, data_dir: &Path, devices: &[Device]) {
        let now_online: Vec<InstallTarget> = devices
            .iter()
            .filter(|d| d.connection_state == DeviceConnectionState::Online)
            .map(|d| InstallTarget {
                serial: d.serial.clone(),
                avd_name: d.avd_name.clone(),
                model: d.model.clone(),
            })
            // An emulator whose AVD name is not known yet is not identified.
            .filter(|t| t.avd_name.is_some() || !is_emulator(devices, &t.serial))
            .collect();
        for target in &now_online {
            remember_device(target);
        }
        let previous = {
            let mut online = self
                .online
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            online.replace(now_online.clone())
        };
        let Some(previous) = previous else {
            return;
        };
        let missing_from = |list: &[InstallTarget], t: &InstallTarget| {
            !list
                .iter()
                .any(|o| t.is_same_device(&o.serial, o.avd_name.as_deref()))
        };
        let changes = previous
            .iter()
            .filter(|t| missing_from(&now_online, t))
            .map(|t| (t, false))
            .chain(
                now_online
                    .iter()
                    .filter(|t| missing_from(&previous, t))
                    .map(|t| (t, true)),
            );
        for (target, online) in changes {
            let change = DebugSessionDeviceChange {
                serial: target.serial.clone(),
            };
            enqueue(PendingEvent {
                data_dir: data_dir.to_path_buf(),
                target: target.clone(),
                package: None,
                actor: None,
                event: if online {
                    DebugSessionEventData::DeviceOnline(change)
                } else {
                    DebugSessionEventData::DeviceOffline(change)
                },
            });
        }
    }
}

fn is_emulator(devices: &[Device], serial: &str) -> bool {
    devices
        .iter()
        .any(|d| d.serial == serial && d.device_kind == DeviceKind::Emulator)
        || serial.starts_with("emulator-")
}

// ── Commands ──────────────────────────────────────────────────────────────────

fn not_found(id: &str) -> AppError {
    AppError::NotFound(format!("Debug session {id} is no longer kept"))
}

fn checked_id(id: &str) -> Result<(), AppError> {
    validate_debug_session_id(id).map_err(AppError::InvalidInput)
}

/// Every session, newest first, at most [`MAX_SESSIONS`].
pub fn list_sessions() -> Vec<DebugSessionSummary> {
    list_sessions_in(&data_dir(), Utc::now())
}

fn list_sessions_in(data_dir: &Path, now: DateTime<Utc>) -> Vec<DebugSessionSummary> {
    load_index_from(data_dir)
        .into_iter()
        .rev()
        .take(MAX_SESSIONS)
        .map(|s| with_idle_view(s, now))
        .collect()
}

/// A session and its newest [`MAX_EVENTS_RETURNED`] events.
pub fn get_session(id: &str) -> Result<DebugSessionDetail, AppError> {
    get_session_in(&data_dir(), id, Utc::now())
}

fn get_session_in(
    data_dir: &Path,
    id: &str,
    now: DateTime<Utc>,
) -> Result<DebugSessionDetail, AppError> {
    checked_id(id)?;
    let mut session = read_manifest(data_dir, id).map_err(|_| not_found(id))?;
    if session.closed_at.is_none() {
        if let Some(at) = idle_closed_at(&session.last_event_at, now) {
            session.closed_at = Some(stamp(at));
            session.close_reason = Some(DebugSessionCloseReason::Idle);
        }
    }
    let path = session_dir(data_dir, id).join(EVENTS_FILE);
    let mut text = String::new();
    if let Ok(file) = std::fs::File::open(&path) {
        let mut limited = file.take(MAX_SESSION_BYTES);
        limited
            .read_to_string(&mut text)
            .map_err(|e| AppError::io(path.display(), e))?;
    }
    let mut events: Vec<DebugSessionEvent> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let crashes = crashes::crash_events(&events);
    let events_truncated = events.len() > MAX_EVENTS_RETURNED;
    if events_truncated {
        events.drain(..events.len() - MAX_EVENTS_RETURNED);
    }
    Ok(DebugSessionDetail {
        session,
        events,
        events_truncated,
        crashes,
    })
}

/// Run `change` on session `id` under the data lock and save it.
fn update_session_in(
    data_dir: &Path,
    id: &str,
    now: DateTime<Utc>,
    change: impl FnOnce(&mut Vec<DebugSessionSummary>, usize) -> Result<(), AppError>,
) -> Result<(), AppError> {
    checked_id(id)?;
    with_data_lock_in(data_dir, || {
        let mut index = load_index_locked(data_dir, now);
        let i = index
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| not_found(id))?;
        change(&mut index, i)?;
        save_index(data_dir, &index).map_err(AppError::Other)
    })
    .map_err(AppError::Other)?
}

/// End an open session, then apply retention. Ending a closed one does nothing.
pub fn end_session(id: &str) -> Result<(), AppError> {
    end_session_in(&data_dir(), id, Retention::from_settings(), Utc::now())
}

fn end_session_in(
    data_dir: &Path,
    id: &str,
    retention: Retention,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    update_session_in(data_dir, id, now, |index, i| {
        if index[i].closed_at.is_none() {
            close_locked(
                data_dir,
                index,
                i,
                DebugSessionCloseReason::Ended,
                stamp(now),
            );
            prune_locked(data_dir, index, retention, now, None);
        }
        Ok(())
    })
}

/// Mark a session Keep (exempt from age pruning; pins its R8 mappings) or not.
/// At most [`MAX_KEPT_SESSIONS`] are kept.
pub fn set_kept(id: &str, kept: bool) -> Result<(), AppError> {
    set_kept_in(&data_dir(), id, kept, Utc::now())
}

fn set_kept_in(data_dir: &Path, id: &str, kept: bool, now: DateTime<Utc>) -> Result<(), AppError> {
    update_session_in(data_dir, id, now, |index, i| {
        if kept && !index[i].kept && index.iter().filter(|s| s.kept).count() >= MAX_KEPT_SESSIONS {
            return Err(AppError::InvalidInput(format!(
                "At most {MAX_KEPT_SESSIONS} debug sessions can be kept. Stop keeping one first."
            )));
        }
        let mut session = read_manifest(data_dir, id).map_err(AppError::Other)?;
        session.kept = kept;
        save_manifest(data_dir, &session).map_err(AppError::Other)?;
        index[i] = DebugSessionSummary::from(&session);
        Ok(())
    })
}

/// Add a bookmark to session `session_id`, or, without one, to the newest
/// open session on `device` (the selected device), or on any device when
/// none is selected.
pub fn add_bookmark(
    session_id: Option<&str>,
    device: Option<&InstallTarget>,
    note: &str,
    log_entry_id: Option<u64>,
) -> Result<DebugSessionEvent, AppError> {
    add_bookmark_in(
        &data_dir(),
        session_id,
        device,
        note,
        log_entry_id,
        Utc::now(),
    )
}

fn add_bookmark_in(
    data_dir: &Path,
    session_id: Option<&str>,
    device: Option<&InstallTarget>,
    note: &str,
    log_entry_id: Option<u64>,
    now: DateTime<Utc>,
) -> Result<DebugSessionEvent, AppError> {
    let note = note.trim();
    if note.is_empty() {
        return Err(AppError::InvalidInput("A bookmark needs a note".into()));
    }
    if note.chars().count() > MAX_BOOKMARK_NOTE_CHARS {
        return Err(AppError::InvalidInput(format!(
            "A bookmark note is at most {MAX_BOOKMARK_NOTE_CHARS} characters"
        )));
    }
    if let Some(id) = session_id {
        checked_id(id)?;
    }
    with_data_lock_in(data_dir, || {
        let mut index = load_index_locked(data_dir, now);
        let i = match session_id {
            Some(id) => {
                let i = index
                    .iter()
                    .position(|s| s.id == id)
                    .ok_or_else(|| not_found(id))?;
                if !is_open(&index[i], now) {
                    return Err(AppError::InvalidInput(format!(
                        "Debug session {id} is closed"
                    )));
                }
                i
            }
            None => index
                .iter()
                .rposition(|s| {
                    is_open(s, now)
                        && device.is_none_or(|d| {
                            d.is_same_device(&s.device.serial, s.device.avd_name.as_deref())
                        })
                })
                .ok_or_else(|| {
                    AppError::NotFound(match device {
                        Some(d) => format!("No debug session is open on {}", d.serial),
                        None => "No debug session is open".into(),
                    })
                })?,
        };
        let mut session = read_manifest(data_dir, &index[i].id).map_err(AppError::Other)?;
        let appended = append_locked(
            data_dir,
            &mut session,
            Some(BuildActor::App),
            DebugSessionEventData::Bookmark(DebugSessionBookmark {
                note: note.to_string(),
                log_entry_id,
            }),
            now,
        )
        .map_err(AppError::Other)?;
        save_manifest(data_dir, &session).map_err(AppError::Other)?;
        index[i] = DebugSessionSummary::from(&session);
        save_index(data_dir, &index).map_err(AppError::Other)?;
        match appended {
            Append::Recorded(event) => Ok(*event),
            Append::Dropped(cap) => Err(AppError::InvalidInput(format!(
                "Debug session {} is full ({cap})",
                session.id
            ))),
        }
    })
    .map_err(AppError::Other)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::build::{AgentActor, BuildStatus, BuiltApk, MappingSnapshot};
    use crate::services::mapping_snapshots;
    use tempfile::TempDir;

    const RETAIN: Retention = Retention {
        days: 14,
        max_folder_mb: 200,
    };

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn phone(serial: &str) -> InstallTarget {
        InstallTarget {
            serial: serial.into(),
            avd_name: None,
            model: Some("Pixel 8".into()),
        }
    }

    fn emulator(serial: &str, avd: &str) -> InstallTarget {
        InstallTarget {
            serial: serial.into(),
            avd_name: Some(avd.into()),
            model: None,
        }
    }

    fn mapping(n: u8) -> MappingSnapshot {
        MappingSnapshot {
            module: ":app".into(),
            variant: "release".into(),
            sha256: format!("{n:064x}"),
            bytes: 1,
            pg_map_id: Some(format!("map{n}")),
        }
    }

    /// What `record_install_in` saved for an install of `apk` on `target`.
    fn installed(
        target: &InstallTarget,
        package: &str,
        apk: &str,
        build_id: Option<u32>,
        mappings: Vec<MappingSnapshot>,
    ) -> InstalledBuild {
        InstalledBuild {
            serial: target.serial.clone(),
            avd_name: target.avd_name.clone(),
            model: target.model.clone(),
            package: package.into(),
            apk_sha256: apk.into(),
            build_id,
            version_code: Some(7),
            mappings,
            installed_at: "2026-09-25T10:32:00+00:00".into(),
        }
    }

    fn record(id: u32, apk: &str) -> BuildRecord {
        BuildRecord {
            id,
            task: "assembleRelease".into(),
            status: BuildStatus::Cancelled,
            errors: vec![],
            started_at: "2026-09-25T10:30:00+00:00".into(),
            project_root: Some("/work/app".into()),
            origin: Some(BuildActor::App),
            cancelled_by: None,
            launch: None,
            mappings: vec![],
            apks: vec![BuiltApk {
                module: ":app".into(),
                variant: "release".into(),
                application_id: Some("com.example".into()),
                version_code: Some(7),
                sha256: apk.into(),
                bytes: 1,
                path: "app/build/outputs/apk/release/app-release.apk".into(),
            }],
        }
    }

    fn write_history(dir: &Path, records: &[BuildRecord]) {
        let newest_first: Vec<&BuildRecord> = records.iter().rev().collect();
        std::fs::write(
            dir.join("build-history.json"),
            serde_json::to_string(&newest_first).unwrap(),
        )
        .unwrap();
    }

    fn open_at(
        dir: &Path,
        target: &InstallTarget,
        package: &str,
        now: DateTime<Utc>,
    ) -> DebugSession {
        let entry = installed(target, package, &"a".repeat(64), None, vec![]);
        open_in(dir, target, &entry, BuildActor::App, RETAIN, now).unwrap()
    }

    fn open(dir: &Path, target: &InstallTarget, package: &str) -> DebugSession {
        open_at(dir, target, package, Utc::now())
    }

    fn events(dir: &Path, id: &str) -> Vec<DebugSessionEvent> {
        get_session_in(dir, id, Utc::now()).unwrap().events
    }

    fn kinds(dir: &Path, id: &str) -> Vec<String> {
        events(dir, id)
            .iter()
            .map(|e| {
                serde_json::to_value(e).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    fn summary(dir: &Path, id: &str) -> DebugSessionSummary {
        list_sessions_in(dir, Utc::now())
            .into_iter()
            .find(|s| s.id == id)
            .expect("listed")
    }

    fn ids(dir: &Path) -> Vec<String> {
        load_index_from(dir).into_iter().map(|s| s.id).collect()
    }

    fn agent(standalone: bool) -> BuildActor {
        BuildActor::Agent(AgentActor {
            session_id: None,
            client_name: Some("claude-code".into()),
            standalone,
        })
    }

    // ── Opening and closing ──────────────────────────────────────────────────

    #[test]
    fn an_install_opens_a_session_naming_its_build() {
        let dir = TempDir::new().unwrap();
        let apk = "a".repeat(64);
        write_history(dir.path(), &[record(4, &apk)]);
        let target = emulator("emulator-5554", "Pixel_7");
        let entry = installed(&target, "com.example", &apk, Some(4), vec![mapping(1)]);

        let session =
            open_for_install_in(dir.path(), &target, &entry, agent(false), RETAIN).expect("opened");

        assert_eq!(session.schema_version, 1);
        assert_eq!(session.package, "com.example");
        assert_eq!(session.device.avd_name.as_deref(), Some("Pixel_7"));
        assert_eq!(session.project_root.as_deref(), Some("/work/app"));
        assert_eq!(session.recorded_by, DebugSessionRecorder::App);
        let build = session.build.clone().expect("build digest");
        assert_eq!(build.id, 4);
        assert_eq!(build.task, "assembleRelease");
        assert_eq!(build.apk.module, ":app");
        assert_eq!(build.apk.variant, "release");
        assert_eq!(build.apk.sha256, apk);
        assert_eq!(build.apk.version_code, Some(7));
        assert_eq!(build.mappings[0].sha256, mapping(1).sha256);
        assert_eq!(build.mappings[0].pg_map_id.as_deref(), Some("map1"));
        assert_eq!(session.install.as_ref().unwrap().by, agent(false));
        assert_eq!(read_manifest(dir.path(), &session.id).unwrap(), session);
        assert_eq!(kinds(dir.path(), &session.id), ["build", "install"]);
        let listed = summary(dir.path(), &session.id);
        assert_eq!(listed.build_id, Some(4));
        assert_eq!(listed.mapping_sha256s, vec![mapping(1).sha256]);
        assert_eq!(listed.event_count, 2);
        assert!(listed.closed_at.is_none());
    }

    #[test]
    fn an_apk_no_recorded_build_wrote_opens_an_unattributed_session() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path(), &phone("R5CT"), "com.example.studio");

        assert_eq!(session.build, None);
        assert!(session.install.is_some());
        assert_eq!(summary(dir.path(), &session.id).build_id, None);
        assert_eq!(kinds(dir.path(), &session.id), ["install"]);
    }

    #[test]
    fn a_standalone_mcp_install_is_labelled_standalone() {
        let dir = TempDir::new().unwrap();
        let target = phone("R5CT");
        let entry = installed(&target, "com.example", &"a".repeat(64), None, vec![]);

        let session =
            open_in(dir.path(), &target, &entry, agent(true), RETAIN, Utc::now()).unwrap();

        assert_eq!(session.recorded_by, DebugSessionRecorder::Standalone);
    }

    #[test]
    fn a_later_install_supersedes_the_session_on_the_same_device_and_package() {
        let dir = TempDir::new().unwrap();
        let first = open(dir.path(), &phone("R5CT"), "com.a");
        let other_package = open(dir.path(), &phone("R5CT"), "com.b");
        let other_device = open(dir.path(), &phone("ZX1G"), "com.a");
        let second = open(dir.path(), &phone("R5CT"), "com.a");

        let first = summary(dir.path(), &first.id);
        assert_eq!(
            first.close_reason,
            Some(DebugSessionCloseReason::Superseded)
        );
        assert_eq!(
            read_manifest(dir.path(), &first.id).unwrap().close_reason,
            Some(DebugSessionCloseReason::Superseded)
        );
        for open in [&other_package.id, &other_device.id, &second.id] {
            assert!(summary(dir.path(), open).closed_at.is_none(), "{open}");
        }
    }

    #[test]
    fn an_emulator_session_is_superseded_by_its_avd_not_its_serial() {
        let dir = TempDir::new().unwrap();
        let pixel_7 = open(dir.path(), &emulator("emulator-5554", "Pixel_7"), "com.a");
        // Pixel_8 takes Pixel_7's old serial; Pixel_7 comes back on another.
        let pixel_8 = open(dir.path(), &emulator("emulator-5554", "Pixel_8"), "com.a");
        assert!(summary(dir.path(), &pixel_7.id).closed_at.is_none());

        open(dir.path(), &emulator("emulator-5556", "Pixel_7"), "com.a");

        assert!(summary(dir.path(), &pixel_7.id).closed_at.is_some());
        assert!(summary(dir.path(), &pixel_8.id).closed_at.is_none());
    }

    #[test]
    fn an_idle_session_reads_as_closed_without_being_rewritten() {
        let dir = TempDir::new().unwrap();
        let session = open_at(
            dir.path(),
            &phone("R5CT"),
            "com.a",
            at("2026-09-20T10:00:00Z"),
        );
        let later = at("2026-09-21T10:00:01Z");

        let listed = list_sessions_in(dir.path(), later);
        assert_eq!(listed[0].close_reason, Some(DebugSessionCloseReason::Idle));
        assert_eq!(
            listed[0].closed_at.as_deref(),
            Some("2026-09-21T10:00:00.000000Z")
        );
        assert_eq!(
            get_session_in(dir.path(), &session.id, later)
                .unwrap()
                .session
                .close_reason,
            Some(DebugSessionCloseReason::Idle)
        );
        // Computed when read, not written by a timer.
        assert_eq!(
            read_manifest(dir.path(), &session.id).unwrap().closed_at,
            None
        );
        // Still open a moment before the timeout.
        assert!(list_sessions_in(dir.path(), at("2026-09-21T09:59:59Z"))[0]
            .closed_at
            .is_none());
    }

    #[test]
    fn an_idle_session_gets_no_more_events_and_is_closed_as_idle_on_the_next_write() {
        let dir = TempDir::new().unwrap();
        let target = phone("R5CT");
        let session = open_at(dir.path(), &target, "com.a", at("2026-09-20T10:00:00Z"));
        let later = at("2026-09-22T10:00:00Z");

        let recorded = record_for_device_in(
            dir.path(),
            &target,
            None,
            None,
            DebugSessionEventData::DeviceOnline(DebugSessionDeviceChange {
                serial: "R5CT".into(),
            }),
            later,
        )
        .unwrap();
        open_at(dir.path(), &target, "com.a", later);

        assert_eq!(recorded, 0);
        let manifest = read_manifest(dir.path(), &session.id).unwrap();
        assert_eq!(manifest.close_reason, Some(DebugSessionCloseReason::Idle));
        assert_eq!(manifest.event_count, 1);
    }

    #[test]
    fn ending_a_session_closes_it() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path(), &phone("R5CT"), "com.a");

        end_session_in(dir.path(), &session.id, RETAIN, Utc::now()).unwrap();

        assert_eq!(
            summary(dir.path(), &session.id).close_reason,
            Some(DebugSessionCloseReason::Ended)
        );
        assert!(matches!(
            end_session_in(
                dir.path(),
                "s-20260925T103200Z-000000000000",
                RETAIN,
                Utc::now()
            ),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            end_session_in(dir.path(), "../settings", RETAIN, Utc::now()),
            Err(AppError::InvalidInput(_))
        ));
    }

    // ── Ids and other processes ──────────────────────────────────────────────

    const PRINT_IDS_ENV: &str = "KEYNOBI_TEST_PRINT_SESSION_IDS";
    const IDS_PER_PROCESS: usize = 200;

    fn ids_at_one_instant() -> Vec<String> {
        let now = at("2026-09-25T10:32:00Z");
        (0..IDS_PER_PROCESS).map(|_| new_session_id(now)).collect()
    }

    /// Run by `ids_are_unique_across_processes` in a second process.
    #[test]
    #[ignore = "run by ids_are_unique_across_processes in another process"]
    fn print_session_ids_for_another_process() {
        if std::env::var_os(PRINT_IDS_ENV).is_some() {
            // The test harness has not ended its own line yet.
            println!();
            for id in ids_at_one_instant() {
                println!("ID {id}");
            }
        }
    }

    #[test]
    fn ids_are_unique_across_processes() {
        let other = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "services::debug_sessions::tests::print_session_ids_for_another_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(PRINT_IDS_ENV, "1")
            .output()
            .unwrap();
        let theirs: Vec<String> = String::from_utf8_lossy(&other.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("ID ").map(str::to_owned))
            .collect();
        assert_eq!(theirs.len(), IDS_PER_PROCESS, "{other:?}");
        let mine = ids_at_one_instant();

        let all: std::collections::HashSet<&String> = mine.iter().chain(&theirs).collect();
        assert_eq!(all.len(), 2 * IDS_PER_PROCESS);
        for id in mine.iter().chain(&theirs) {
            validate_debug_session_id(id).unwrap();
        }
    }

    #[test]
    fn sessions_opened_by_two_processes_are_both_kept() {
        let dir = TempDir::new().unwrap();
        // The app and a standalone server each open one; both re-read the index.
        let app = open(dir.path(), &phone("R5CT"), "com.a");
        let target = phone("ZX1G");
        let entry = installed(&target, "com.a", &"b".repeat(64), None, vec![]);
        let standalone =
            open_in(dir.path(), &target, &entry, agent(true), RETAIN, Utc::now()).unwrap();

        assert_eq!(ids(dir.path()), vec![app.id, standalone.id]);
    }

    #[test]
    fn a_write_waits_for_another_process_holding_the_data_lock_and_keeps_its_session() {
        let dir = TempDir::new().unwrap();
        let theirs = open(dir.path(), &phone("ZX1G"), "com.a");
        // Another process opens the lock file itself; a second handle stands in for it.
        let other_process = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.path().join(".lock"))
            .unwrap();
        other_process.lock().unwrap();
        let path = dir.path().to_path_buf();
        let opening = std::thread::spawn(move || open(&path, &phone("R5CT"), "com.a"));
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !opening.is_finished(),
            "wrote while another process held the lock"
        );
        // The other process ends its session meanwhile, then releases the lock.
        let mut index = load_index_from(dir.path());
        index[0].closed_at = Some("2026-09-25T11:00:00+00:00".into());
        index[0].close_reason = Some(DebugSessionCloseReason::Ended);
        save_index(dir.path(), &index).unwrap();
        other_process.unlock().unwrap();
        let mine = opening.join().unwrap();

        let index = load_index_from(dir.path());
        assert_eq!(ids(dir.path()), vec![theirs.id, mine.id]);
        assert_eq!(index[0].close_reason, Some(DebugSessionCloseReason::Ended));
    }

    #[test]
    fn a_lost_index_is_rebuilt_from_the_manifests() {
        let dir = TempDir::new().unwrap();
        let first = open(dir.path(), &phone("R5CT"), "com.a");
        let second = open(dir.path(), &phone("ZX1G"), "com.a");
        std::fs::write(sessions_dir(dir.path()).join(INDEX_FILE), "{not json").unwrap();

        assert_eq!(ids(dir.path()), vec![first.id, second.id]);
    }

    // ── Caps ─────────────────────────────────────────────────────────────────

    /// Rewrite the manifest (and index) of `id` through `change`.
    fn tamper(dir: &Path, id: &str, change: impl FnOnce(&mut DebugSession)) {
        let mut session = read_manifest(dir, id).unwrap();
        change(&mut session);
        save_manifest(dir, &session).unwrap();
        let mut index = load_index_from(dir);
        for s in index.iter_mut().filter(|s| s.id == id) {
            *s = DebugSessionSummary::from(&session);
        }
        save_index(dir, &index).unwrap();
    }

    fn device_online(dir: &Path, target: &InstallTarget) -> usize {
        record_for_device_in(
            dir,
            target,
            None,
            None,
            DebugSessionEventData::DeviceOnline(DebugSessionDeviceChange {
                serial: target.serial.clone(),
            }),
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn events_past_the_cap_are_counted_as_dropped() {
        let dir = TempDir::new().unwrap();
        let target = phone("R5CT");
        let session = open(dir.path(), &target, "com.a");
        tamper(dir.path(), &session.id, |s| {
            s.event_count = MAX_EVENTS_PER_SESSION - 1
        });

        assert_eq!(device_online(dir.path(), &target), 1);
        assert_eq!(device_online(dir.path(), &target), 0);

        let listed = summary(dir.path(), &session.id);
        assert_eq!(listed.event_count, MAX_EVENTS_PER_SESSION);
        assert_eq!(listed.dropped_events, 1);
    }

    #[test]
    fn an_event_log_past_the_byte_cap_takes_no_more() {
        let dir = TempDir::new().unwrap();
        let target = phone("R5CT");
        let session = open(dir.path(), &target, "com.a");
        tamper(dir.path(), &session.id, |s| {
            s.bytes = MAX_SESSION_BYTES - 10
        });

        assert_eq!(device_online(dir.path(), &target), 0);

        assert_eq!(summary(dir.path(), &session.id).dropped_events, 1);
        assert_eq!(events(dir.path(), &session.id).len(), 1);
    }

    #[test]
    fn bookmarks_are_capped_and_their_notes_bounded() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path(), &phone("R5CT"), "com.a");
        let long = "x".repeat(MAX_BOOKMARK_NOTE_CHARS + 1);
        let bookmark = |note: &str| {
            add_bookmark_in(dir.path(), Some(&session.id), None, note, None, Utc::now())
        };

        assert!(matches!(bookmark(&long), Err(AppError::InvalidInput(_))));
        assert!(matches!(bookmark("  "), Err(AppError::InvalidInput(_))));
        assert!(bookmark(&long[1..]).is_ok());
        tamper(dir.path(), &session.id, |s| {
            s.counts.bookmarks = MAX_BOOKMARKS_PER_SESSION
        });
        assert!(matches!(
            bookmark("one more"),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn a_logcat_stop_reason_is_bounded() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path(), &phone("R5CT"), "com.a");

        let reason = "r".repeat(MAX_EVENT_TEXT_CHARS * 2);
        assert!(record_logcat_in(
            dir.path(),
            Some("R5CT"),
            LogcatChange::Stopped(reason)
        ));
        QUEUE.flush();

        let DebugSessionEventData::LogcatStopped(stopped) =
            &events(dir.path(), &session.id)[1].event
        else {
            panic!("{:?}", events(dir.path(), &session.id));
        };
        assert_eq!(
            stopped.reason.as_ref().map(|r| r.chars().count()),
            Some(MAX_EVENT_TEXT_CHARS)
        );
    }

    #[test]
    fn a_session_view_returns_only_the_newest_events() {
        let dir = TempDir::new().unwrap();
        let target = phone("R5CT");
        let session = open(dir.path(), &target, "com.a");
        for _ in 0..MAX_EVENTS_RETURNED + 5 {
            device_online(dir.path(), &target);
        }

        let detail = get_session_in(dir.path(), &session.id, Utc::now()).unwrap();

        assert!(detail.events_truncated);
        assert_eq!(detail.events.len(), MAX_EVENTS_RETURNED);
        assert_eq!(
            detail.events.last().unwrap().seq as usize,
            MAX_EVENTS_RETURNED + 6
        );
    }

    #[test]
    fn at_most_the_kept_cap_can_be_kept() {
        let dir = TempDir::new().unwrap();
        let sessions: Vec<DebugSession> = (0..=MAX_KEPT_SESSIONS)
            .map(|n| open(dir.path(), &phone(&format!("SERIAL{n}")), "com.a"))
            .collect();
        for s in &sessions[..MAX_KEPT_SESSIONS] {
            set_kept_in(dir.path(), &s.id, true, Utc::now()).unwrap();
        }

        let last = &sessions[MAX_KEPT_SESSIONS].id;
        assert!(matches!(
            set_kept_in(dir.path(), last, true, Utc::now()),
            Err(AppError::InvalidInput(_))
        ));
        set_kept_in(dir.path(), &sessions[0].id, false, Utc::now()).unwrap();
        set_kept_in(dir.path(), last, true, Utc::now()).unwrap();
        assert!(read_manifest(dir.path(), last).unwrap().kept);
    }

    #[test]
    fn pruning_unkeeps_the_oldest_past_the_kept_cap() {
        let dir = TempDir::new().unwrap();
        let sessions: Vec<DebugSession> = (0..MAX_KEPT_SESSIONS + 2)
            .map(|n| open(dir.path(), &phone(&format!("SERIAL{n}")), "com.a"))
            .collect();
        // Kept past the cap by hand or by two processes at once.
        for s in &sessions {
            tamper(dir.path(), &s.id, |s| s.kept = true);
        }

        prune_persisted_in(dir.path(), RETAIN, Utc::now()).unwrap();

        let kept: Vec<bool> = sessions
            .iter()
            .map(|s| read_manifest(dir.path(), &s.id).unwrap().kept)
            .collect();
        assert_eq!(kept, [false, false, true, true, true, true, true]);
    }

    #[test]
    fn known_emulators_are_capped_and_forget_an_unnamed_one() {
        let known = KnownEmulators::default();
        for n in 0..MAX_KNOWN_EMULATORS + 1 {
            known.remember(&emulator(&format!("emulator-{}", 7000 + n), "Cap"));
        }
        assert_eq!(known.entries().len(), MAX_KNOWN_EMULATORS);
        assert_eq!(known.avd_of("emulator-7000"), None);
        assert_eq!(known.avd_of("emulator-7001").as_deref(), Some("Cap"));
        known.remember(&InstallTarget {
            avd_name: None,
            ..emulator("emulator-7001", "")
        });
        assert_eq!(known.avd_of("emulator-7001"), None);
        known.remember(&phone("R5CT"));
        assert_eq!(known.avd_of("R5CT"), None);
    }

    // ── Retention ────────────────────────────────────────────────────────────

    #[test]
    fn retention_removes_sessions_idle_past_the_retention_days_unless_kept() {
        let dir = TempDir::new().unwrap();
        let old = open_at(
            dir.path(),
            &phone("A1"),
            "com.a",
            at("2026-09-01T10:00:00Z"),
        );
        let old_kept = open_at(
            dir.path(),
            &phone("A2"),
            "com.a",
            at("2026-09-01T10:00:00Z"),
        );
        tamper(dir.path(), &old_kept.id, |s| s.kept = true);
        let recent = open_at(
            dir.path(),
            &phone("A3"),
            "com.a",
            at("2026-09-12T10:00:00Z"),
        );

        prune_persisted_in(dir.path(), RETAIN, at("2026-09-25T10:00:00Z")).unwrap();

        assert_eq!(ids(dir.path()), vec![old_kept.id, recent.id]);
        assert!(!session_dir(dir.path(), &old.id).exists());
    }

    #[test]
    fn zero_retention_days_keeps_sessions_of_any_age() {
        let dir = TempDir::new().unwrap();
        open_at(
            dir.path(),
            &phone("A1"),
            "com.a",
            at("2025-01-01T10:00:00Z"),
        );
        let forever = Retention { days: 0, ..RETAIN };

        prune_persisted_in(dir.path(), forever, at("2026-09-25T10:00:00Z")).unwrap();

        assert_eq!(ids(dir.path()).len(), 1);
    }

    fn pad(dir: &Path, id: &str, bytes: usize) {
        std::fs::write(session_dir(dir, id).join("padding"), vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn retention_removes_the_oldest_until_the_folder_fits() {
        let dir = TempDir::new().unwrap();
        let s: Vec<DebugSession> = (0..4)
            .map(|n| open(dir.path(), &phone(&format!("A{n}")), "com.a"))
            .collect();
        for session in &s {
            pad(dir.path(), &session.id, 400 * 1024);
        }
        tamper(dir.path(), &s[0].id, |s| s.kept = true);
        let one_mb = Retention {
            max_folder_mb: 1,
            ..RETAIN
        };

        prune_persisted_in(dir.path(), one_mb, Utc::now()).unwrap();

        // The kept one stays and still counts toward the size.
        assert_eq!(ids(dir.path()), vec![s[0].id.clone(), s[3].id.clone()]);
    }

    #[test]
    fn retention_by_size_removes_closed_sessions_before_open_ones() {
        let dir = TempDir::new().unwrap();
        let open_old = open(dir.path(), &phone("A1"), "com.a");
        let closed = open(dir.path(), &phone("A2"), "com.a");
        end_session_in(dir.path(), &closed.id, RETAIN, Utc::now()).unwrap();
        for id in [&open_old.id, &closed.id] {
            pad(dir.path(), id, 700 * 1024);
        }
        let one_mb = Retention {
            max_folder_mb: 1,
            ..RETAIN
        };

        prune_persisted_in(dir.path(), one_mb, Utc::now()).unwrap();

        assert_eq!(ids(dir.path()), vec![open_old.id]);
    }

    #[test]
    fn retention_keeps_at_most_the_session_cap() {
        let dir = TempDir::new().unwrap();
        let opened: Vec<String> = (0..MAX_SESSIONS + 2)
            .map(|n| open(dir.path(), &phone(&format!("S{n}")), "com.a").id)
            .collect();

        // Opening applies retention too.
        assert_eq!(ids(dir.path()), opened[2..]);
        assert!(!session_dir(dir.path(), &opened[0]).exists());
    }

    #[test]
    fn pruning_removes_abandoned_folders_only_once_they_are_old() {
        let dir = TempDir::new().unwrap();
        open(dir.path(), &phone("A1"), "com.a");
        let fresh = sessions_dir(dir.path()).join("s-20260925T103200Z-000000000001");
        let abandoned = sessions_dir(dir.path()).join("s-20260925T103200Z-000000000002");
        let foreign = sessions_dir(dir.path()).join("notes");
        for d in [&fresh, &abandoned, &foreign] {
            std::fs::create_dir(d).unwrap();
        }
        let old = SystemTime::now() - ORPHAN_AGE * 2;
        for d in [&abandoned, &foreign] {
            std::fs::File::open(d).unwrap().set_modified(old).unwrap();
        }

        prune_persisted_in(dir.path(), RETAIN, Utc::now()).unwrap();

        assert!(fresh.exists());
        assert!(!abandoned.exists());
        assert!(foreign.exists());
    }

    #[test]
    fn a_kept_sessions_mapping_survives_mapping_pruning() {
        let dir = TempDir::new().unwrap();
        let apk = "a".repeat(64);
        let history = std::collections::VecDeque::from([record(4, &apk)]);
        write_history(dir.path(), &Vec::from(history.clone()));
        std::fs::create_dir_all(mapping_snapshots::mappings_dir(dir.path())).unwrap();
        let snapshot = mapping_snapshots::snapshot_path(dir.path(), &mapping(1).sha256).unwrap();
        std::fs::write(&snapshot, "x").unwrap();
        let target = phone("R5CT");
        let entry = installed(&target, "com.example", &apk, Some(4), vec![mapping(1)]);
        let session = open_in(
            dir.path(),
            &target,
            &entry,
            BuildActor::App,
            RETAIN,
            Utc::now(),
        )
        .unwrap();

        set_kept_in(dir.path(), &session.id, true, Utc::now()).unwrap();
        with_data_lock_in(dir.path(), || {
            build_runner::prune_mappings(dir.path(), &history)
        })
        .unwrap();
        assert!(snapshot.exists(), "a kept session pins its mapping");

        set_kept_in(dir.path(), &session.id, false, Utc::now()).unwrap();
        with_data_lock_in(dir.path(), || {
            build_runner::prune_mappings(dir.path(), &history)
        })
        .unwrap();
        assert!(!snapshot.exists(), "an unkept session does not");
    }

    // ── Timeline hooks ───────────────────────────────────────────────────────

    fn launch(serial: &str, package: &str, restart: bool) -> LaunchRecord {
        let mut launch = LaunchRecord {
            restart,
            ..LaunchRecord::from_am_start(
                serial,
                package,
                Some(AmStartTiming {
                    total_ms: 240,
                    wait_ms: Some(244),
                    launch_state: None,
                }),
                agent(false),
            )
        };
        if let Some(timing) = &mut launch.timing {
            timing.displayed_ms = restart.then_some(512);
        }
        launch
    }

    #[test]
    fn a_launch_is_recorded_on_the_session_of_its_device_and_package() {
        let dir = TempDir::new().unwrap();
        let target = emulator("emulator-5554", "Pixel_7");
        let session = open(dir.path(), &target, "com.a");
        let other_package = open(dir.path(), &target, "com.b");

        assert!(record_launch_in(
            dir.path(),
            target.clone(),
            launch("emulator-5554", "com.a", false)
        ));
        assert!(record_launch_in(
            dir.path(),
            target,
            launch("emulator-5554", "com.a", true)
        ));
        QUEUE.flush();

        let recorded = events(dir.path(), &session.id);
        assert_eq!(
            kinds(dir.path(), &session.id),
            ["install", "launch", "launch"]
        );
        let DebugSessionEventData::Launch(first) = &recorded[1].event else {
            panic!("{recorded:?}");
        };
        let timing = first.timing.as_ref().expect("timing");
        assert_eq!(timing.total_ms, 240);
        assert_eq!(timing.avd_name.as_deref(), Some("Pixel_7"));
        assert!(!first.restart);
        assert_eq!(recorded[1].actor, Some(agent(false)));
        let DebugSessionEventData::Launch(restart) = &recorded[2].event else {
            panic!("{recorded:?}");
        };
        assert!(restart.restart);
        assert_eq!(
            restart.timing.as_ref().and_then(|t| t.displayed_ms),
            Some(512)
        );
        assert_eq!(summary(dir.path(), &session.id).counts.launches, 2);
        assert_eq!(kinds(dir.path(), &other_package.id), ["install"]);
    }

    #[test]
    fn late_display_times_are_added_to_the_session_of_the_launch() {
        let dir = TempDir::new().unwrap();
        let target = emulator("emulator-5556", "Late_AVD");
        let session = open(dir.path(), &target, "com.a");
        let other_package = open(dir.path(), &target, "com.b");
        // Another serial of the same AVD: the device is matched by its name.
        let mut timing = launch("emulator-5558", "com.a", false).timing.unwrap();
        timing.avd_name = Some("Late_AVD".into());
        timing.fully_drawn_ms = Some(1_400);

        assert!(record_late_launch_timing_in(
            dir.path(),
            "com.a",
            timing.clone(),
            BuildActor::App
        ));
        QUEUE.flush();

        let recorded = events(dir.path(), &session.id);
        assert_eq!(kinds(dir.path(), &session.id), ["install", "launchTiming"]);
        assert_eq!(
            recorded[1].event,
            DebugSessionEventData::LaunchTiming(timing)
        );
        assert_eq!(summary(dir.path(), &session.id).counts.launches, 0);
        assert_eq!(kinds(dir.path(), &other_package.id), ["install"]);
    }

    #[test]
    fn logcat_changes_are_recorded_on_the_sessions_of_the_streamed_device() {
        let dir = TempDir::new().unwrap();
        let target = emulator("emulator-5600", "Logcat_AVD");
        let streamed = open(dir.path(), &target, "com.a");
        let elsewhere = open(dir.path(), &phone("R5CT"), "com.a");
        remember_device(&target);

        record_logcat_in(dir.path(), Some("emulator-5600"), LogcatChange::Reconnect);
        record_logcat_in(
            dir.path(),
            Some("emulator-5600"),
            LogcatChange::Stopped("gave up".into()),
        );
        record_logcat_in(dir.path(), Some("emulator-5600"), LogcatChange::Cleared);
        assert!(!record_logcat_in(dir.path(), None, LogcatChange::Cleared));
        QUEUE.flush();

        assert_eq!(
            kinds(dir.path(), &streamed.id),
            [
                "install",
                "logcatReconnect",
                "logcatStopped",
                "logcatCleared"
            ]
        );
        assert_eq!(kinds(dir.path(), &elsewhere.id), ["install"]);
    }

    #[tokio::test]
    async fn clearing_logcat_records_it_on_the_streamed_devices_session() {
        // The unit-test data directory, which the hook writes to.
        let target = phone("CLEARTEST01");
        let session = open(&data_dir(), &target, "com.cleartest");
        let state = crate::commands::logcat::new_logcat_state();
        state.lock().await.device_serial = Some("CLEARTEST01".into());

        crate::services::logcat::request_clear(&state, None).await;
        QUEUE.flush();

        assert_eq!(
            kinds(&data_dir(), &session.id),
            ["install", "logcatCleared"]
        );
    }

    fn device(serial: &str, avd: Option<&str>, state: DeviceConnectionState) -> Device {
        Device {
            serial: serial.into(),
            name: serial.into(),
            model: None,
            device_kind: if serial.starts_with("emulator-") {
                DeviceKind::Emulator
            } else {
                DeviceKind::Physical
            },
            connection_state: state,
            api_level: None,
            android_version: None,
            avd_name: avd.map(str::to_owned),
        }
    }

    #[test]
    fn the_device_poll_records_devices_going_offline_and_coming_back() {
        let dir = TempDir::new().unwrap();
        let phone_session = open(dir.path(), &phone("R5CT"), "com.a");
        let avd_session = open(dir.path(), &emulator("emulator-5554", "Pixel_7"), "com.a");
        let presence = DevicePresence::default();
        let online = DeviceConnectionState::Online;
        let both = [
            device("R5CT", None, online.clone()),
            device("emulator-5554", Some("Pixel_7"), online.clone()),
        ];

        // The first list is the baseline.
        presence.observe_in(dir.path(), &both);
        presence.observe_in(
            dir.path(),
            &[
                device("R5CT", None, DeviceConnectionState::Offline),
                device("emulator-5554", Some("Pixel_7"), online.clone()),
            ],
        );
        presence.observe_in(dir.path(), &both);
        // The emulator restarts: gone, then online before its AVD answers.
        presence.observe_in(dir.path(), &both[..1]);
        presence.observe_in(
            dir.path(),
            &[
                both[0].clone(),
                device("emulator-5554", None, online.clone()),
            ],
        );
        presence.observe_in(dir.path(), &both);
        QUEUE.flush();

        assert_eq!(
            kinds(dir.path(), &phone_session.id),
            ["install", "deviceOffline", "deviceOnline"]
        );
        assert_eq!(
            kinds(dir.path(), &avd_session.id),
            ["install", "deviceOffline", "deviceOnline"]
        );
    }

    #[test]
    fn a_bookmark_goes_to_the_named_or_the_selected_devices_newest_session() {
        let dir = TempDir::new().unwrap();
        let first = open(dir.path(), &phone("R5CT"), "com.a");
        let second = open(dir.path(), &phone("R5CT"), "com.b");
        let other = open(dir.path(), &phone("ZX1G"), "com.a");
        let now = Utc::now();

        let named =
            add_bookmark_in(dir.path(), Some(&first.id), None, "here", Some(42), now).unwrap();
        add_bookmark_in(
            dir.path(),
            None,
            Some(&phone("R5CT")),
            "selected",
            None,
            now,
        )
        .unwrap();
        add_bookmark_in(dir.path(), None, None, "newest", None, now).unwrap();

        assert_eq!(named.seq, 2);
        assert_eq!(
            named.event,
            DebugSessionEventData::Bookmark(DebugSessionBookmark {
                note: "here".into(),
                log_entry_id: Some(42),
            })
        );
        assert_eq!(kinds(dir.path(), &first.id), ["install", "bookmark"]);
        assert_eq!(kinds(dir.path(), &second.id), ["install", "bookmark"]);
        assert_eq!(kinds(dir.path(), &other.id), ["install", "bookmark"]);
        assert_eq!(summary(dir.path(), &first.id).counts.bookmarks, 1);
        assert!(matches!(
            add_bookmark_in(dir.path(), None, Some(&phone("NOPE")), "x", None, now),
            Err(AppError::NotFound(_))
        ));
        end_session_in(dir.path(), &first.id, RETAIN, now).unwrap();
        assert!(matches!(
            add_bookmark_in(dir.path(), Some(&first.id), None, "x", None, now),
            Err(AppError::InvalidInput(_))
        ));
    }

    // ── Failures never reach the caller ──────────────────────────────────────

    #[test]
    fn a_session_that_cannot_be_written_does_not_fail_the_install() {
        let dir = TempDir::new().unwrap();
        // A file where the sessions folder should be.
        std::fs::write(dir.path().join(SESSIONS_DIR), "").unwrap();
        let target = phone("R5CT");
        let entry = installed(&target, "com.a", &"a".repeat(64), None, vec![]);

        assert!(
            open_for_install_in(dir.path(), &target, &entry, BuildActor::App, RETAIN).is_none()
        );
    }

    #[test]
    fn a_hook_whose_write_fails_returns_and_the_writer_carries_on() {
        // A data directory whose index cannot be replaced.
        let failing = TempDir::new().unwrap();
        open(failing.path(), &phone("R5CT"), "com.a");
        let index = sessions_dir(failing.path()).join(INDEX_FILE);
        std::fs::remove_file(&index).unwrap();
        std::fs::create_dir(&index).unwrap();
        let dir = TempDir::new().unwrap();
        let session = open(dir.path(), &phone("R5CT"), "com.a");

        assert!(record_for_device_in(
            failing.path(),
            &phone("R5CT"),
            None,
            None,
            DebugSessionEventData::LogcatCleared(DebugSessionLogcatChange {
                serial: "R5CT".into(),
                reason: None,
            }),
            Utc::now(),
        )
        .is_err());
        assert!(record_logcat_in(
            failing.path(),
            Some("R5CT"),
            LogcatChange::Cleared
        ));
        record_logcat_in(dir.path(), Some("R5CT"), LogcatChange::Cleared);
        QUEUE.flush();

        assert_eq!(kinds(dir.path(), &session.id), ["install", "logcatCleared"]);
    }

    #[test]
    fn the_queue_drops_and_counts_events_when_full() {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = StdMutex::new(gate);
        let queue = EventQueue::start(2, move |_| {
            let _ = started_tx.send(());
            let _ = gate.lock().unwrap().recv();
        });
        let event = || PendingEvent {
            data_dir: PathBuf::from("/nowhere"),
            target: phone("R5CT"),
            package: None,
            actor: None,
            event: DebugSessionEventData::LogcatCleared(DebugSessionLogcatChange {
                serial: "R5CT".into(),
                reason: None,
            }),
        };

        assert!(queue.push(event()));
        started.recv_timeout(Duration::from_secs(30)).unwrap();
        assert!(queue.push(event()));
        assert!(queue.push(event()));
        assert!(!queue.push(event()), "a full queue drops");
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1);

        for _ in 0..3 {
            release.send(()).unwrap();
        }
        queue.flush();
    }
}
