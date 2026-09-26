//! Deobfuscating crash stack traces with the Android SDK's R8 `retrace`.
//!
//! A trace is deobfuscated only with the mapping of the build Keynobi
//! installed on the device the crash came from, and only after the device
//! reports the same install. A wrong mapping gives plausible but wrong
//! stacks, so anything uncertain is refused: the trace is returned as logcat
//! printed it, with the reason. The Tauri command and the MCP tools call
//! [`retrace_crash_group`].

use crate::models::build::{InstalledBuild, MappingSnapshot};
use crate::models::error::AppError;
use crate::models::retrace::{MappingMatch, RetraceOutcome, RetraceStatus};
use crate::models::settings::AppSettings;
use crate::services::adb_manager::{self, DeviceState};
use crate::services::build_runner;
use crate::services::installed_builds::{self, InstallTarget};
use crate::services::jdk::{self, JdkSearchRoots, MIN_GRADLE_JDK_MAJOR};
use crate::services::logcat::{EntryDevice, LogcatState};
use crate::services::mapping_snapshots;
use crate::services::settings_manager::{data_dir, unique_tmp_path};
use crate::utils::device_shell::quote_device_shell_arg;
use crate::utils::process::{
    describe_failure, output_with_timeout, ADB_QUERY_TIMEOUT, ADB_UNRESPONSIVE_HINT, RETRACE_HINT,
    RETRACE_TIMEOUT,
};
use crate::utils::validation::{validate_device_serial, validate_package_name};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeDelta, TimeZone, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime};
use tokio::process::Command;

/// Largest trace deobfuscated. A Java crash with its causes is a few KiB.
pub const MAX_RETRACE_INPUT_BYTES: usize = 256 * 1024;

/// Largest `retrace` output accepted. It prints about one line per input
/// line, more where inlined frames expand.
pub const MAX_RETRACE_OUTPUT_BYTES: usize = 1024 * 1024;

/// Most deobfuscated traces kept in memory, keyed by mapping and trace.
pub const MAX_RETRACE_CACHE: usize = 32;

/// Most crash groups `get_crash_logs` deobfuscates in one call, newest first.
pub const MAX_RETRACED_CRASH_GROUPS: usize = 5;

/// Largest `dumpsys package` output read.
const MAX_DUMPSYS_BYTES: usize = 1024 * 1024;

/// How much later than Keynobi's recorded install the device may report the
/// app's last update: `dumpsys` prints whole seconds, and the two clocks are
/// compared through one `date` call.
const UPDATE_TIME_TOLERANCE_SECS: i64 = 30;

/// Largest `source.properties` read for the Command-line Tools version.
const MAX_SOURCE_PROPERTIES_BYTES: u64 = 64 * 1024;

/// Folder in the data directory for the trace file `retrace` reads.
const RETRACE_TMP_DIR: &str = "retrace";

/// Trace files older than this belong to a process that died mid-call.
const STALE_TMP_AGE: Duration = Duration::from_secs(60 * 60);

const MISSING_TOOL_HINT: &str = "Install \"Android SDK Command-line Tools\" in Android Studio's \
     SDK Manager (SDK Tools tab) for the SDK set in Settings.";

// ── The tool ──────────────────────────────────────────────────────────────────

/// The SDK's `retrace` script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetraceTool {
    /// Canonical path of the script.
    pub path: PathBuf,
    /// `Pkg.Revision` of the Command-line Tools it belongs to (`22.0`).
    pub version: Option<String>,
}

/// `retrace` in the Android SDK at `sdk`: `cmdline-tools/latest/bin/retrace`,
/// else the one in the highest versioned `cmdline-tools/<version>`.
pub fn find_retrace(sdk: &Path) -> Option<RetraceTool> {
    let tools = sdk.join("cmdline-tools");
    retrace_in(&tools.join("latest")).or_else(|| {
        let mut versioned: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(&tools)
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                Some((dotted_version(&name)?, entry.path()))
            })
            .collect();
        versioned.sort();
        versioned
            .into_iter()
            .rev()
            .find_map(|(_, dir)| retrace_in(&dir))
    })
}

/// `retrace` in the SDK the settings name. A project's `local.properties`
/// is never consulted: the project must not choose what Keynobi runs.
pub fn find_configured_retrace(settings: &AppSettings) -> Option<RetraceTool> {
    find_retrace(&settings_sdk(settings)?)
}

/// The Health view of [`find_configured_retrace`] for MCP.
pub fn health_json(settings: &AppSettings) -> Value {
    let tool = find_configured_retrace(settings);
    json!({
        "ok": tool.is_some(),
        "path": tool.as_ref().map(|t| t.path.to_string_lossy().into_owned()),
        "version": tool.as_ref().and_then(|t| t.version.clone()),
        "hint": if tool.is_some() {
            Value::Null
        } else {
            json!(format!("retrace not found, so crash stacks cannot be deobfuscated. {MISSING_TOOL_HINT}"))
        },
    })
}

/// The Health view of [`find_configured_retrace`] for the app: the Command-line
/// Tools version, `unknown` when it is not recorded, or `None` when missing.
pub fn health_version(settings: &AppSettings) -> Option<String> {
    find_configured_retrace(settings).map(|t| t.version.unwrap_or_else(|| "unknown".into()))
}

fn settings_sdk(settings: &AppSettings) -> Option<PathBuf> {
    let sdk = settings.android.sdk_path.as_deref().map(str::trim)?;
    if sdk.is_empty() {
        return None;
    }
    Some(match (sdk.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(sdk),
    })
}

fn retrace_in(dir: &Path) -> Option<RetraceTool> {
    let path = dir.join("bin").join("retrace").canonicalize().ok()?;
    let meta = std::fs::metadata(&path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    if !meta.is_file() {
        return None;
    }
    Some(RetraceTool {
        path,
        version: tools_revision(dir),
    })
}

/// `22.0` → `[22, 0]`; anything but dot-separated numbers is `None`.
fn dotted_version(name: &str) -> Option<Vec<u32>> {
    name.split('.').map(|part| part.parse().ok()).collect()
}

fn tools_revision(dir: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(dir.join("source.properties"))
        .ok()?
        .take(MAX_SOURCE_PROPERTIES_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("Pkg.Revision=")?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// `JAVA_HOME` for `retrace` (`None` to use `java` on `PATH`), resolved as
/// for Gradle builds, or why no JDK 17+ is available.
async fn retrace_java_home(env: &RetraceEnv) -> Result<Option<PathBuf>, String> {
    let java = jdk::check_project_java(
        &env.settings,
        env.project_root.as_deref(),
        env.gradle_root.as_deref(),
        &env.jdk_roots,
    )
    .await;
    if !java.found {
        return Err(format!(
            "retrace needs Java {MIN_GRADLE_JDK_MAJOR} or newer, and no working Java was found \
             at {}. Set a JDK {MIN_GRADLE_JDK_MAJOR}+ in Settings → Tools (java.home), or \
             install Android Studio.",
            java.bin.display()
        ));
    }
    match java.major {
        Some(major) if major >= MIN_GRADLE_JDK_MAJOR => Ok(java.jdk.map(|jdk| jdk.home)),
        major => Err(format!(
            "retrace needs JDK {MIN_GRADLE_JDK_MAJOR} or newer; the JDK Keynobi uses ({}) is {}. \
             Set a JDK {MIN_GRADLE_JDK_MAJOR}+ in Settings → Tools (java.home).",
            java.bin.display(),
            major.map_or("of an unknown version".to_string(), |m| format!("JDK {m}")),
        )),
    }
}

// ── Running it ────────────────────────────────────────────────────────────────

/// A trace file that is removed when dropped, whatever happened.
struct TraceFile(PathBuf);

impl Drop for TraceFile {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.0) {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Cannot remove {}: {e}", self.0.display());
            }
        }
    }
}

fn write_trace_file(dir: &Path, trace: &str) -> Result<TraceFile, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    remove_stale_trace_files(dir);
    let path = unique_tmp_path(&dir.join("trace.txt"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|e| format!("Cannot write the trace for retrace: {e}"))?;
    let guard = TraceFile(path);
    file.write_all(trace.as_bytes())
        .map_err(|e| format!("Cannot write the trace for retrace: {e}"))?;
    Ok(guard)
}

fn remove_stale_trace_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age > STALE_TMP_AGE);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Run `retrace <mapping> <trace file>` and return what it printed. The trace
/// goes through a private file in `tmp_dir`, removed before this returns.
async fn run_retrace(
    tool: &Path,
    java_home: Option<&Path>,
    mapping: &Path,
    trace: &str,
    tmp_dir: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let trace_file = write_trace_file(tmp_dir, trace)?;
    let mut cmd = Command::new(tool);
    cmd.arg(mapping)
        .arg(&trace_file.0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = java_home {
        cmd.env("JAVA_HOME", home);
    }
    let output = output_with_timeout(&mut cmd, timeout)
        .await
        .map_err(|e| describe_failure("retrace", &e, RETRACE_HINT))?;
    drop(trace_file);

    if !output.status.success() {
        let why = first_line(&output.stderr)
            .or_else(|| first_line(&output.stdout))
            .unwrap_or_else(|| format!("exit status {}", output.status));
        return Err(format!("retrace failed: {why}"));
    }
    if output.stdout.len() > MAX_RETRACE_OUTPUT_BYTES {
        return Err(format!(
            "retrace printed {} KiB, more than the {} KiB Keynobi accepts",
            output.stdout.len() / 1024,
            MAX_RETRACE_OUTPUT_BYTES / 1024
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn first_line(bytes: &[u8]) -> Option<String> {
    const MAX_CHARS: usize = 300;
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(MAX_CHARS).collect())
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Deobfuscated traces by (mapping SHA-256, trace SHA-256), least recently
/// used first; at most [`MAX_RETRACE_CACHE`].
#[derive(Default)]
pub struct RetraceCache(Mutex<VecDeque<((String, String), String)>>);

impl RetraceCache {
    fn get(&self, key: &(String, String)) -> Option<String> {
        let mut entries = self.0.lock().ok()?;
        let at = entries.iter().position(|(k, _)| k == key)?;
        let entry = entries.remove(at)?;
        let value = entry.1.clone();
        entries.push_back(entry);
        Some(value)
    }

    fn put(&self, key: (String, String), value: String) {
        let Ok(mut entries) = self.0.lock() else {
            return;
        };
        entries.retain(|(k, _)| *k != key);
        entries.push_back((key, value));
        while entries.len() > MAX_RETRACE_CACHE {
            entries.pop_front();
        }
    }
}

static SHARED_CACHE: LazyLock<Arc<RetraceCache>> = LazyLock::new(Arc::default);

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ── Choosing the mapping ──────────────────────────────────────────────────────

fn build_name(build_id: Option<u32>) -> String {
    build_id.map_or_else(
        || "an unrecorded build".to_string(),
        |id| format!("build #{id}"),
    )
}

fn describe_mapping(mapping: &MappingSnapshot) -> String {
    match &mapping.pg_map_id {
        Some(id) => format!("{} {}, map id {id}", mapping.module, mapping.variant),
        None => format!("{} {}", mapping.module, mapping.variant),
    }
}

/// The saved mapping of the build `installed` names, or why there is none
/// to trust. Several mappings are narrowed to the installed APK's module and
/// variant, as the build's record lists it; still several is refused.
fn choose_mapping(dir: &Path, installed: &InstalledBuild) -> Result<MappingSnapshot, String> {
    let Some(build_id) = installed.build_id else {
        return Err(format!(
            "the APK Keynobi installed of {} was not written by a recorded build, so no R8 \
             mapping is known for it",
            installed.package
        ));
    };
    if installed.mappings.is_empty() {
        return Err(format!(
            "no R8 mapping was recorded for this install of build #{build_id} (the variant is \
             not minified, or the build did not rewrite its mapping)"
        ));
    }
    let present: Vec<&MappingSnapshot> = installed
        .mappings
        .iter()
        .filter(|m| {
            mapping_snapshots::snapshot_path(dir, &m.sha256).is_some_and(|path| path.is_file())
        })
        .collect();
    match present.as_slice() {
        [] => Err(format!(
            "the saved R8 mapping of build #{build_id} ({}) is missing from Keynobi's data \
             directory",
            describe_mapping(&installed.mappings[0])
        )),
        [only] => Ok((*only).clone()),
        several => {
            let history = build_runner::load_build_history_from(dir);
            let apk = history
                .iter()
                .filter(|record| record.id == build_id)
                .flat_map(|record| record.apks.iter())
                .find(|apk| apk.sha256 == installed.apk_sha256);
            let matching: Vec<&&MappingSnapshot> = several
                .iter()
                .filter(|m| {
                    apk.is_some_and(|apk| {
                        m.module == apk.module && m.variant.eq_ignore_ascii_case(&apk.variant)
                    })
                })
                .collect();
            match matching.as_slice() {
                [only] => Ok((**only).clone()),
                _ => Err(format!(
                    "build #{build_id} saved several R8 mappings for this install ({}), and the \
                     installed APK's variant does not pick one",
                    several
                        .iter()
                        .map(|m| describe_mapping(m))
                        .collect::<Vec<_>>()
                        .join("; ")
                )),
            }
        }
    }
}

// ── Checking the device ───────────────────────────────────────────────────────

/// What `dumpsys package` reports about an installed package.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DevicePackage {
    version_code: Option<u64>,
    /// Device local time, as printed.
    last_update: Option<String>,
}

/// The `Package [<package>]` block of `dumpsys package <package>`, or `None`
/// when the package is not installed.
fn parse_dumpsys_package(text: &str, package: &str) -> Option<DevicePackage> {
    let header = format!("Package [{package}]");
    let mut lines = text.lines();
    let indent = lines.by_ref().find_map(|line| {
        let trimmed = line.trim_start();
        trimmed
            .starts_with(&header)
            .then(|| line.len() - trimmed.len())
    })?;
    let mut found = DevicePackage {
        version_code: None,
        last_update: None,
    };
    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        // The block ends at the next line indented no deeper than its header.
        if line.len() - trimmed.len() <= indent {
            break;
        }
        if found.version_code.is_none() {
            if let Some(rest) = trimmed.strip_prefix("versionCode=") {
                found.version_code = rest
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|digits| digits.parse().ok());
            }
        }
        if found.last_update.is_none() {
            if let Some(rest) = trimmed.strip_prefix("lastUpdateTime=") {
                found.last_update = Some(rest.trim().to_string());
            }
        }
    }
    Some(found)
}

/// `date +%s:%z` output: the device's clock and UTC offset.
fn parse_device_clock(text: &str) -> Option<(i64, FixedOffset)> {
    let (epoch, offset) = text.trim().split_once(':')?;
    let epoch = epoch.trim().parse().ok()?;
    let offset = offset.trim();
    let sign = match offset.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = offset.get(1..)?;
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..].parse().ok()?;
    Some((
        epoch,
        FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))?,
    ))
}

/// Whether the device's `lastUpdateTime` (local time at `offset`) is later
/// than `installed_at`, after correcting for the device clock's difference
/// from the host (`device_now` against `host_now`).
fn updated_after_install(
    last_update: NaiveDateTime,
    offset: FixedOffset,
    device_now: i64,
    host_now: DateTime<Utc>,
    installed_at: DateTime<Utc>,
) -> Option<bool> {
    let updated = offset
        .from_local_datetime(&last_update)
        .single()?
        .with_timezone(&Utc);
    let skew = TimeDelta::try_seconds(device_now - host_now.timestamp())?;
    let on_host_clock = updated - skew;
    Some(on_host_clock > installed_at + TimeDelta::try_seconds(UPDATE_TIME_TOLERANCE_SECS)?)
}

async fn adb_shell(adb: &Path, serial: &str, args: &[&str]) -> Result<String, String> {
    let output = output_with_timeout(
        Command::new(adb)
            .args(["-s", serial, "shell"])
            .args(args)
            .stdin(Stdio::null()),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure(&format!("adb shell {}", args[0]), &e, ADB_UNRESPONSIVE_HINT))?;
    if !output.status.success() {
        let why = first_line(&output.stderr)
            .or_else(|| first_line(&output.stdout))
            .unwrap_or_else(|| format!("exit status {}", output.status));
        return Err(format!("adb shell {} failed: {why}", args[0]));
    }
    let end = output.stdout.len().min(MAX_DUMPSYS_BYTES);
    Ok(String::from_utf8_lossy(&output.stdout[..end]).into_owned())
}

/// Ask the device whether `installed` is still the app it runs: the same
/// version code, and no update after Keynobi's install. Returns what the
/// device confirmed, or why the mapping must not be used.
async fn check_device(
    adb: &Path,
    serial: &str,
    device: &str,
    installed: &InstalledBuild,
) -> Result<String, String> {
    let build = build_name(installed.build_id);
    let unchecked = |why: String| {
        format!(
            "could not check that {} on {device} is still {build} ({why}), so its mapping \
             was not used",
            installed.package
        )
    };
    let dumpsys = adb_shell(
        adb,
        serial,
        &[
            "dumpsys",
            "package",
            &quote_device_shell_arg(&installed.package),
        ],
    )
    .await
    .map_err(unchecked)?;
    let clock = adb_shell(adb, serial, &["date", "+%s:%z"])
        .await
        .map_err(unchecked)?;
    let host_now = Utc::now();

    let Some(app) = parse_dumpsys_package(&dumpsys, &installed.package) else {
        return Err(format!(
            "{} is no longer installed on {device}",
            installed.package
        ));
    };
    let Some(device_version) = app.version_code else {
        return Err(unchecked("the device reported no versionCode".into()));
    };
    if let Some(recorded) = installed.version_code {
        if u64::from(recorded) != device_version {
            return Err(format!(
                "the app was reinstalled outside Keynobi after {build}: {device} runs \
                 versionCode {device_version}, Keynobi installed versionCode {recorded}"
            ));
        }
    }
    let Some(last_update) = app.last_update else {
        return Err(unchecked("the device reported no lastUpdateTime".into()));
    };
    let parsed = NaiveDateTime::parse_from_str(&last_update, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| unchecked(format!("unrecognised lastUpdateTime {last_update:?}")))?;
    let (device_now, offset) = parse_device_clock(&clock)
        .ok_or_else(|| unchecked(format!("unrecognised device clock {:?}", clock.trim())))?;
    let installed_at = DateTime::parse_from_rfc3339(&installed.installed_at)
        .map_err(|_| unchecked("the install record has no valid time".into()))?
        .with_timezone(&Utc);
    match updated_after_install(parsed, offset, device_now, host_now, installed_at) {
        Some(false) => Ok(format!(
            "versionCode {device_version}, last updated {last_update}"
        )),
        Some(true) => Err(format!(
            "the app was reinstalled outside Keynobi after {build}: {device} last updated \
             {} at {last_update} (device time), after Keynobi's install at {}",
            installed.package, installed.installed_at
        )),
        None => Err(unchecked(format!(
            "cannot compare lastUpdateTime {last_update:?} with the install time"
        ))),
    }
}

// ── Deobfuscating ─────────────────────────────────────────────────────────────

/// What deobfuscation may use: settings (SDK, JDK), the open project (whose
/// `gradle.properties` may choose the JDK only when trusted), and devices.
pub struct RetraceEnv {
    pub settings: AppSettings,
    pub project_root: Option<PathBuf>,
    pub gradle_root: Option<PathBuf>,
    pub device_state: DeviceState,
    data_dir: PathBuf,
    jdk_roots: JdkSearchRoots,
    cache: Arc<RetraceCache>,
    timeout: Duration,
}

impl RetraceEnv {
    pub fn new(
        settings: AppSettings,
        project_root: Option<PathBuf>,
        gradle_root: Option<PathBuf>,
        device_state: DeviceState,
    ) -> Self {
        Self {
            settings,
            project_root,
            gradle_root,
            device_state,
            data_dir: data_dir(),
            jdk_roots: JdkSearchRoots::system(),
            cache: SHARED_CACHE.clone(),
            timeout: RETRACE_TIMEOUT,
        }
    }
}

/// One crash from the logcat buffer: its lines, package, and device.
#[derive(Debug, Clone)]
pub struct CrashTrace {
    pub trace: String,
    pub package: Option<String>,
    pub device: EntryDevice,
}

/// The crash group `crash_group_id` in the logcat buffer, or `None` when it
/// is no longer there.
pub async fn crash_trace(logcat_state: &LogcatState, crash_group_id: u64) -> Option<CrashTrace> {
    let state = logcat_state.lock().await;
    let mut lines: Vec<_> = state
        .store
        .iter()
        .filter(|e| e.crash_group_id == Some(crash_group_id))
        .collect();
    lines.sort_by_key(|e| e.id);
    let first = lines.first()?;
    let mut trace = String::new();
    for line in &lines {
        trace.push_str(&line.message);
        trace.push('\n');
    }
    Some(CrashTrace {
        device: state.device_of_entry(first.id),
        package: lines.iter().find_map(|e| e.package.clone()),
        trace,
    })
}

/// Deobfuscate the crash group `crash_group_id` from the logcat buffer with
/// the mapping of the build Keynobi installed on its device. `NotFound` when
/// the group left the buffer; every other outcome, refusals included, is an
/// `Ok` that says what happened.
pub async fn retrace_crash_group(
    env: &RetraceEnv,
    logcat_state: &LogcatState,
    crash_group_id: u64,
) -> Result<RetraceOutcome, AppError> {
    let crash = crash_trace(logcat_state, crash_group_id)
        .await
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Crash {crash_group_id} is no longer in the logcat buffer."
            ))
        })?;
    Ok(retrace_trace(env, &crash.device, crash.package.as_deref(), &crash.trace).await)
}

/// Deobfuscate `trace`, a crash of `package` read from `device`.
pub async fn retrace_trace(
    env: &RetraceEnv,
    device: &EntryDevice,
    package: Option<&str>,
    trace: &str,
) -> RetraceOutcome {
    let mut outcome = RetraceOutcome {
        status: RetraceStatus::Refused,
        trace: trace.to_string(),
        build_id: None,
        mapping: None,
        matched_by: None,
        device: None,
        package: package.map(str::to_string),
        reason: None,
        summary: String::new(),
    };
    match retrace_into(env, device, package, trace, &mut outcome).await {
        Ok(retraced) => {
            outcome.status = RetraceStatus::Retraced;
            outcome.trace = retraced.trace;
            outcome.summary = retraced.summary;
        }
        Err((status, reason)) => {
            outcome.status = status;
            outcome.summary = format!("Not deobfuscated: {reason}.");
            outcome.reason = Some(reason);
        }
    }
    outcome
}

struct Retraced {
    trace: String,
    summary: String,
}

type NotRetraced = (RetraceStatus, String);

fn refused(reason: String) -> NotRetraced {
    (RetraceStatus::Refused, reason)
}

async fn retrace_into(
    env: &RetraceEnv,
    device: &EntryDevice,
    package: Option<&str>,
    trace: &str,
    outcome: &mut RetraceOutcome,
) -> Result<Retraced, NotRetraced> {
    if trace.len() > MAX_RETRACE_INPUT_BYTES {
        return Err(refused(format!(
            "the trace is {} KiB; at most {} KiB is deobfuscated",
            trace.len() / 1024,
            MAX_RETRACE_INPUT_BYTES / 1024
        )));
    }
    let package = package.ok_or_else(|| {
        refused("logcat did not attribute the crash to a package, so its build is unknown".into())
    })?;
    validate_package_name(package)
        .map_err(|_| refused(format!("{package:?} is not a valid package name")))?;

    let adb = adb_manager::get_adb_path(&env.settings);
    let serial = match device {
        EntryDevice::Serial(serial) => serial.clone(),
        EntryDevice::Unnamed => only_online_device(&adb).await.map_err(refused)?,
        EntryDevice::Unknown => {
            return Err(refused(
                "Keynobi no longer knows which device this crash came from (logcat was \
                 restarted many times since)"
                    .into(),
            ))
        }
    };
    validate_device_serial(&serial)
        .map_err(|_| refused(format!("{serial:?} is not a valid device serial")))?;

    let target = installed_builds::resolve_target(&adb, &serial, &env.device_state).await;
    if serial.starts_with("emulator-") && target.avd_name.is_none() {
        return Err(refused(format!(
            "{serial} did not report its AVD name, so Keynobi cannot tell which install is on \
             it; is the emulator still running?"
        )));
    }
    let device_name = target.avd_name.clone().unwrap_or_else(|| serial.clone());
    outcome.device = Some(device_name.clone());

    let installed = installed_for(&env.data_dir, &target, package).ok_or_else(|| {
        refused(format!(
            "Keynobi has no record of installing {package} on {device_name}; only apps it \
             installed (Run App, MCP install_apk) can be deobfuscated"
        ))
    })?;
    outcome.build_id = installed.build_id;
    let mapping = choose_mapping(&env.data_dir, &installed).map_err(refused)?;
    outcome.mapping = Some(mapping.clone());
    outcome.matched_by = Some(MappingMatch::InstallRecord);

    let confirmed = check_device(&adb, &serial, &device_name, &installed)
        .await
        .map_err(refused)?;
    let summary = format!(
        "Deobfuscated with the R8 mapping of {} ({}), matched by Keynobi's install on \
         {device_name} at {} and confirmed by the device ({confirmed}).",
        build_name(installed.build_id),
        describe_mapping(&mapping),
        installed.installed_at
    );

    let key = (mapping.sha256.clone(), sha256_hex(trace.as_bytes()));
    if let Some(trace) = env.cache.get(&key) {
        return Ok(Retraced { trace, summary });
    }

    let tool = settings_sdk(&env.settings)
        .as_deref()
        .and_then(find_retrace)
        .ok_or_else(|| {
            (
                RetraceStatus::Unavailable,
                format!("retrace was not found in the Android SDK. {MISSING_TOOL_HINT}"),
            )
        })?;
    let java_home = retrace_java_home(env)
        .await
        .map_err(|reason| (RetraceStatus::Unavailable, reason))?;
    let mapping_path = mapping_snapshots::snapshot_path(&env.data_dir, &mapping.sha256)
        .ok_or_else(|| refused("the mapping's name is not a SHA-256".into()))?;

    let retraced = run_retrace(
        &tool.path,
        java_home.as_deref(),
        &mapping_path,
        trace,
        &env.data_dir.join(RETRACE_TMP_DIR),
        env.timeout,
    )
    .await
    .map_err(|reason| (RetraceStatus::Failed, reason))?;
    env.cache.put(key, retraced.clone());
    Ok(Retraced {
        trace: retraced,
        summary,
    })
}

fn installed_for(dir: &Path, target: &InstallTarget, package: &str) -> Option<InstalledBuild> {
    installed_builds::installed_build_in(dir, target, package)
}

/// The serial of the one online device, for a logcat stream started without
/// one (adb then reads its only device).
async fn only_online_device(adb: &Path) -> Result<String, String> {
    let online: Vec<String> = adb_manager::list_devices(adb)
        .await
        .into_iter()
        .filter(|d| d.connection_state == crate::models::device::DeviceConnectionState::Online)
        .map(|d| d.serial)
        .collect();
    match online.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(
            "logcat was started without naming a device, and no device is connected \
                   now to check the install on"
                .into(),
        ),
        several => Err(format!(
            "logcat was started without naming a device, and {} devices are connected now; \
             start logcat on the device that crashed",
            several.len()
        )),
    }
}

/// The MCP view of an outcome, snake_case like the other tools.
pub fn outcome_json(outcome: &RetraceOutcome) -> Value {
    let mapping = outcome.mapping.as_ref();
    json!({
        "status": outcome.status,
        "mapping_line": outcome.summary,
        "trace": outcome.trace,
        "build_id": outcome.build_id,
        "module": mapping.map(|m| &m.module),
        "variant": mapping.map(|m| &m.variant),
        "map_id": mapping.and_then(|m| m.pg_map_id.as_ref()),
        "mapping_sha256": mapping.map(|m| &m.sha256),
        "matched_by": outcome.matched_by.map(|_| "install_record"),
        "device": outcome.device,
        "reason": outcome.reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::build::{BuildRecord, BuildStatus, BuiltApk};
    use crate::services::jdk::test_support::fake_jdk_home;
    use crate::utils::process::test_support::run_once;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    const SERIAL: &str = "R5CT1234ABC";
    const PACKAGE: &str = "com.example.app";
    const TRACE: &str = "FATAL EXCEPTION: main\n\
        Process: com.example.app, PID: 1234\n\
        java.lang.RuntimeException: boom\n\
        \tat a.a.onCreate(SourceFile:1)\n";
    const MAPPING: &[u8] = b"# pg_map_id: 6b1c2f0\n\
        com.example.app.MainActivity -> a.a:\n\
        \x20   1:1:void onCreate(android.os.Bundle):24 -> onCreate\n";

    fn write_script(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A fake `retrace` in `<sdk>/cmdline-tools/<dir>/bin`. Run with no
    /// arguments it exits at once, so it can be run once before a timed test.
    fn write_retrace(sdk: &Path, dir: &str, body: &str) -> PathBuf {
        let path = sdk.join("cmdline-tools").join(dir).join("bin/retrace");
        write_script(&path, &format!("[ $# -eq 0 ] && exit 0\n{body}"));
        run_once(&path);
        path
    }

    /// A fake `retrace` that records its argument vector (NUL-separated) and
    /// the trace file it was given, counts its runs, and prints the trace with
    /// `a.a.onCreate` deobfuscated.
    fn transforming_retrace(sdk: &Path, record: &Path) -> PathBuf {
        write_retrace(
            sdk,
            "latest",
            &format!(
                "printf '%s\\0' \"$@\" > '{rec}.argv'\n\
                 cat \"$2\" > '{rec}.input'\n\
                 echo run >> '{rec}.runs'\n\
                 sed 's/a\\.a\\.onCreate(SourceFile:1)/com.example.app.MainActivity.onCreate(MainActivity.kt:24)/' \"$2\"",
                rec = record.display()
            ),
        )
    }

    struct Fixture {
        _tmp: TempDir,
        root: PathBuf,
        data: PathBuf,
        sdk: PathBuf,
        env: RetraceEnv,
    }

    impl Fixture {
        /// A data directory, an SDK with a fake adb that reports the app as
        /// installed ten minutes ago with versionCode 42, and a JDK 17.
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let root = tmp.path().canonicalize().unwrap();
            let data = root.join("data");
            let sdk = root.join("sdk");
            std::fs::create_dir_all(&data).unwrap();
            let jdk = fake_jdk_home(&root.join("jdk17"), "17.0.9");
            let mut settings = AppSettings::default();
            settings.android.sdk_path = Some(sdk.to_string_lossy().into_owned());
            settings.java.home = Some(jdk.to_string_lossy().into_owned());
            let env = RetraceEnv {
                settings,
                project_root: None,
                gradle_root: None,
                device_state: DeviceState::new(),
                data_dir: data.clone(),
                jdk_roots: JdkSearchRoots::default(),
                cache: Arc::default(),
                timeout: Duration::from_secs(30),
            };
            let fixture = Self {
                _tmp: tmp,
                root,
                data,
                sdk,
                env,
            };
            let installed_at = Utc::now() - TimeDelta::minutes(10);
            fixture.write_adb(&dumpsys(42, installed_at - TimeDelta::seconds(2)));
            fixture
        }

        fn record(&self) -> PathBuf {
            self.root.join("retrace-record")
        }

        fn runs(&self) -> usize {
            std::fs::read_to_string(self.record().with_extension("runs"))
                .map(|s| s.lines().count())
                .unwrap_or(0)
        }

        /// A fake adb answering `dumpsys package` with `dumpsys` and `date`
        /// with the host clock in UTC.
        fn write_adb(&self, dumpsys: &str) {
            let answer = self.root.join("dumpsys.txt");
            std::fs::write(&answer, dumpsys).unwrap();
            self.write_adb_script(&format!(
                "case \"$*\" in\n\
                 *'dumpsys package'*) cat '{}' ;;\n\
                 *date*) echo \"$(date -u +%s):+0000\" ;;\n\
                 *devices*) printf 'List of devices attached\\n{SERIAL} device product:p model:Pixel_8 device:d\\n' ;;\n\
                 esac",
                answer.display()
            ));
        }

        fn write_adb_script(&self, body: &str) {
            let adb = self.sdk.join("platform-tools/adb");
            write_script(&adb, body);
            run_once(&adb);
        }

        fn save_mapping(&self, bytes: &[u8]) -> MappingSnapshot {
            let sha256 = sha256_hex(bytes);
            let path = mapping_snapshots::snapshot_path(&self.data, &sha256).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
            MappingSnapshot {
                module: ":app".into(),
                variant: "release".into(),
                sha256,
                bytes: bytes.len() as u64,
                pg_map_id: Some("6b1c2f0".into()),
            }
        }

        fn save_install(&self, installed: &InstalledBuild) {
            std::fs::write(
                self.data.join("installed-builds.json"),
                serde_json::to_string(&vec![installed]).unwrap(),
            )
            .unwrap();
        }

        /// Keynobi installed build #12 on the device ten minutes ago, with
        /// the one mapping it saved.
        fn install_with_mapping(&self) -> InstalledBuild {
            let mapping = self.save_mapping(MAPPING);
            let installed = installed(vec![mapping]);
            self.save_install(&installed);
            installed
        }

        async fn retrace(&self, trace: &str) -> RetraceOutcome {
            retrace_trace(
                &self.env,
                &EntryDevice::Serial(SERIAL.into()),
                Some(PACKAGE),
                trace,
            )
            .await
        }
    }

    fn installed(mappings: Vec<MappingSnapshot>) -> InstalledBuild {
        InstalledBuild {
            serial: SERIAL.into(),
            avd_name: None,
            model: None,
            package: PACKAGE.into(),
            apk_sha256: "a1".repeat(32),
            build_id: Some(12),
            version_code: Some(42),
            mappings,
            installed_at: (Utc::now() - TimeDelta::minutes(10)).to_rfc3339(),
        }
    }

    fn dumpsys(version_code: u64, last_update: DateTime<Utc>) -> String {
        format!(
            "Packages:\n  Package [{PACKAGE}] (1a2b3c):\n    userId=10123\n    \
             versionCode={version_code} minSdk=24 targetSdk=34\n    versionName=1.0\n    \
             firstInstallTime=2026-01-01 10:00:00\n    lastUpdateTime={}\n\n\
             Queries:\n  system apps queryable: false\n",
            last_update.format("%Y-%m-%d %H:%M:%S")
        )
    }

    fn assert_refused(outcome: &RetraceOutcome, reason: &str) {
        assert_eq!(outcome.status, RetraceStatus::Refused, "{outcome:?}");
        let got = outcome.reason.as_deref().unwrap_or_default();
        assert!(got.contains(reason), "{got:?} does not mention {reason:?}");
        assert_eq!(outcome.trace, TRACE, "a refusal returns the original trace");
        assert!(outcome.summary.starts_with("Not deobfuscated: "));
    }

    // ── Detection ────────────────────────────────────────────────────────────

    #[test]
    fn latest_is_preferred_over_versioned_tools() {
        let tmp = TempDir::new().unwrap();
        let sdk = tmp.path().canonicalize().unwrap();
        write_retrace(&sdk, "22.0", "");
        let latest = write_retrace(&sdk, "latest", "");
        std::fs::write(
            sdk.join("cmdline-tools/latest/source.properties"),
            "Pkg.Desc=Android SDK Command-line Tools\nPkg.Revision=19.0\n",
        )
        .unwrap();

        assert_eq!(
            find_retrace(&sdk),
            Some(RetraceTool {
                path: latest,
                version: Some("19.0".into()),
            })
        );
    }

    #[test]
    fn without_latest_the_highest_version_with_retrace_is_used() {
        let tmp = TempDir::new().unwrap();
        let sdk = tmp.path().canonicalize().unwrap();
        write_retrace(&sdk, "9.0", "");
        let newest = write_retrace(&sdk, "11.0", "");
        write_retrace(&sdk, "backup", "");
        // Newer, but without the tool.
        std::fs::create_dir_all(sdk.join("cmdline-tools/22.0/bin")).unwrap();

        assert_eq!(find_retrace(&sdk).map(|t| t.path), Some(newest));
    }

    #[test]
    fn a_missing_or_non_executable_tool_is_not_found() {
        let tmp = TempDir::new().unwrap();
        let sdk = tmp.path().canonicalize().unwrap();
        assert_eq!(find_retrace(&sdk), None);

        let path = sdk.join("cmdline-tools/latest/bin/retrace");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        assert_eq!(find_retrace(&sdk), None);
    }

    #[test]
    fn a_symlinked_tool_is_canonicalized() {
        let tmp = TempDir::new().unwrap();
        let sdk = tmp.path().canonicalize().unwrap();
        let real = write_retrace(&sdk, "22.0", "");
        std::os::unix::fs::symlink(
            sdk.join("cmdline-tools/22.0"),
            sdk.join("cmdline-tools/latest"),
        )
        .unwrap();

        let tool = find_retrace(&sdk).unwrap();
        assert_eq!(tool.path, real);
        assert_eq!(tool.path, tool.path.canonicalize().unwrap());
    }

    #[test]
    fn the_sdk_comes_from_settings_only() {
        let mut settings = AppSettings::default();
        assert_eq!(find_configured_retrace(&settings), None);
        let tmp = TempDir::new().unwrap();
        let sdk = tmp.path().canonicalize().unwrap();
        let tool = write_retrace(&sdk, "latest", "");
        settings.android.sdk_path = Some(sdk.to_string_lossy().into_owned());
        assert_eq!(
            find_configured_retrace(&settings).map(|t| t.path),
            Some(tool)
        );
    }

    // ── Running it ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_trace_is_deobfuscated_through_a_file_without_a_shell() {
        let fx = Fixture::new();
        let installed = fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());
        let marker = fx.root.join("injected");
        let trace = format!(
            "{TRACE}\tat a.b.c(SourceFile:2) $(touch {m}) ; touch {m} `touch {m}`\n",
            m = marker.display()
        );

        let outcome = fx.retrace(&trace).await;

        assert_eq!(outcome.status, RetraceStatus::Retraced, "{outcome:?}");
        assert!(outcome
            .trace
            .contains("at com.example.app.MainActivity.onCreate(MainActivity.kt:24)"));
        assert_eq!(outcome.build_id, Some(12));
        assert_eq!(outcome.mapping.as_ref(), installed.mappings.first());
        assert_eq!(outcome.matched_by, Some(MappingMatch::InstallRecord));
        assert_eq!(outcome.device.as_deref(), Some(SERIAL));
        assert!(
            outcome.summary.contains("build #12")
                && outcome.summary.contains(":app release, map id 6b1c2f0")
                && outcome.summary.contains("versionCode 42"),
            "{}",
            outcome.summary
        );

        let argv = std::fs::read_to_string(fx.record().with_extension("argv")).unwrap();
        let argv: Vec<&str> = argv.split_terminator('\0').collect();
        let mapping =
            mapping_snapshots::snapshot_path(&fx.data, &installed.mappings[0].sha256).unwrap();
        assert_eq!(argv.len(), 2, "{argv:?}");
        assert_eq!(argv[0], mapping.to_string_lossy());
        assert!(Path::new(argv[1]).starts_with(fx.data.join(RETRACE_TMP_DIR)));
        assert_eq!(
            std::fs::read_to_string(fx.record().with_extension("input")).unwrap(),
            trace
        );
        assert!(!marker.exists(), "the trace reached a shell");
        assert_eq!(
            std::fs::read_dir(fx.data.join(RETRACE_TMP_DIR))
                .unwrap()
                .count(),
            0,
            "the trace file was left behind"
        );
    }

    #[tokio::test]
    async fn a_second_request_is_answered_from_the_cache() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());

        let first = fx.retrace(TRACE).await;
        let second = fx.retrace(TRACE).await;

        assert_eq!(first.status, RetraceStatus::Retraced);
        assert_eq!(second, first);
        assert_eq!(fx.runs(), 1);
    }

    #[tokio::test]
    async fn a_retrace_that_hangs_times_out_and_leaves_no_file() {
        let mut fx = Fixture::new();
        fx.install_with_mapping();
        write_retrace(&fx.sdk, "latest", "exec sleep 30");
        fx.env.timeout = Duration::from_millis(300);

        let outcome = fx.retrace(TRACE).await;

        assert_eq!(outcome.status, RetraceStatus::Failed, "{outcome:?}");
        assert!(outcome
            .reason
            .as_deref()
            .unwrap()
            .contains("retrace timed out after 300 ms"));
        assert_eq!(outcome.trace, TRACE);
        assert_eq!(
            std::fs::read_dir(fx.data.join(RETRACE_TMP_DIR))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn a_failing_retrace_reports_its_error_and_leaves_no_file() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        write_retrace(
            &fx.sdk,
            "latest",
            "echo 'Error: java.nio.file.NoSuchFileException: x' >&2\nexit 1",
        );

        let outcome = fx.retrace(TRACE).await;

        assert_eq!(outcome.status, RetraceStatus::Failed);
        assert_eq!(
            outcome.reason.as_deref(),
            Some("retrace failed: Error: java.nio.file.NoSuchFileException: x")
        );
        assert_eq!(
            std::fs::read_dir(fx.data.join(RETRACE_TMP_DIR))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn output_over_the_cap_is_refused() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        write_retrace(
            &fx.sdk,
            "latest",
            &format!(
                "head -c {} /dev/zero | tr '\\0' a",
                MAX_RETRACE_OUTPUT_BYTES + 1
            ),
        );

        let outcome = fx.retrace(TRACE).await;

        assert_eq!(outcome.status, RetraceStatus::Failed);
        assert!(outcome.reason.unwrap().contains("more than the 1024 KiB"));
        assert_eq!(outcome.trace, TRACE);
    }

    #[tokio::test]
    async fn a_trace_over_the_cap_is_not_sent() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());
        let trace = "\tat a.a.onCreate(SourceFile:1)\n".repeat(MAX_RETRACE_INPUT_BYTES / 20);

        let outcome = fx.retrace(&trace).await;

        assert_eq!(outcome.status, RetraceStatus::Refused);
        assert!(outcome.reason.unwrap().contains("at most 256 KiB"));
        assert_eq!(fx.runs(), 0);
    }

    #[tokio::test]
    async fn without_the_tool_or_a_jdk_17_it_is_unavailable() {
        let mut fx = Fixture::new();
        fx.install_with_mapping();

        let outcome = fx.retrace(TRACE).await;
        assert_eq!(outcome.status, RetraceStatus::Unavailable);
        assert!(outcome
            .reason
            .unwrap()
            .contains("Install \"Android SDK Command-line Tools\""));

        transforming_retrace(&fx.sdk, &fx.record());
        let jdk11 = fake_jdk_home(&fx.root.join("jdk11"), "11.0.21");
        fx.env.settings.java.home = Some(jdk11.to_string_lossy().into_owned());
        let outcome = fx.retrace(TRACE).await;
        assert_eq!(outcome.status, RetraceStatus::Unavailable);
        assert!(
            outcome.reason.as_deref().unwrap().contains("is JDK 11"),
            "{outcome:?}"
        );

        fx.env.settings.java.home = Some(fx.root.join("no-jdk").to_string_lossy().into_owned());
        let outcome = fx.retrace(TRACE).await;
        assert_eq!(outcome.status, RetraceStatus::Unavailable);
        assert!(outcome.reason.unwrap().contains("no working Java"));
        assert_eq!(fx.runs(), 0);
    }

    // ── Choosing the mapping ─────────────────────────────────────────────────

    #[tokio::test]
    async fn without_an_install_record_nothing_is_deobfuscated() {
        let fx = Fixture::new();
        transforming_retrace(&fx.sdk, &fx.record());
        assert_refused(
            &fx.retrace(TRACE).await,
            "no record of installing com.example.app on R5CT1234ABC",
        );
        assert_eq!(fx.runs(), 0);
    }

    #[tokio::test]
    async fn an_install_without_a_build_or_a_mapping_is_refused() {
        let fx = Fixture::new();
        transforming_retrace(&fx.sdk, &fx.record());

        let mut unmatched = installed(vec![]);
        unmatched.build_id = None;
        fx.save_install(&unmatched);
        assert_refused(&fx.retrace(TRACE).await, "not written by a recorded build");

        fx.save_install(&installed(vec![]));
        assert_refused(
            &fx.retrace(TRACE).await,
            "no R8 mapping was recorded for this install of build #12",
        );
        assert_eq!(fx.runs(), 0);
    }

    #[tokio::test]
    async fn a_missing_snapshot_file_is_refused() {
        let fx = Fixture::new();
        transforming_retrace(&fx.sdk, &fx.record());
        let mapping = fx.save_mapping(MAPPING);
        std::fs::remove_file(mapping_snapshots::snapshot_path(&fx.data, &mapping.sha256).unwrap())
            .unwrap();
        fx.save_install(&installed(vec![mapping]));

        let outcome = fx.retrace(TRACE).await;

        assert_refused(&outcome, "is missing from Keynobi's data directory");
        assert_eq!(outcome.build_id, Some(12));
        assert_eq!(fx.runs(), 0);
    }

    fn record_with_apk(id: u32, apk: BuiltApk) -> BuildRecord {
        BuildRecord {
            id,
            task: "assembleRelease".into(),
            status: BuildStatus::Idle,
            errors: vec![],
            started_at: Utc::now().to_rfc3339(),
            project_root: None,
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings: vec![],
            apks: vec![apk],
        }
    }

    #[test]
    fn several_mappings_are_narrowed_to_the_installed_variant_or_refused() {
        let fx = Fixture::new();
        let release = fx.save_mapping(b"release mapping");
        let mut staging = fx.save_mapping(b"staging mapping");
        staging.variant = "staging".into();
        let entry = installed(vec![release.clone(), staging.clone()]);

        let err = choose_mapping(&fx.data, &entry).unwrap_err();
        assert!(err.contains("several R8 mappings"), "{err}");
        assert!(
            err.contains(":app release") && err.contains(":app staging"),
            "{err}"
        );

        let apk = BuiltApk {
            module: ":app".into(),
            variant: "Staging".into(),
            application_id: Some(PACKAGE.into()),
            version_code: Some(42),
            sha256: entry.apk_sha256.clone(),
            bytes: 1,
            path: "app/build/outputs/apk/staging/app-staging.apk".into(),
        };
        std::fs::write(
            fx.data.join("build-history.json"),
            serde_json::to_string(&vec![record_with_apk(12, apk.clone())]).unwrap(),
        )
        .unwrap();
        assert_eq!(choose_mapping(&fx.data, &entry), Ok(staging));

        // Another build's record does not decide for build #12.
        std::fs::write(
            fx.data.join("build-history.json"),
            serde_json::to_string(&vec![record_with_apk(13, apk)]).unwrap(),
        )
        .unwrap();
        assert!(choose_mapping(&fx.data, &entry).is_err());
    }

    #[test]
    fn one_present_mapping_among_several_listed_is_used() {
        let fx = Fixture::new();
        let kept = fx.save_mapping(b"kept");
        let gone = fx.save_mapping(b"gone");
        std::fs::remove_file(mapping_snapshots::snapshot_path(&fx.data, &gone.sha256).unwrap())
            .unwrap();
        assert_eq!(
            choose_mapping(&fx.data, &installed(vec![gone, kept.clone()])),
            Ok(kept)
        );
    }

    // ── Checking the device ──────────────────────────────────────────────────

    #[tokio::test]
    async fn an_update_after_keynobis_install_is_refused() {
        let fx = Fixture::new();
        let installed = fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());
        let installed_at = DateTime::parse_from_rfc3339(&installed.installed_at)
            .unwrap()
            .with_timezone(&Utc);
        fx.write_adb(&dumpsys(42, installed_at + TimeDelta::minutes(5)));

        let outcome = fx.retrace(TRACE).await;

        assert_refused(
            &outcome,
            "the app was reinstalled outside Keynobi after build #12",
        );
        assert_eq!(outcome.mapping.as_ref(), installed.mappings.first());
        assert_eq!(fx.runs(), 0);
    }

    #[tokio::test]
    async fn a_different_version_code_is_refused() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());
        fx.write_adb(&dumpsys(43, Utc::now() - TimeDelta::minutes(20)));

        assert_refused(
            &fx.retrace(TRACE).await,
            "runs versionCode 43, Keynobi installed versionCode 42",
        );
        assert_eq!(fx.runs(), 0);
    }

    #[tokio::test]
    async fn a_device_that_cannot_be_asked_is_refused() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());
        fx.write_adb_script("echo 'error: device offline' >&2\nexit 1");

        assert_refused(
            &fx.retrace(TRACE).await,
            "could not check that com.example.app on R5CT1234ABC is still build #12 \
             (adb shell dumpsys failed: error: device offline)",
        );

        fx.write_adb("Packages:\n");
        assert_refused(&fx.retrace(TRACE).await, "is no longer installed");
        assert_eq!(fx.runs(), 0);
    }

    #[tokio::test]
    async fn the_package_is_quoted_for_the_device_shell() {
        let fx = Fixture::new();
        let (adb, record) =
            crate::utils::device_shell::test_support::fake_adb(&fx.sdk.join("platform-tools"));
        let mut hostile = installed(vec![]);
        hostile.package = "com.example;reboot $(id)".into();

        let _ = check_device(&adb, SERIAL, SERIAL, &hostile).await;

        let calls = crate::utils::device_shell::test_support::recorded_calls(&record);
        assert_eq!(calls[0], ["dumpsys", "package", "com.example;reboot $(id)"]);
        assert_eq!(calls[1], ["date", "+%s:%z"]);
    }

    #[test]
    fn health_reports_the_tool_and_its_version() {
        let tmp = TempDir::new().unwrap();
        let sdk = tmp.path().canonicalize().unwrap();
        let mut settings = AppSettings::default();
        settings.android.sdk_path = Some(sdk.to_string_lossy().into_owned());
        assert_eq!(health_version(&settings), None);
        assert_eq!(health_json(&settings)["ok"], false);
        assert!(health_json(&settings)["hint"]
            .as_str()
            .unwrap()
            .contains("Android SDK Command-line Tools"));

        write_retrace(&sdk, "latest", "");
        assert_eq!(health_version(&settings).as_deref(), Some("unknown"));
        std::fs::write(
            sdk.join("cmdline-tools/latest/source.properties"),
            "Pkg.Revision=22.0\n",
        )
        .unwrap();
        assert_eq!(health_version(&settings).as_deref(), Some("22.0"));
        assert_eq!(health_json(&settings)["version"], "22.0");
        assert_eq!(health_json(&settings)["hint"], Value::Null);
    }

    #[test]
    fn dumpsys_output_is_read_from_the_package_block_only() {
        let text = "Packages:\n  Package [com.other] (1):\n    versionCode=7 minSdk=21\n    \
                    lastUpdateTime=2020-01-01 00:00:00\n  Package [com.example.app] (2):\n    \
                    versionCode=42 minSdk=24 targetSdk=34\n    \
                    lastUpdateTime=2026-09-25 10:32:01\n    User 0: installed=true\n  \
                    Package [com.later] (3):\n    versionCode=9\n";
        assert_eq!(
            parse_dumpsys_package(text, PACKAGE),
            Some(DevicePackage {
                version_code: Some(42),
                last_update: Some("2026-09-25 10:32:01".into()),
            })
        );
        assert_eq!(parse_dumpsys_package(text, "com.missing"), None);
        assert_eq!(
            parse_dumpsys_package(
                "  Package [com.example.app] (2):\n  Package [x]:\n    versionCode=1\n",
                PACKAGE
            ),
            Some(DevicePackage {
                version_code: None,
                last_update: None,
            })
        );
    }

    #[test]
    fn the_device_clock_and_offset_are_parsed() {
        assert_eq!(
            parse_device_clock("1790377228:-0400\n"),
            Some((1_790_377_228, FixedOffset::west_opt(4 * 3600).unwrap()))
        );
        assert_eq!(
            parse_device_clock("1790377228:+0530"),
            Some((
                1_790_377_228,
                FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap()
            ))
        );
        assert_eq!(parse_device_clock("1790377228"), None);
        assert_eq!(parse_device_clock("1790377228:EDT"), None);
    }

    #[test]
    fn the_update_time_is_compared_in_the_devices_zone_and_clock() {
        let installed_at = DateTime::parse_from_rfc3339("2026-09-25T14:32:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let host_now = installed_at + TimeDelta::minutes(10);
        let local = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap();
        let edt = FixedOffset::west_opt(4 * 3600).unwrap();
        let in_sync = host_now.timestamp();

        // 10:31:58 EDT is 14:31:58 UTC: the install itself.
        assert_eq!(
            updated_after_install(
                local("2026-09-25 10:31:58"),
                edt,
                in_sync,
                host_now,
                installed_at
            ),
            Some(false)
        );
        // Five minutes after the install.
        assert_eq!(
            updated_after_install(
                local("2026-09-25 10:37:00"),
                edt,
                in_sync,
                host_now,
                installed_at
            ),
            Some(true)
        );
        // A device clock five minutes fast moves its times back by as much.
        assert_eq!(
            updated_after_install(
                local("2026-09-25 10:37:00"),
                edt,
                in_sync + 300,
                host_now,
                installed_at
            ),
            Some(false)
        );
    }

    // ── Devices ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_crash_from_an_unknown_device_is_refused() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        let outcome = retrace_trace(&fx.env, &EntryDevice::Unknown, Some(PACKAGE), TRACE).await;
        assert_refused(&outcome, "no longer knows which device");

        let outcome =
            retrace_trace(&fx.env, &EntryDevice::Serial(SERIAL.into()), None, TRACE).await;
        assert_refused(&outcome, "did not attribute the crash to a package");
    }

    #[tokio::test]
    async fn an_unnamed_stream_uses_the_only_online_device() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        transforming_retrace(&fx.sdk, &fx.record());
        let outcome = retrace_trace(&fx.env, &EntryDevice::Unnamed, Some(PACKAGE), TRACE).await;
        assert_eq!(outcome.status, RetraceStatus::Retraced, "{outcome:?}");
        assert_eq!(outcome.device.as_deref(), Some(SERIAL));
    }

    #[tokio::test]
    async fn an_emulator_without_an_avd_name_is_refused() {
        let fx = Fixture::new();
        fx.install_with_mapping();
        let outcome = retrace_trace(
            &fx.env,
            &EntryDevice::Serial("emulator-5554".into()),
            Some(PACKAGE),
            TRACE,
        )
        .await;
        assert_refused(&outcome, "emulator-5554 did not report its AVD name");
    }

    #[tokio::test]
    async fn a_crash_group_is_read_from_the_buffer_with_its_device() {
        use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel, ProcessedEntry};
        let state = crate::commands::logcat::new_logcat_state();
        let entry = |id: u64, message: &str, group: Option<u64>| ProcessedEntry {
            id,
            timestamp: "09-25 10:32:01.000".into(),
            pid: 1234,
            tid: 1234,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: message.into(),
            package: Some(PACKAGE.into()),
            kind: LogcatKind::Normal,
            is_crash: group.is_some(),
            flags: 0,
            category: EntryCategory::General,
            crash_group_id: group,
            json_body: None,
        };
        {
            let mut s = state.lock().await;
            s.record_stream_start(Some("first".into()));
            s.store
                .push(entry(1, "java.lang.RuntimeException: old", Some(1)));
            for _ in 0..10 {
                s.ids.next_entry_id();
            }
            s.record_stream_start(Some(SERIAL.into()));
            let first = s.ids.peek_next_entry_id();
            s.store
                .push(entry(first + 1, "\tat a.a.onCreate(SourceFile:1)", Some(2)));
            s.store
                .push(entry(first, "java.lang.RuntimeException: boom", Some(2)));
            s.store.push(entry(first + 2, "unrelated", None));
        }

        let crash = crash_trace(&state, 2).await.unwrap();
        assert_eq!(
            crash.trace,
            "java.lang.RuntimeException: boom\n\tat a.a.onCreate(SourceFile:1)\n"
        );
        assert_eq!(crash.device, EntryDevice::Serial(SERIAL.into()));
        assert_eq!(crash.package.as_deref(), Some(PACKAGE));
        assert_eq!(
            crash_trace(&state, 1).await.unwrap().device,
            EntryDevice::Serial("first".into())
        );
        assert!(crash_trace(&state, 3).await.is_none());
    }
}
