//! Crashes and ANRs on debug sessions: the log lines around each, the install
//! it belongs to, and the process exits Android recorded for it.
//!
//! The logcat store hands the first entry of each new crash group
//! ([`CrashSeen`]) to a bounded queue ([`MAX_PENDING_CRASHES`]) and returns.
//! One writer thread waits [`CRASH_SETTLE`] for the group to finish, copies a
//! bounded window of the buffer, writes it to a temporary file without the
//! data lock, and asks the device whether the session's install is still the
//! app it runs. Then, under the data lock, it drops a crash another process
//! already recorded, publishes the capture, and appends the event. About
//! [`EXIT_READ_DELAY`] later it reads the app's exit reasons, once per session
//! however many crashes arrive meanwhile.

use super::*;
use crate::models::app_exit::AppExitRecord;
use crate::models::logcat::{EntryFlags, ProcessedEntry};
use crate::services::app_exit_info;
use crate::services::logcat::{EntryDevice, LogcatState};
use crate::services::{adb_manager, retrace};
use crate::utils::validation::{validate_device_serial, validate_package_name};
use chrono::NaiveDateTime;
use std::collections::HashSet;
use std::io::{Seek, SeekFrom};
use std::time::Instant;

// ── Caps ──────────────────────────────────────────────────────────────────────

/// Crashes waiting for the capture writer; more are dropped and counted.
pub const MAX_PENDING_CRASHES: usize = 64;
/// Most log lines kept with one crash.
pub const MAX_CAPTURE_ENTRIES: usize = 1_000;
/// Largest capture of one crash.
pub const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;
/// Most crash captures in one session; later crashes get an event only.
pub const MAX_CAPTURES_PER_SESSION: u32 = 10;
/// Lines before the crash kept with it.
pub const CAPTURE_CONTEXT_BEFORE: usize = 500;
/// Buffer entries searched from a crash's first line for the rest of its group.
const CAPTURE_SCAN_ENTRIES: usize = 10_000;
/// Entries after an ANR line searched for ActivityManager's `PID:` line.
const ANR_PID_LOOKAHEAD: usize = 10;
/// Most crash and ANR events `get_debug_session` returns (the newest).
pub const MAX_CRASHES_RETURNED: usize = 100;
/// Sessions waiting for their exit reasons to be read; more are skipped.
pub const MAX_PENDING_EXIT_READS: usize = 16;
/// The same crash (device, package, pid, first line) seen within this many
/// seconds, by another process or after a reconnect, is recorded once.
const CRASH_DEDUPE_SECS: i64 = 5;
/// How much of the end of a session's event log is searched for a duplicate.
const DEDUPE_TAIL_BYTES: u64 = 64 * 1024;
/// A session without a Keynobi install opens at its first crash's host time;
/// the device clock is read to the second, so its exits may be this early.
const UNATTRIBUTED_EXIT_TOLERANCE_SECS: i64 = 2;
/// How long a crash group may keep growing before it is captured.
#[cfg(not(test))]
pub const CRASH_SETTLE: Duration = Duration::from_secs(2);
#[cfg(test)]
pub const CRASH_SETTLE: Duration = Duration::from_millis(50);
/// How long after a crash its exit reason is read (Android records it once
/// the process is gone).
#[cfg(not(test))]
pub const EXIT_READ_DELAY: Duration = Duration::from_secs(5);
#[cfg(test)]
pub const EXIT_READ_DELAY: Duration = Duration::from_millis(50);

const CAPTURES_DIR: &str = "captures";
/// Temporary captures are written as `sessions/capture.jsonl.<pid>.<n>.tmp`.
pub(super) const CAPTURE_TMP: &str = "capture.jsonl";

fn capture_file(seq: u32) -> String {
    format!("crash-{seq}.jsonl")
}

// ── The hook ──────────────────────────────────────────────────────────────────

/// The first entry of a crash group the logcat store had not seen.
#[derive(Debug, Clone)]
pub struct CrashSeen {
    pub crash_group_id: u64,
    pub first_entry_id: u64,
    pub pid: i32,
    pub package: Option<String>,
    pub anr: bool,
    pub device: EntryDevice,
    pub received_at: DateTime<Utc>,
    seen: Instant,
}

impl CrashSeen {
    pub fn new(first: &ProcessedEntry, device: EntryDevice) -> Self {
        CrashSeen {
            crash_group_id: first.crash_group_id.unwrap_or_default(),
            first_entry_id: first.id,
            pid: first.pid,
            package: first.package.clone(),
            anr: first.flags & EntryFlags::ANR != 0,
            device,
            received_at: Utc::now(),
            seen: Instant::now(),
        }
    }
}

/// Where a logcat stream's crashes come from: its buffer, the adb it runs,
/// and the kind of process reading it.
#[derive(Clone)]
pub struct CrashSource {
    logcat: LogcatState,
    adb: PathBuf,
    recorded_by: DebugSessionRecorder,
}

impl CrashSource {
    /// `in_app` when the stream runs in the app (a GUI or attached MCP
    /// stream), not in a standalone MCP server.
    pub fn new(logcat: LogcatState, adb: PathBuf, in_app: bool) -> Self {
        CrashSource {
            logcat,
            adb,
            recorded_by: if in_app {
                DebugSessionRecorder::App
            } else {
                DebugSessionRecorder::Standalone
            },
        }
    }

    /// Queue `crash` for its debug session. Never waits: a full queue drops
    /// it and counts it.
    pub fn record(&self, crash: CrashSeen) -> bool {
        CRASH_QUEUE.push(CrashWork {
            seen: crash,
            source: self.clone(),
            data_dir: data_dir(),
        })
    }
}

struct CrashWork {
    seen: CrashSeen,
    source: CrashSource,
    data_dir: PathBuf,
}

enum CrashJob {
    Crash(Box<CrashWork>),
    /// Answered once everything queued before it was handled.
    #[cfg(test)]
    Flush(tokio::sync::oneshot::Sender<()>),
}

/// A bounded queue handled in order by one thread with its own runtime, so
/// the logcat pipeline never waits for a device or the disk.
struct CrashQueue {
    tx: tokio::sync::mpsc::Sender<CrashJob>,
    dropped: AtomicU64,
}

impl CrashQueue {
    fn start<F, Fut>(capacity: usize, handle: F) -> Self
    where
        F: Fn(CrashWork) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()>,
    {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<CrashJob>(capacity);
        let spawned = std::thread::Builder::new()
            .name("debug-session-crashes".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        tracing::warn!("Crashes will not be added to debug sessions: {e}");
                        return;
                    }
                };
                runtime.block_on(async move {
                    while let Some(job) = rx.recv().await {
                        match job {
                            CrashJob::Crash(work) => handle(*work).await,
                            #[cfg(test)]
                            CrashJob::Flush(done) => {
                                let _ = done.send(());
                            }
                        }
                    }
                });
            });
        if let Err(e) = spawned {
            // The receiver is gone with the closure, so every push is dropped.
            tracing::warn!("Crashes will not be added to debug sessions: {e}");
        }
        CrashQueue {
            tx,
            dropped: AtomicU64::new(0),
        }
    }

    fn push(&self, work: CrashWork) -> bool {
        match self.tx.try_send(CrashJob::Crash(Box::new(work))) {
            Ok(()) => true,
            Err(_) => {
                let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if dropped == 1 || dropped.is_multiple_of(100) {
                    tracing::warn!(
                        "{dropped} crashes not added to debug sessions: more than \
                         {MAX_PENDING_CRASHES} waiting to be captured"
                    );
                }
                false
            }
        }
    }

    #[cfg(test)]
    fn flush(&self) {
        let (done, wait) = tokio::sync::oneshot::channel();
        self.tx
            .try_send(CrashJob::Flush(done))
            .expect("crash queue has room");
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut wait = wait;
        loop {
            match wait.try_recv() {
                Ok(()) => return,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "crash queue flushed");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("crash queue stopped: {e}"),
            }
        }
    }
}

static CRASH_QUEUE: LazyLock<CrashQueue> =
    LazyLock::new(|| CrashQueue::start(MAX_PENDING_CRASHES, handle_crash));

/// Crashes not added to a debug session because the queue was full, in this
/// process.
pub fn dropped_crashes() -> u64 {
    CRASH_QUEUE.dropped.load(Ordering::Relaxed)
}

async fn handle_crash(work: CrashWork) {
    tokio::time::sleep_until(tokio::time::Instant::from_std(
        work.seen.seen + CRASH_SETTLE,
    ))
    .await;
    let gid = work.seen.crash_group_id;
    match record_crash_in(work).await {
        Ok(Some(_)) => {}
        Ok(None) => tracing::debug!("Crash group {gid} was already recorded"),
        Err(e) => tracing::warn!("Crash group {gid} not added to a debug session: {e}"),
    }
}

// ── The log window ────────────────────────────────────────────────────────────

/// The log lines kept with a crash, bounded, as JSON lines.
#[derive(Debug)]
struct CrashWindow {
    /// The crash group's lines that were kept, oldest first.
    group: Vec<ProcessedEntry>,
    /// Oldest first: the lines before the crash, then the group.
    lines: Vec<String>,
    bytes: u64,
    truncated: bool,
    dropped_lines: u64,
    /// ActivityManager's `PID:` line after an ANR.
    anr_pid: Option<u32>,
}

/// Copy the crash's lines from the logcat buffer, or `None` when its first
/// line is no longer there (cleared). The lock is held only to copy.
async fn copy_window(logcat: &LogcatState, seen: &CrashSeen) -> Option<CrashWindow> {
    let (before, group, after, dropped_lines) = {
        let state = logcat.lock().await;
        let group = state.store.crash_group_from(
            seen.first_entry_id,
            seen.crash_group_id,
            CAPTURE_SCAN_ENTRIES,
            MAX_CAPTURE_ENTRIES + 1,
        );
        if group.is_empty() {
            return None;
        }
        let before = state
            .store
            .context_before(seen.first_entry_id, CAPTURE_CONTEXT_BEFORE);
        let after = if seen.anr {
            state
                .store
                .context_after(seen.first_entry_id, ANR_PID_LOOKAHEAD)
        } else {
            Vec::new()
        };
        (before, group, after, state.store.stats.dropped_lines)
    };
    let anr_pid = after.iter().find_map(|e| {
        (e.tag == "ActivityManager")
            .then(|| e.message.strip_prefix("PID: "))
            .flatten()
            .and_then(|pid| pid.trim().parse().ok())
    });
    Some(bound_window(before, group, dropped_lines, anr_pid))
}

/// Keep the crash group first, then the lines before it (nearest first),
/// within [`MAX_CAPTURE_ENTRIES`] and [`MAX_CAPTURE_BYTES`].
fn bound_window(
    before: Vec<ProcessedEntry>,
    group: Vec<ProcessedEntry>,
    dropped_lines: u64,
    anr_pid: Option<u32>,
) -> CrashWindow {
    let mut truncated = false;
    let mut bytes: u64 = 0;
    let mut fits = |line: &str, count: usize| {
        let size = line.len() as u64 + 1;
        let ok = count < MAX_CAPTURE_ENTRIES && bytes + size <= MAX_CAPTURE_BYTES;
        if ok {
            bytes += size;
        }
        ok
    };
    let mut kept_group = Vec::new();
    let mut group_lines = Vec::new();
    for entry in group {
        let Ok(line) = serde_json::to_string(&entry) else {
            continue;
        };
        if !fits(&line, group_lines.len()) {
            truncated = true;
            break;
        }
        group_lines.push(line);
        kept_group.push(entry);
    }
    let mut before_lines = Vec::new();
    for entry in before.iter().rev() {
        let Ok(line) = serde_json::to_string(entry) else {
            continue;
        };
        if !fits(&line, group_lines.len() + before_lines.len()) {
            truncated = true;
            break;
        }
        before_lines.push(line);
    }
    before_lines.reverse();
    before_lines.extend(group_lines);
    CrashWindow {
        group: kept_group,
        lines: before_lines,
        bytes,
        truncated,
        dropped_lines,
        anr_pid,
    }
}

/// The package that crashed: for an ANR, the process ActivityManager names;
/// otherwise the logcat entry's package, else AndroidRuntime's `Process:`
/// line. A process name (`com.example:remote`) gives its package.
fn crash_package(seen: &CrashSeen, group: &[ProcessedEntry]) -> Option<String> {
    let process = if seen.anr {
        group.first().and_then(|e| {
            let rest = e.message.split_once("ANR in ")?.1;
            rest.split_whitespace().next().map(str::to_owned)
        })
    } else {
        seen.package.clone().or_else(|| {
            group.iter().find_map(|e| {
                let rest = e.message.strip_prefix("Process: ")?;
                rest.split(',').next().map(|p| p.trim().to_owned())
            })
        })
    }?;
    let package = process.split(':').next()?.to_string();
    validate_package_name(&package).ok()?;
    Some(package)
}

/// The line that says what happened: the exception after AndroidRuntime's
/// header lines, or the ANR line.
fn crash_summary(anr: bool, group: &[ProcessedEntry]) -> String {
    let header = |m: &str| {
        m.starts_with("FATAL EXCEPTION")
            || m.starts_with("Process: ")
            || m.starts_with("Uncaught handler")
            || m.starts_with('\t')
            || m.trim().is_empty()
    };
    let line = if anr {
        group.first()
    } else {
        group
            .iter()
            .find(|e| !header(&e.message))
            .or_else(|| group.first())
    };
    truncate_chars(
        line.map(|e| e.message.trim()).unwrap_or_default(),
        MAX_EVENT_TEXT_CHARS,
    )
}

fn signature_of(first_line: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(first_line.as_bytes())[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A capture written to a temporary file in the sessions folder, without the
/// data lock. Removed when dropped unless it was published.
struct PreparedCapture {
    tmp: Option<PathBuf>,
    entries: u32,
    bytes: u64,
    truncated: bool,
}

impl PreparedCapture {
    fn write(data_dir: &Path, window: &CrashWindow) -> Result<Self, String> {
        let dir = sessions_dir(data_dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        let tmp = unique_tmp_path(&dir.join(CAPTURE_TMP));
        let mut prepared = PreparedCapture {
            tmp: Some(tmp.clone()),
            entries: u32::try_from(window.lines.len()).unwrap_or(u32::MAX),
            bytes: window.bytes,
            truncated: window.truncated,
        };
        let mut text = String::with_capacity(window.bytes as usize);
        for line in &window.lines {
            text.push_str(line);
            text.push('\n');
        }
        if let Err(e) = std::fs::write(&tmp, text) {
            prepared.tmp = None;
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("Cannot write a crash capture: {e}"));
        }
        Ok(prepared)
    }

    /// Move the capture into `session`'s folder as the capture of event
    /// `seq`. Callers hold the data lock.
    fn publish(&mut self, data_dir: &Path, session: &str, seq: u32) -> Result<(), String> {
        let tmp = self.tmp.as_ref().ok_or("The capture was not written")?;
        let dir = session_dir(data_dir, session).join(CAPTURES_DIR);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        std::fs::rename(tmp, dir.join(capture_file(seq)))
            .map_err(|e| format!("Cannot publish a crash capture: {e}"))?;
        self.tmp = None;
        Ok(())
    }

    fn reference(&self) -> DebugSessionCaptureRef {
        DebugSessionCaptureRef {
            entries: self.entries,
            bytes: self.bytes,
            truncated: self.truncated,
        }
    }
}

impl Drop for PreparedCapture {
    fn drop(&mut self) {
        if let Some(tmp) = self.tmp.take() {
            let _ = std::fs::remove_file(tmp);
        }
    }
}

// ── Attribution ───────────────────────────────────────────────────────────────

/// The device a crash was read from. An emulator is named by its AVD, asked
/// now (else the one this process last saw on the serial).
async fn crash_target(adb: &Path, device: &EntryDevice) -> Result<InstallTarget, String> {
    let serial = match device {
        EntryDevice::Serial(serial) => serial.clone(),
        EntryDevice::Unnamed => retrace::only_online_device(adb).await?,
        EntryDevice::Unknown => {
            return Err("the stream it came from is no longer remembered".into());
        }
    };
    validate_device_serial(&serial)?;
    let avd_name = if serial.starts_with("emulator-") {
        match adb_manager::resolve_avd_name(adb, &serial).await {
            Some(avd) => Some(avd),
            None => KNOWN_EMULATORS.avd_of(&serial),
        }
    } else {
        None
    };
    let target = InstallTarget {
        serial,
        avd_name,
        model: None,
    };
    remember_device(&target);
    Ok(target)
}

fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Whether `summary` was open at `at`.
fn open_at(summary: &DebugSessionSummary, at: DateTime<Utc>) -> bool {
    let opened = parse_time(&summary.opened_at).is_some_and(|o| o <= at);
    let not_closed = match summary.closed_at.as_deref() {
        None => idle_closed_at(&summary.last_event_at, at).is_none(),
        Some(closed) => parse_time(closed).is_some_and(|c| at < c),
    };
    opened && not_closed
}

fn is_session_of(summary: &DebugSessionSummary, target: &InstallTarget, package: &str) -> bool {
    summary.package == package
        && target.is_same_device(&summary.device.serial, summary.device.avd_name.as_deref())
}

/// Where a crash goes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Destination {
    /// The session of Keynobi's install.
    Session(String),
    /// The open session without a build on the device and package, created
    /// when there is none.
    Unattributed,
}

/// The session of Keynobi's install of `package` on `target` open at `at`,
/// and whether the device confirms that install is what crashed.
async fn attribute(
    data_dir: &Path,
    adb: &Path,
    target: &InstallTarget,
    package: &str,
    at: DateTime<Utc>,
) -> (Destination, DebugSessionAttribution) {
    let device = target
        .avd_name
        .clone()
        .unwrap_or_else(|| target.serial.clone());
    let unattributed = |reason: String| {
        (
            Destination::Unattributed,
            DebugSessionAttribution {
                method: DebugSessionAttributionMethod::Unattributed,
                verified: false,
                reason: Some(reason),
            },
        )
    };
    let session = load_index_from(data_dir)
        .into_iter()
        .rev()
        .find(|s| s.apk_sha256.is_some() && is_session_of(s, target, package) && open_at(s, at))
        .and_then(|s| read_manifest(data_dir, &s.id).ok());
    let Some((session, install)) = session.and_then(|s| {
        let install = s.install.clone()?;
        Some((s, install))
    }) else {
        return unattributed(format!(
            "Keynobi has no install of {package} on {device} open when it crashed"
        ));
    };
    let destination = Destination::Session(session.id.clone());
    let installed_record = |verified: bool, reason: String| {
        (
            destination.clone(),
            DebugSessionAttribution {
                method: DebugSessionAttributionMethod::InstallRecord,
                verified,
                reason: Some(reason),
            },
        )
    };
    if session.close_reason == Some(DebugSessionCloseReason::Superseded) {
        return installed_record(
            false,
            "a later install replaced it before the device could be asked".into(),
        );
    }
    let installed = InstalledBuild {
        serial: target.serial.clone(),
        avd_name: target.avd_name.clone(),
        model: None,
        package: package.to_string(),
        apk_sha256: install.apk_sha256.clone(),
        build_id: session.build.as_ref().map(|b| b.id),
        version_code: install.version_code,
        mappings: Vec::new(),
        installed_at: install.installed_at.clone(),
    };
    match installed_builds::verify_install_on_device(adb, &target.serial, &device, &installed).await
    {
        Ok(confirmed) => installed_record(true, format!("confirmed by the device: {confirmed}")),
        Err(installed_builds::InstallMismatch::Reinstalled(why)) => unattributed(why),
        Err(other) => installed_record(false, other.into_message()),
    }
}

// ── Recording ─────────────────────────────────────────────────────────────────

/// A crash ready to be written.
struct CrashRecord {
    target: InstallTarget,
    package: String,
    anr: bool,
    crash: DebugSessionCrash,
    recorded_by: DebugSessionRecorder,
}

/// Capture, attribute, and record one crash. `None` when another process (or
/// a replayed line) already recorded it.
async fn record_crash_in(work: CrashWork) -> Result<Option<(String, u32)>, String> {
    let CrashWork {
        seen,
        source,
        data_dir,
    } = work;
    let window = copy_window(&source.logcat, &seen)
        .await
        .ok_or("its lines left the logcat buffer (cleared)")?;
    let package = crash_package(&seen, &window.group)
        .ok_or("logcat did not name its package, so its session is unknown")?;
    let capture = {
        let dir = data_dir.clone();
        tokio::task::spawn_blocking(move || {
            let capture = PreparedCapture::write(&dir, &window);
            (window, capture)
        })
        .await
        .map_err(|e| e.to_string())?
    };
    let (window, capture) = capture;
    let capture = capture
        .map_err(|e| tracing::warn!("Crash capture not kept: {e}"))
        .ok();
    let target = crash_target(&source.adb, &seen.device)
        .await
        .map_err(|e| format!("its device is unknown: {e}"))?;
    let (destination, attribution) =
        attribute(&data_dir, &source.adb, &target, &package, seen.received_at).await;

    let first_line = window
        .group
        .first()
        .map(|e| e.message.as_str())
        .unwrap_or_default();
    let pid = if seen.anr {
        window.anr_pid
    } else {
        u32::try_from(seen.pid).ok()
    };
    let record = CrashRecord {
        crash: DebugSessionCrash {
            serial: target.serial.clone(),
            pid,
            summary: crash_summary(seen.anr, &window.group),
            signature: signature_of(first_line),
            received_at: stamp(seen.received_at),
            device_time: window
                .group
                .first()
                .map(|e| e.timestamp.clone())
                .unwrap_or_default(),
            attribution,
            capture: None,
            dropped_lines: window.dropped_lines,
        },
        target,
        package,
        anr: seen.anr,
        recorded_by: source.recorded_by,
    };
    let serial = record.target.serial.clone();
    let retention = Retention::from_settings();
    let dir = data_dir.clone();
    let published = tokio::task::spawn_blocking(move || {
        publish_crash_in(&dir, destination, record, capture, retention, Utc::now())
    })
    .await
    .map_err(|e| e.to_string())??;
    if let Some((session, _)) = &published {
        schedule_exit_read(data_dir, session.clone(), source.adb, serial);
    }
    Ok(published)
}

/// Whether a session of the crash's device and package already holds this
/// crash: the same pid and first line within [`CRASH_DEDUPE_SECS`].
fn is_duplicate_locked(
    data_dir: &Path,
    index: &[DebugSessionSummary],
    record: &CrashRecord,
) -> bool {
    let Some(received) = parse_time(&record.crash.received_at) else {
        return false;
    };
    let window = chrono::Duration::seconds(CRASH_DEDUPE_SECS);
    index
        .iter()
        .filter(|s| is_session_of(s, &record.target, &record.package))
        .filter(|s| parse_time(&s.last_event_at).is_some_and(|last| last >= received - window))
        .any(|s| {
            recent_crashes(data_dir, &s.id).iter().any(|c| {
                c.pid == record.crash.pid
                    && c.signature == record.crash.signature
                    && parse_time(&c.received_at).is_some_and(|at| (at - received).abs() <= window)
            })
        })
}

/// The crash and ANR events in the last [`DEDUPE_TAIL_BYTES`] of a session's log.
fn recent_crashes(data_dir: &Path, id: &str) -> Vec<DebugSessionCrash> {
    let path = session_dir(data_dir, id).join(EVENTS_FILE);
    let Ok(mut file) = std::fs::File::open(&path) else {
        return Vec::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(DEDUPE_TAIL_BYTES);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    // Bytes: the tail may start inside a character (its line is skipped).
    let mut bytes = Vec::new();
    if file
        .take(DEDUPE_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Vec::new();
    }
    events_of_kinds(&String::from_utf8_lossy(&bytes), &["crash", "anr"])
        .into_iter()
        .filter_map(|event| match event.event {
            DebugSessionEventData::Crash(c) | DebugSessionEventData::Anr(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// The events of `kinds` in event log `text`, skipping other lines unparsed.
fn events_of_kinds(text: &str, kinds: &[&str]) -> Vec<DebugSessionEvent> {
    let needles: Vec<String> = kinds.iter().map(|k| format!("\"kind\":\"{k}\"")).collect();
    text.lines()
        .filter(|line| needles.iter().any(|n| line.contains(n.as_str())))
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The open session without a build on the crash's device and package, or a
/// new one opened at the crash. Callers hold the data lock.
fn unattributed_locked(
    data_dir: &Path,
    index: &mut Vec<DebugSessionSummary>,
    record: &CrashRecord,
    retention: Retention,
    now: DateTime<Utc>,
) -> Result<usize, String> {
    if let Some(i) = index.iter().rposition(|s| {
        s.apk_sha256.is_none()
            && is_session_of(s, &record.target, &record.package)
            && is_open(s, now)
    }) {
        return Ok(i);
    }
    let id = create_session_dir(data_dir, now)?;
    let session = DebugSession {
        schema_version: DEBUG_SESSION_SCHEMA_VERSION,
        id: id.clone(),
        project_root: None,
        package: record.package.clone(),
        device: DebugSessionDevice {
            serial: record.target.serial.clone(),
            avd_name: record.target.avd_name.clone(),
            model: record.target.model.clone(),
        },
        build: None,
        install: None,
        opened_at: record.crash.received_at.clone(),
        closed_at: None,
        close_reason: None,
        recorded_by: record.recorded_by,
        kept: false,
        counts: DebugSessionCounts::default(),
        last_event_at: stamp(now),
        event_count: 0,
        dropped_events: 0,
        bytes: 0,
    };
    save_manifest(data_dir, &session)?;
    index.push(DebugSessionSummary::from(&session));
    prune_locked(data_dir, index, retention, now, Some(&id));
    index
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| "The new session was pruned".to_string())
}

/// Record `record` on its session under the data lock, with its capture
/// while the session has fewer than [`MAX_CAPTURES_PER_SESSION`]. Returns the
/// session and the event's `seq`, or `None` for a duplicate or a full log.
fn publish_crash_in(
    data_dir: &Path,
    destination: Destination,
    mut record: CrashRecord,
    mut capture: Option<PreparedCapture>,
    retention: Retention,
    now: DateTime<Utc>,
) -> Result<Option<(String, u32)>, String> {
    with_data_lock_in(data_dir, || {
        let mut index = load_index_locked(data_dir, now);
        if is_duplicate_locked(data_dir, &index, &record) {
            return Ok(None);
        }
        let named = match &destination {
            Destination::Session(id) => index.iter().position(|s| &s.id == id),
            Destination::Unattributed => None,
        };
        let i = match named {
            Some(i) => i,
            None => unattributed_locked(data_dir, &mut index, &record, retention, now)?,
        };
        let mut session = read_manifest(data_dir, &index[i].id)?;
        let seq = session.event_count + 1;
        let mut published = false;
        if let Some(capture) = capture.as_mut() {
            let room = session.counts.captures < MAX_CAPTURES_PER_SESSION
                && session.event_count < MAX_EVENTS_PER_SESSION;
            if room {
                capture.publish(data_dir, &session.id, seq)?;
                record.crash.capture = Some(capture.reference());
                published = true;
            }
        }
        let event = if record.anr {
            DebugSessionEventData::Anr(record.crash)
        } else {
            DebugSessionEventData::Crash(record.crash)
        };
        let recorded = match append_locked(data_dir, &mut session, None, event, now) {
            Ok(Append::Recorded(event)) => Some((session.id.clone(), event.seq)),
            not_recorded => {
                if published {
                    let _ = std::fs::remove_file(
                        session_dir(data_dir, &session.id)
                            .join(CAPTURES_DIR)
                            .join(capture_file(seq)),
                    );
                }
                not_recorded?;
                None
            }
        };
        save_manifest(data_dir, &session)?;
        index[i] = DebugSessionSummary::from(&session);
        save_index(data_dir, &index)?;
        Ok(recorded)
    })?
}

// ── Reading a capture ─────────────────────────────────────────────────────────

/// The log lines kept with crash event `seq` of session `id`: the newest
/// `limit` (at most [`MAX_CAPTURE_ENTRIES`]), which end with the crash.
pub fn get_capture(
    id: &str,
    seq: u32,
    limit: Option<u32>,
) -> Result<DebugSessionCapture, AppError> {
    get_capture_in(&data_dir(), id, seq, limit)
}

pub(super) fn get_capture_in(
    data_dir: &Path,
    id: &str,
    seq: u32,
    limit: Option<u32>,
) -> Result<DebugSessionCapture, AppError> {
    checked_id(id)?;
    read_manifest(data_dir, id).map_err(|_| not_found(id))?;
    let path = session_dir(data_dir, id)
        .join(CAPTURES_DIR)
        .join(capture_file(seq));
    let file = std::fs::File::open(&path).map_err(|_| {
        AppError::NotFound(format!(
            "Debug session {id} kept no log lines for event {seq}"
        ))
    })?;
    let mut text = String::new();
    file.take(MAX_CAPTURE_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| AppError::io(path.display(), e))?;
    let mut entries: Vec<ProcessedEntry> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let limit = limit
        .map_or(MAX_CAPTURE_ENTRIES, |l| l as usize)
        .clamp(1, MAX_CAPTURE_ENTRIES);
    let truncated = entries.len() > limit;
    if truncated {
        entries.drain(..entries.len() - limit);
    }
    Ok(DebugSessionCapture {
        seq,
        entries,
        truncated,
    })
}

/// The crash and ANR events among `events`, the newest [`MAX_CRASHES_RETURNED`].
pub(super) fn crash_events(events: &[DebugSessionEvent]) -> Vec<DebugSessionEvent> {
    let crashes: Vec<DebugSessionEvent> = events
        .iter()
        .filter(|e| {
            matches!(
                e.event,
                DebugSessionEventData::Crash(_) | DebugSessionEventData::Anr(_)
            )
        })
        .cloned()
        .collect();
    let skip = crashes.len().saturating_sub(MAX_CRASHES_RETURNED);
    crashes.into_iter().skip(skip).collect()
}

// ── Exit reasons ──────────────────────────────────────────────────────────────

/// Sessions whose exit reasons are about to be read, at most
/// [`MAX_PENDING_EXIT_READS`].
#[derive(Default)]
struct ExitReads(StdMutex<HashSet<String>>);

impl ExitReads {
    /// Whether a read of `session` may be scheduled: none is waiting.
    fn begin(&self, session: &str) -> bool {
        let mut pending = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if pending.contains(session) || pending.len() >= MAX_PENDING_EXIT_READS {
            return false;
        }
        pending.insert(session.to_string())
    }

    fn end(&self, session: &str) {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(session);
    }
}

static EXIT_READS: LazyLock<ExitReads> = LazyLock::new(ExitReads::default);

/// Read `session`'s exit reasons [`EXIT_READ_DELAY`] from now, unless a read
/// is already waiting.
fn schedule_exit_read(data_dir: PathBuf, session: String, adb: PathBuf, serial: String) -> bool {
    if !EXIT_READS.begin(&session) {
        return false;
    }
    tokio::spawn(async move {
        tokio::time::sleep(EXIT_READ_DELAY).await;
        EXIT_READS.end(&session);
        if let Err(e) = read_exits_in(&data_dir, &session, &adb, &serial).await {
            tracing::debug!("Exit reasons of debug session {session} not read: {e}");
        }
    });
    true
}

/// Read the app's exit reasons from the session's device now and add the
/// ones that belong to it (`refresh_session_exit_reasons`).
pub async fn refresh_exit_reasons(
    id: &str,
    adb: &Path,
) -> Result<DebugSessionExitRefresh, AppError> {
    refresh_exit_reasons_in(&data_dir(), id, adb).await
}

async fn refresh_exit_reasons_in(
    data_dir: &Path,
    id: &str,
    adb: &Path,
) -> Result<DebugSessionExitRefresh, AppError> {
    checked_id(id)?;
    let session = read_manifest(data_dir, id).map_err(|_| not_found(id))?;
    let serial = session.device.serial.clone();
    validate_device_serial(&serial).map_err(AppError::InvalidInput)?;
    if let Some(avd) = &session.device.avd_name {
        if adb_manager::resolve_avd_name(adb, &serial).await.as_ref() != Some(avd) {
            return Err(AppError::ProcessFailed(format!(
                "{avd} is not running on {serial}; start it to read its exit reasons"
            )));
        }
    }
    read_exits_in(data_dir, id, adb, &serial).await
}

async fn read_exits_in(
    data_dir: &Path,
    id: &str,
    adb: &Path,
    serial: &str,
) -> Result<DebugSessionExitRefresh, AppError> {
    let package = read_manifest(data_dir, id)
        .map_err(|_| not_found(id))?
        .package;
    let reasons = app_exit_info::read_exit_reasons(adb, serial, None, Some(&package)).await?;
    if !reasons.supported {
        return Ok(DebugSessionExitRefresh {
            added: 0,
            message: reasons.message,
        });
    }
    let clock = installed_builds::device_clock(adb, serial)
        .await
        .map_err(AppError::ProcessFailed)?;
    let records: Vec<(DateTime<Utc>, AppExitRecord)> = reasons
        .records
        .into_iter()
        .filter_map(|record| {
            let local = NaiveDateTime::parse_from_str(
                record.timestamp_local.as_deref()?,
                "%Y-%m-%dT%H:%M:%S%.3f",
            )
            .ok()?;
            Some((clock.to_host_time(local)?, record))
        })
        .collect();
    let (dir, id, serial) = (data_dir.to_path_buf(), id.to_string(), serial.to_string());
    let added = tokio::task::spawn_blocking(move || {
        attribute_exits_in(&dir, &id, &serial, records, Utc::now())
    })
    .await
    .map_err(|e| AppError::Other(format!("Debug session task failed: {e}")))??;
    Ok(DebugSessionExitRefresh {
        added,
        message: None,
    })
}

/// When the session's app could have exited: from its install (or, without
/// one, a moment before its first crash) until it closed.
fn exit_window(session: &DebugSession) -> Option<(DateTime<Utc>, Option<DateTime<Utc>>)> {
    let start = match &session.install {
        Some(install) => parse_time(&install.installed_at)?,
        None => {
            parse_time(&session.opened_at)?
                - chrono::Duration::seconds(UNATTRIBUTED_EXIT_TOLERANCE_SECS)
        }
    };
    let end = match &session.closed_at {
        Some(closed) => Some(parse_time(closed)?),
        None => None,
    };
    Some((start, end))
}

/// Add the exits among `records` (with their host times) that fall within
/// session `id` and are not recorded yet. Returns how many were added.
fn attribute_exits_in(
    data_dir: &Path,
    id: &str,
    serial: &str,
    mut records: Vec<(DateTime<Utc>, AppExitRecord)>,
    now: DateTime<Utc>,
) -> Result<u32, AppError> {
    records.sort_by_key(|(at, _)| *at);
    with_data_lock_in(data_dir, || {
        let mut index = load_index_locked(data_dir, now);
        let i = index
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| not_found(id))?;
        let mut session = read_manifest(data_dir, id).map_err(AppError::Other)?;
        let Some((start, end)) = exit_window(&session) else {
            return Ok(0);
        };
        let path = session_dir(data_dir, id).join(EVENTS_FILE);
        let mut text = String::new();
        if let Ok(file) = std::fs::File::open(&path) {
            file.take(MAX_SESSION_BYTES)
                .read_to_string(&mut text)
                .map_err(|e| AppError::io(path.display(), e))?;
        }
        let mut pids = HashSet::new();
        let mut known = HashSet::new();
        for event in events_of_kinds(&text, &["crash", "anr", "exit"]) {
            match event.event {
                DebugSessionEventData::Crash(c) | DebugSessionEventData::Anr(c) => {
                    pids.extend(c.pid);
                }
                DebugSessionEventData::Exit(exit) => {
                    known.insert(exit_key(&exit.record));
                }
                _ => {}
            }
        }
        let mut added = 0;
        for (at, record) in records {
            if at < start || end.is_some_and(|end| at >= end) {
                continue;
            }
            if !known.insert(exit_key(&record)) {
                continue;
            }
            let matched_by = if record.pid.is_some_and(|pid| pids.contains(&pid)) {
                DebugSessionExitMatch::Pid
            } else if record.process_name.as_deref() == Some(session.package.as_str()) {
                DebugSessionExitMatch::ProcessName
            } else {
                DebugSessionExitMatch::TimeWindow
            };
            let event = DebugSessionEventData::Exit(DebugSessionExit {
                serial: serial.to_string(),
                exited_at: stamp(at),
                matched_by,
                record,
            });
            match append_locked(data_dir, &mut session, None, event, now)
                .map_err(AppError::Other)?
            {
                Append::Recorded(_) => added += 1,
                Append::Dropped(_) => break,
            }
        }
        if added > 0 {
            save_manifest(data_dir, &session).map_err(AppError::Other)?;
            index[i] = DebugSessionSummary::from(&session);
            save_index(data_dir, &index).map_err(AppError::Other)?;
        }
        Ok(added)
    })
    .map_err(AppError::Other)?
}

/// An exit's identity: when, which process, and why.
fn exit_key(record: &AppExitRecord) -> (Option<String>, Option<u32>, String) {
    (
        record.timestamp.clone(),
        record.pid,
        record.reason.name().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::app_exit::AppExitReason;
    use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel};
    use crate::services::logcat::LogcatStateInner;
    use crate::utils::process::test_support::run_once;
    use std::sync::Arc;
    use tempfile::TempDir;

    const PACKAGE: &str = "com.crash.app";
    const SERIAL: &str = "R5CTCRASH01";
    const RETAIN: Retention = Retention {
        days: 14,
        max_folder_mb: 200,
    };

    fn line(id: u64, gid: Option<u64>, tag: &str, message: &str) -> ProcessedEntry {
        ProcessedEntry {
            id,
            timestamp: "09-25 10:32:01.100".into(),
            pid: 4321,
            tid: 4321,
            level: LogcatLevel::Error,
            tag: tag.into(),
            message: message.into(),
            package: None,
            kind: LogcatKind::Normal,
            is_crash: gid.is_some(),
            flags: if gid.is_some() { EntryFlags::CRASH } else { 0 },
            category: EntryCategory::General,
            crash_group_id: gid,
            json_body: None,
        }
    }

    const JAVA_CRASH: [&str; 4] = [
        "FATAL EXCEPTION: main",
        "Process: com.crash.app, PID: 4321",
        "java.lang.RuntimeException: boom",
        "\tat a.a.onCreate(SourceFile:1)",
    ];

    /// A buffer with `before` ordinary lines, then a crash of [`PACKAGE`]
    /// (group 1, from entry `before + 1`), then one more line.
    fn crash_buffer(before: u64) -> LogcatState {
        let mut state = LogcatStateInner::new();
        for id in 1..=before {
            state
                .store
                .push(line(id, None, "App", &format!("line {id}")));
        }
        for (i, message) in JAVA_CRASH.iter().enumerate() {
            state.store.push(line(
                before + 1 + i as u64,
                Some(1),
                "AndroidRuntime",
                message,
            ));
        }
        state
            .store
            .push(line(before + 5, None, "App", "after the crash"));
        Arc::new(tokio::sync::Mutex::new(state))
    }

    fn seen(first_entry_id: u64, gid: u64) -> CrashSeen {
        CrashSeen::new(
            &line(first_entry_id, Some(gid), "AndroidRuntime", JAVA_CRASH[0]),
            EntryDevice::Serial(SERIAL.into()),
        )
    }

    fn work(dir: &Path, adb: &Path, logcat: LogcatState, seen: CrashSeen) -> CrashWork {
        CrashWork {
            seen,
            source: CrashSource::new(logcat, adb.to_path_buf(), true),
            data_dir: dir.to_path_buf(),
        }
    }

    fn phone() -> InstallTarget {
        InstallTarget {
            serial: SERIAL.into(),
            avd_name: None,
            model: None,
        }
    }

    fn installed_at() -> DateTime<Utc> {
        Utc::now() - chrono::Duration::minutes(10)
    }

    /// Keynobi's install of [`PACKAGE`] (versionCode 7) on [`SERIAL`] at `at`.
    fn open_install(dir: &Path, at: DateTime<Utc>) -> DebugSession {
        let entry = InstalledBuild {
            serial: SERIAL.into(),
            avd_name: None,
            model: None,
            package: PACKAGE.into(),
            apk_sha256: "a".repeat(64),
            build_id: None,
            version_code: Some(7),
            mappings: vec![],
            installed_at: at.to_rfc3339(),
        };
        open_in(dir, &phone(), &entry, BuildActor::App, RETAIN, Utc::now()).unwrap()
    }

    fn dumpsys(version_code: u32, last_update: DateTime<Utc>) -> String {
        format!(
            "Packages:\n  Package [{PACKAGE}] (1a2b):\n    versionCode={version_code} \
             minSdk=24\n    lastUpdateTime={}\n",
            last_update.format("%Y-%m-%d %H:%M:%S")
        )
    }

    /// A device answering `dumpsys package` with `dumpsys`, its clock in UTC,
    /// API 34, and `exit-info` with what `exits` holds. Calls are logged.
    struct FakeDevice {
        adb: PathBuf,
        calls: PathBuf,
        exits: PathBuf,
    }

    impl FakeDevice {
        fn new(dir: &Path, dumpsys: &str) -> Self {
            Self::with_script(dir, dumpsys, "")
        }

        fn with_script(dir: &Path, dumpsys: &str, extra: &str) -> Self {
            let answer = dir.join("dumpsys.txt");
            std::fs::write(&answer, dumpsys).unwrap();
            let calls = dir.join("calls.log");
            let exits = dir.join("exits.txt");
            std::fs::write(&exits, "").unwrap();
            let adb = dir.join("adb");
            std::fs::write(
                &adb,
                format!(
                    "#!/bin/sh\n[ $# -eq 0 ] && exit 0\necho \"$*\" >> '{calls}'\ncase \"$*\" in\n\
                     {extra}\
                     *exit-info*) cat '{exits}' ;;\n\
                     *'dumpsys package'*) cat '{answer}' ;;\n\
                     *getprop*) echo 34 ;;\n\
                     *date*) echo \"$(date -u +%s):+0000\" ;;\n\
                     esac\n",
                    calls = calls.display(),
                    exits = exits.display(),
                    answer = answer.display(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&adb, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            run_once(&adb);
            FakeDevice { adb, calls, exits }
        }

        fn calls(&self, containing: &str) -> usize {
            std::fs::read_to_string(&self.calls)
                .unwrap_or_default()
                .lines()
                .filter(|l| l.contains(containing))
                .count()
        }
    }

    fn crashes_of(dir: &Path, id: &str) -> Vec<DebugSessionCrash> {
        get_session_in(dir, id, Utc::now())
            .unwrap()
            .crashes
            .into_iter()
            .filter_map(|e| match e.event {
                DebugSessionEventData::Crash(c) | DebugSessionEventData::Anr(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    fn exits_of(dir: &Path, id: &str) -> Vec<DebugSessionExit> {
        get_session_in(dir, id, Utc::now())
            .unwrap()
            .events
            .into_iter()
            .filter_map(|e| match e.event {
                DebugSessionEventData::Exit(exit) => Some(exit),
                _ => None,
            })
            .collect()
    }

    // ── Capture ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_crash_is_captured_with_the_lines_before_it() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let device = FakeDevice::new(
            dir.path(),
            &dumpsys(7, installed_at() - chrono::Duration::seconds(2)),
        );

        let (id, seq) = record_crash_in(work(dir.path(), &device.adb, crash_buffer(3), seen(4, 1)))
            .await
            .unwrap()
            .expect("recorded");

        assert_eq!(id, session.id);
        let crash = &crashes_of(dir.path(), &id)[0];
        assert_eq!(crash.summary, "java.lang.RuntimeException: boom");
        assert_eq!(crash.pid, Some(4321));
        assert_eq!(crash.device_time, "09-25 10:32:01.100");
        let capture = crash.capture.clone().expect("captured");
        assert_eq!(capture.entries, 7);
        assert!(!capture.truncated);
        let lines = get_capture_in(dir.path(), &id, seq, None).unwrap();
        let messages: Vec<&str> = lines.entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages[..3], ["line 1", "line 2", "line 3"]);
        assert_eq!(messages[3..], JAVA_CRASH);
        // The newest lines when asked for fewer: the crash itself.
        let last = get_capture_in(dir.path(), &id, seq, Some(4)).unwrap();
        assert!(last.truncated);
        assert_eq!(last.entries[0].message, JAVA_CRASH[0]);
        let counts = summary_of(dir.path(), &id).counts;
        assert_eq!((counts.crashes, counts.captures), (1, 1));
        // No temporary capture is left behind.
        assert!(!std::fs::read_dir(sessions_dir(dir.path()))
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".tmp")));
        assert!(matches!(
            get_capture_in(dir.path(), &id, seq + 1, None),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            get_capture_in(dir.path(), "../x", seq, None),
            Err(AppError::InvalidInput(_))
        ));
    }

    fn summary_of(dir: &Path, id: &str) -> DebugSessionSummary {
        load_index_from(dir)
            .into_iter()
            .find(|s| s.id == id)
            .expect("indexed")
    }

    #[test]
    fn the_window_is_bounded_and_keeps_the_crash_first() {
        let before: Vec<ProcessedEntry> = (1..=CAPTURE_CONTEXT_BEFORE as u64)
            .map(|id| line(id, None, "App", &format!("before {id}")))
            .collect();
        let group: Vec<ProcessedEntry> = (0..900u64)
            .map(|i| {
                line(
                    1_000 + i,
                    Some(1),
                    "AndroidRuntime",
                    &format!("\tat frame {i}"),
                )
            })
            .collect();

        let window = bound_window(before.clone(), group, 0, None);
        assert_eq!(window.lines.len(), MAX_CAPTURE_ENTRIES);
        assert_eq!(window.group.len(), 900, "the whole crash is kept");
        assert!(window.truncated);
        // The 100 lines nearest the crash are the ones kept before it.
        assert!(
            window.lines[0].contains("before 401"),
            "{}",
            window.lines[0]
        );

        let big = "x".repeat(60 * 1024);
        let group: Vec<ProcessedEntry> = (0..40u64)
            .map(|i| line(1_000 + i, Some(1), "AndroidRuntime", &big))
            .collect();
        let window = bound_window(before, group, 7, None);
        assert!(window.bytes <= MAX_CAPTURE_BYTES);
        let written: u64 = window.lines.iter().map(|l| l.len() as u64 + 1).sum();
        assert_eq!(written, window.bytes);
        assert!(window.truncated);
        assert_eq!(window.dropped_lines, 7);
        assert!(window.group.len() < 40);
        assert!(window.lines.iter().all(|l| l.contains("xxxx")));
    }

    #[tokio::test]
    async fn at_most_the_capture_cap_is_kept_per_session() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let logcat = crash_buffer(0);

        for pid in 0..(MAX_CAPTURES_PER_SESSION + 2) {
            let window = copy_window(&logcat, &seen(1, 1)).await.unwrap();
            let capture = PreparedCapture::write(dir.path(), &window).unwrap();
            let mut record = crash_record(Utc::now());
            record.crash.pid = Some(pid);
            publish_crash_in(
                dir.path(),
                Destination::Session(session.id.clone()),
                record,
                Some(capture),
                RETAIN,
                Utc::now(),
            )
            .unwrap()
            .expect("recorded");
        }

        let crashes = crashes_of(dir.path(), &session.id);
        assert_eq!(crashes.len(), MAX_CAPTURES_PER_SESSION as usize + 2);
        assert!(crashes[..MAX_CAPTURES_PER_SESSION as usize]
            .iter()
            .all(|c| c.capture.is_some()));
        assert!(crashes[MAX_CAPTURES_PER_SESSION as usize..]
            .iter()
            .all(|c| c.capture.is_none()));
        let files = std::fs::read_dir(session_dir(dir.path(), &session.id).join(CAPTURES_DIR))
            .unwrap()
            .count();
        assert_eq!(files, MAX_CAPTURES_PER_SESSION as usize);
        let counts = summary_of(dir.path(), &session.id).counts;
        assert_eq!(counts.captures, MAX_CAPTURES_PER_SESSION);
        assert_eq!(counts.crashes, MAX_CAPTURES_PER_SESSION + 2);
        // The uncaptured ones left no temporary file.
        assert!(!std::fs::read_dir(sessions_dir(dir.path()))
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".tmp")));
    }

    fn crash_record(received: DateTime<Utc>) -> CrashRecord {
        CrashRecord {
            target: phone(),
            package: PACKAGE.into(),
            anr: false,
            crash: DebugSessionCrash {
                serial: SERIAL.into(),
                pid: Some(4321),
                summary: "java.lang.RuntimeException: boom".into(),
                signature: signature_of(JAVA_CRASH[0]),
                received_at: stamp(received),
                device_time: "09-25 10:32:01.100".into(),
                attribution: DebugSessionAttribution {
                    method: DebugSessionAttributionMethod::InstallRecord,
                    verified: true,
                    reason: None,
                },
                capture: None,
                dropped_lines: 0,
            },
            recorded_by: DebugSessionRecorder::App,
        }
    }

    // ── The hook ─────────────────────────────────────────────────────────────

    #[test]
    fn the_hook_never_waits_and_a_full_queue_drops_and_counts() {
        let (started_tx, started) = std::sync::mpsc::channel();
        let gate = Arc::new(tokio::sync::Notify::new());
        let release = gate.clone();
        let queue = CrashQueue::start(2, move |_| {
            let _ = started_tx.send(());
            let gate = gate.clone();
            async move { gate.notified().await }
        });
        let dir = TempDir::new().unwrap();
        let item = || work(dir.path(), Path::new("adb"), crash_buffer(0), seen(1, 1));

        assert!(queue.push(item()));
        started.recv_timeout(Duration::from_secs(30)).unwrap();
        assert!(queue.push(item()));
        assert!(queue.push(item()));
        let pushed_at = Instant::now();
        assert!(!queue.push(item()), "a full queue drops");
        assert!(
            pushed_at.elapsed() < Duration::from_secs(1),
            "and does not wait"
        );
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1);

        for _ in 0..3 {
            release.notify_one();
            std::thread::sleep(Duration::from_millis(20));
        }
        release.notify_waiters();
    }

    #[tokio::test]
    async fn a_crash_streamed_by_logcat_is_captured_once_and_its_exit_attributed() {
        // The unit-test data directory, which the queue writes to.
        let serial = "R5CTSTREAM1";
        let package = "com.crash.streamed";
        let fixtures = TempDir::new().unwrap();
        let target = InstallTarget {
            serial: serial.into(),
            avd_name: None,
            model: None,
        };
        let at = installed_at();
        let entry = InstalledBuild {
            serial: serial.into(),
            avd_name: None,
            model: None,
            package: package.into(),
            apk_sha256: "b".repeat(64),
            build_id: None,
            version_code: Some(7),
            mappings: vec![],
            installed_at: at.to_rfc3339(),
        };
        let session = open_in(
            &data_dir(),
            &target,
            &entry,
            BuildActor::App,
            RETAIN,
            Utc::now(),
        )
        .unwrap();
        let logcat_lines = [
            "09-25 10:32:01.000  4321  4321 I App: starting",
            "09-25 10:32:01.100  4321  4321 E AndroidRuntime: FATAL EXCEPTION: main",
            "09-25 10:32:01.100  4321  4321 E AndroidRuntime: Process: com.crash.streamed, PID: 4321",
            "09-25 10:32:01.100  4321  4321 E AndroidRuntime: java.lang.IllegalStateException: streamed",
            "09-25 10:32:01.100  4321  4321 E AndroidRuntime: \tat a.a.onCreate(SourceFile:1)",
            "09-25 10:32:02.000   999   999 I ActivityManager: Process com.crash.streamed (pid 4321) has died",
        ];
        let logcat = format!(
            "  *logcat*) printf '%s\\n' {} ; exec sleep 60 ;;\n  *'shell ps'*) printf 'PID NAME\\n4321 {package}\\n' ;;\n",
            logcat_lines
                .iter()
                .map(|l| format!("'{l}'"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let dumpsys = format!(
            "  Package [{package}] (1):\n    versionCode=7\n    lastUpdateTime={}\n",
            (at - chrono::Duration::seconds(2)).format("%Y-%m-%d %H:%M:%S")
        );
        let device = FakeDevice::with_script(fixtures.path(), &dumpsys, &logcat);
        std::fs::write(
            &device.exits,
            format!(
                "  package: {package}\n    ApplicationExitInfo #0:\n      timestamp={} pid=4321 \
                 realUid=10152\n      process={package} reason=4 (APP CRASH(EXCEPTION)) \
                 subreason=0 (UNKNOWN) status=0\n",
                (Utc::now() + chrono::Duration::seconds(1)).format("%Y-%m-%d %H:%M:%S%.3f")
            ),
        )
        .unwrap();
        let state = crate::commands::logcat::new_logcat_state();

        crate::services::logcat::request_start(
            &state,
            device.adb.clone(),
            Some(serial.into()),
            None,
        )
        .await
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while summary_of(&data_dir(), &session.id).counts.exits == 0 {
            assert!(Instant::now() < deadline, "no crash and exit recorded");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        crate::services::logcat::request_stop(&state).await;
        tokio::task::spawn_blocking(|| CRASH_QUEUE.flush())
            .await
            .unwrap();

        let counts = summary_of(&data_dir(), &session.id).counts;
        assert_eq!((counts.crashes, counts.captures, counts.exits), (1, 1, 1));
        let crash = &crashes_of(&data_dir(), &session.id)[0];
        assert_eq!(crash.summary, "java.lang.IllegalStateException: streamed");
        assert!(crash.attribution.verified, "{:?}", crash.attribution);
        let seq = get_session_in(&data_dir(), &session.id, Utc::now())
            .unwrap()
            .crashes[0]
            .seq;
        let captured: Vec<String> = get_capture_in(&data_dir(), &session.id, seq, None)
            .unwrap()
            .entries
            .into_iter()
            .map(|e| e.message)
            .collect();
        // The line before the crash, then its group (which takes the line
        // that ends it, as the pipeline groups them).
        assert_eq!(captured[0], "starting");
        assert_eq!(captured[1], "FATAL EXCEPTION: main");
        assert_eq!(captured.len(), logcat_lines.len(), "{captured:?}");
        assert_eq!(
            crash.capture.as_ref().map(|c| c.entries as usize),
            Some(logcat_lines.len())
        );
        let exit = &exits_of(&data_dir(), &session.id)[0];
        assert_eq!(exit.matched_by, DebugSessionExitMatch::Pid);
        assert_eq!(exit.record.reason, AppExitReason::Crash);
    }

    // ── Attribution ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_crash_the_device_confirms_is_attributed_to_the_installs_session() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let device = FakeDevice::new(
            dir.path(),
            &dumpsys(7, installed_at() - chrono::Duration::seconds(2)),
        );

        let (id, _) = record_crash_in(work(dir.path(), &device.adb, crash_buffer(0), seen(1, 1)))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(id, session.id);
        let attribution = &crashes_of(dir.path(), &id)[0].attribution;
        assert_eq!(
            attribution.method,
            DebugSessionAttributionMethod::InstallRecord
        );
        assert!(attribution.verified);
        assert!(attribution
            .reason
            .as_deref()
            .unwrap()
            .starts_with("confirmed by the device: versionCode 7"));
    }

    #[tokio::test]
    async fn a_device_that_cannot_be_asked_leaves_the_crash_unverified_on_its_session() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let device = FakeDevice::with_script(
            dir.path(),
            "",
            "  *'dumpsys package'*) echo 'error: device offline' >&2; exit 1 ;;\n",
        );

        let (id, _) = record_crash_in(work(dir.path(), &device.adb, crash_buffer(0), seen(1, 1)))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(id, session.id);
        let attribution = &crashes_of(dir.path(), &id)[0].attribution;
        assert_eq!(
            attribution.method,
            DebugSessionAttributionMethod::InstallRecord
        );
        assert!(!attribution.verified);
        assert!(
            attribution
                .reason
                .as_deref()
                .unwrap()
                .contains("could not check that com.crash.app on R5CTCRASH01"),
            "{attribution:?}"
        );
    }

    #[tokio::test]
    async fn a_reinstall_outside_keynobi_sends_the_crash_to_an_unattributed_session() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        // Another tool installed versionCode 8 since.
        let device = FakeDevice::new(dir.path(), &dumpsys(8, Utc::now()));

        let (id, _) = record_crash_in(work(dir.path(), &device.adb, crash_buffer(0), seen(1, 1)))
            .await
            .unwrap()
            .unwrap();

        assert_ne!(id, session.id);
        assert!(crashes_of(dir.path(), &session.id).is_empty());
        let unattributed = read_manifest(dir.path(), &id).unwrap();
        assert_eq!(unattributed.build, None);
        assert_eq!(unattributed.install, None);
        assert_eq!(unattributed.package, PACKAGE);
        assert_eq!(unattributed.device.serial, SERIAL);
        assert!(unattributed.closed_at.is_none());
        let attribution = &crashes_of(dir.path(), &id)[0].attribution;
        assert_eq!(
            attribution.method,
            DebugSessionAttributionMethod::Unattributed
        );
        assert!(attribution
            .reason
            .as_deref()
            .unwrap()
            .contains("reinstalled outside Keynobi"));

        // The next crash goes to the same unattributed session.
        let mut again = seen(1, 1);
        again.pid = 5555;
        let (next, _) = record_crash_in(work(dir.path(), &device.adb, crash_buffer(0), again))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next, id);
    }

    #[tokio::test]
    async fn a_crash_without_a_keynobi_install_opens_an_unattributed_session() {
        let dir = TempDir::new().unwrap();
        let device = FakeDevice::new(dir.path(), "");

        let (id, _) = record_crash_in(work(dir.path(), &device.adb, crash_buffer(0), seen(1, 1)))
            .await
            .unwrap()
            .unwrap();

        let session = read_manifest(dir.path(), &id).unwrap();
        assert_eq!((session.build, session.install), (None, None));
        assert_eq!(session.recorded_by, DebugSessionRecorder::App);
        let crash = &crashes_of(dir.path(), &id)[0];
        assert_eq!(
            crash.attribution.method,
            DebugSessionAttributionMethod::Unattributed
        );
        assert!(crash.capture.is_some());
        assert_eq!(device.calls("dumpsys package"), 0, "nothing to verify");
    }

    #[tokio::test]
    async fn an_anr_names_its_package_and_pid_from_activity_manager() {
        let dir = TempDir::new().unwrap();
        let mut state = LogcatStateInner::new();
        let mut anr = line(
            1,
            Some(9),
            "ActivityManager",
            "ANR in com.crash.app:remote (com.crash.app/.Service)",
        );
        anr.pid = 1000;
        anr.package = Some("system_server".into());
        anr.flags |= EntryFlags::ANR;
        state.store.push(anr.clone());
        state
            .store
            .push(line(2, None, "ActivityManager", "PID: 6060"));
        let device = FakeDevice::new(dir.path(), "");

        let seen = CrashSeen::new(&anr, EntryDevice::Serial(SERIAL.into()));
        assert!(seen.anr);
        let (id, _) = record_crash_in(work(
            dir.path(),
            &device.adb,
            Arc::new(tokio::sync::Mutex::new(state)),
            seen,
        ))
        .await
        .unwrap()
        .unwrap();

        let session = summary_of(dir.path(), &id);
        assert_eq!(session.package, PACKAGE);
        assert_eq!((session.counts.anrs, session.counts.crashes), (1, 0));
        let anr = &crashes_of(dir.path(), &id)[0];
        assert_eq!(anr.pid, Some(6060));
        assert!(anr.summary.starts_with("ANR in com.crash.app:remote"));
    }

    // ── Two processes ────────────────────────────────────────────────────────

    const OTHER_PROCESS_ENV: &str = "KEYNOBI_TEST_CRASH_OTHER_PROCESS";

    /// Run by `a_crash_seen_by_two_processes_is_recorded_once` in a second
    /// process: record the same crash, received a second later.
    #[test]
    #[ignore = "run by a_crash_seen_by_two_processes_is_recorded_once in another process"]
    fn record_a_crash_for_another_process() {
        let Some(args) = std::env::var_os(OTHER_PROCESS_ENV) else {
            return;
        };
        let args = args.to_string_lossy().into_owned();
        let mut parts = args.split('|');
        let (dir, session, received, pid) = (
            PathBuf::from(parts.next().unwrap()),
            parts.next().unwrap().to_string(),
            parse_time(parts.next().unwrap()).unwrap(),
            parts.next().unwrap().parse().unwrap(),
        );
        let mut record = crash_record(received);
        record.crash.pid = Some(pid);
        let recorded = publish_crash_in(
            &dir,
            Destination::Session(session),
            record,
            None,
            RETAIN,
            Utc::now(),
        )
        .unwrap();
        println!();
        println!("RECORDED {}", recorded.is_some());
    }

    fn record_in_other_process(
        dir: &Path,
        session: &str,
        received: DateTime<Utc>,
        pid: u32,
    ) -> bool {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "services::debug_sessions::crashes::tests::record_a_crash_for_another_process",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(
                OTHER_PROCESS_ENV,
                format!("{}|{session}|{}|{pid}", dir.display(), stamp(received)),
            )
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("RECORDED "), "{stdout}");
        stdout.contains("RECORDED true")
    }

    #[test]
    fn a_crash_seen_by_two_processes_is_recorded_once() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let received = Utc::now();
        let mine = publish_crash_in(
            dir.path(),
            Destination::Session(session.id.clone()),
            crash_record(received),
            None,
            RETAIN,
            Utc::now(),
        )
        .unwrap();
        assert!(mine.is_some());

        let one_second_later = received + chrono::Duration::seconds(1);
        assert!(!record_in_other_process(
            dir.path(),
            &session.id,
            one_second_later,
            4321
        ));
        // Another process of the app, or the same one much later, is a new crash.
        assert!(record_in_other_process(
            dir.path(),
            &session.id,
            one_second_later,
            4322
        ));
        let ten_seconds_later = received + chrono::Duration::seconds(10);
        assert!(record_in_other_process(
            dir.path(),
            &session.id,
            ten_seconds_later,
            4321
        ));

        assert_eq!(crashes_of(dir.path(), &session.id).len(), 3);
    }

    // ── Exit reasons ─────────────────────────────────────────────────────────

    fn exit(at: &str, pid: u32, process: &str, reason: AppExitReason) -> AppExitRecord {
        AppExitRecord {
            timestamp: Some(at.into()),
            timestamp_local: None,
            pid: Some(pid),
            process_name: Some(process.into()),
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
        }
    }

    #[test]
    fn exits_are_attributed_by_pid_process_name_or_time_within_the_session_only() {
        let dir = TempDir::new().unwrap();
        let installed = installed_at();
        let session = open_install(dir.path(), installed);
        let mut record = crash_record(installed + chrono::Duration::minutes(1));
        record.crash.pid = Some(4321);
        publish_crash_in(
            dir.path(),
            Destination::Session(session.id.clone()),
            record,
            None,
            RETAIN,
            Utc::now(),
        )
        .unwrap();
        let after = |secs: i64| installed + chrono::Duration::seconds(secs);
        let records = vec![
            (
                after(61),
                exit("t1", 4321, "com.other", AppExitReason::Crash),
            ),
            (
                after(70),
                exit("t2", 999, PACKAGE, AppExitReason::LowMemory),
            ),
            (
                after(80),
                exit("t3", 998, "com.crash.app:remote", AppExitReason::Signaled),
            ),
            (after(-60), exit("t0", 997, PACKAGE, AppExitReason::Crash)),
        ];

        let added =
            attribute_exits_in(dir.path(), &session.id, SERIAL, records.clone(), Utc::now())
                .unwrap();

        assert_eq!(added, 3);
        let matched: Vec<(String, DebugSessionExitMatch)> = exits_of(dir.path(), &session.id)
            .into_iter()
            .map(|e| (e.record.timestamp.unwrap(), e.matched_by))
            .collect();
        assert_eq!(
            matched,
            [
                ("t1".to_string(), DebugSessionExitMatch::Pid),
                ("t2".to_string(), DebugSessionExitMatch::ProcessName),
                ("t3".to_string(), DebugSessionExitMatch::TimeWindow),
            ]
        );
        assert_eq!(
            exits_of(dir.path(), &session.id)[0].exited_at,
            stamp(after(61))
        );
        // Read again: nothing new.
        assert_eq!(
            attribute_exits_in(dir.path(), &session.id, SERIAL, records, Utc::now()).unwrap(),
            0
        );
        assert_eq!(summary_of(dir.path(), &session.id).counts.exits, 3);
    }

    #[test]
    fn an_exit_after_the_session_closed_belongs_to_the_next_one() {
        let dir = TempDir::new().unwrap();
        let first = open_install(dir.path(), installed_at());
        std::thread::sleep(Duration::from_millis(5));
        open_install(dir.path(), Utc::now());
        let closed = parse_time(
            &read_manifest(dir.path(), &first.id)
                .unwrap()
                .closed_at
                .unwrap(),
        )
        .unwrap();

        let added = attribute_exits_in(
            dir.path(),
            &first.id,
            SERIAL,
            vec![
                (
                    closed - chrono::Duration::seconds(1),
                    exit("in", 1, PACKAGE, AppExitReason::Crash),
                ),
                (
                    closed + chrono::Duration::seconds(1),
                    exit("out", 2, PACKAGE, AppExitReason::Crash),
                ),
            ],
            Utc::now(),
        )
        .unwrap();

        assert_eq!(added, 1);
        assert_eq!(
            exits_of(dir.path(), &first.id)[0]
                .record
                .timestamp
                .as_deref(),
            Some("in")
        );
    }

    #[tokio::test]
    async fn exit_reasons_are_read_once_per_session_however_many_crashes_arrive() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let device = FakeDevice::new(dir.path(), "");
        let schedule = || {
            schedule_exit_read(
                dir.path().to_path_buf(),
                session.id.clone(),
                device.adb.clone(),
                SERIAL.into(),
            )
        };

        assert!(schedule());
        assert!(!schedule(), "a read is already waiting");
        let deadline = Instant::now() + Duration::from_secs(30);
        while device.calls("exit-info") == 0 {
            assert!(Instant::now() < deadline, "exit reasons not read");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        // Once the read started, a later crash schedules another.
        assert!(schedule());
        while device.calls("exit-info") < 2 {
            assert!(Instant::now() < deadline, "exit reasons not read again");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(EXIT_READ_DELAY * 2).await;
        assert_eq!(device.calls("exit-info"), 2);

        let reads = ExitReads::default();
        for n in 0..MAX_PENDING_EXIT_READS {
            assert!(reads.begin(&format!("s{n}")));
        }
        assert!(!reads.begin("one too many"));
        reads.end("s0");
        assert!(reads.begin("one too many"));
    }

    #[tokio::test]
    async fn refreshing_reads_the_sessions_device_and_reports_old_devices() {
        let dir = TempDir::new().unwrap();
        let session = open_install(dir.path(), installed_at());
        let device = FakeDevice::with_script(dir.path(), "", "  *getprop*) echo 29 ;;\n");

        let refresh = refresh_exit_reasons_in(dir.path(), &session.id, &device.adb)
            .await
            .unwrap();

        assert_eq!(refresh.added, 0);
        assert!(refresh.message.unwrap().contains("Android 11"));
        assert_eq!(device.calls("exit-info"), 0);
        assert!(matches!(
            refresh_exit_reasons_in(dir.path(), "s-20260925T103200Z-000000000000", &device.adb)
                .await,
            Err(AppError::NotFound(_))
        ));
    }
}
