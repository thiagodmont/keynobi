//! Why an app's processes exited.
//!
//! Android 11 (API 30) and later keep a per-package history of process exits
//! (`ApplicationExitInfo`): crashes, native crashes, ANRs, low-memory kills,
//! kills by the user or the system, and so on. `dumpsys activity exit-info
//! <package>` prints it. The history shows exits that never reached the
//! logcat buffer (logcat was not running, or the buffer rolled over).
//!
//! The dump is printed by `AppExitInfoTracker.dumpHistoryProcessExitInfo` and
//! `ApplicationExitInfo.dump`:
//!
//! ```text
//! ACTIVITY MANAGER PROCESS EXIT INFO (dumpsys activity exit-info)
//! Last Timestamp of Persistence Into Persistent Storage: 2024-05-02 10:20:00.000
//!   package: com.example.app
//!     Historical Process Exit for uid=10152
//!         ApplicationExitInfo #0:
//!           timestamp=2024-05-02 10:15:03.482 pid=12345 realUid=10152 packageUid=10152 definingUid=10152 user=0
//!           process=com.example.app reason=4 (APP CRASH(EXCEPTION)) subreason=0 (UNKNOWN) status=0
//!           importance=100 pss=55MB rss=127MB description=crash state=empty trace=null
//! ```
//!
//! The parser is tolerant: it reads `key=value` pairs wherever they appear in
//! a record, ignores keys it does not know, and leaves missing fields `None`.

use crate::models::app_exit::{AppExitReason, AppExitReasons, AppExitRecord};
use crate::models::error::AppError;
use crate::utils::device_shell::quote_device_shell_arg;
use crate::utils::process::{
    describe_failure, output_with_timeout, ADB_QUERY_TIMEOUT, ADB_UNRESPONSIVE_HINT,
};
use crate::utils::validation::{validate_device_serial, validate_package_name};
use std::path::Path;
use tokio::process::Command;

/// The first API level with `ApplicationExitInfo` (Android 11).
pub const MIN_EXIT_INFO_API: u32 = 30;
/// Records returned, newest first. Android keeps 16 per package and user by default.
pub const MAX_EXIT_RECORDS: usize = 100;
/// Characters kept of one record's description.
pub const MAX_EXIT_DESCRIPTION_CHARS: usize = 500;
/// Bytes of `dumpsys` output parsed; the rest is ignored.
pub const MAX_EXIT_INFO_OUTPUT_BYTES: usize = 1024 * 1024;
/// Records kept while parsing, before sorting and the [`MAX_EXIT_RECORDS`] cap.
const MAX_PARSED_EXIT_RECORDS: usize = 2_000;

/// Keys that can follow `description=` on its line. The description is free
/// text, so any other `key=` inside it is part of the description.
const DESCRIPTION_FOLLOWERS: &[&str] = &["state", "trace"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitInfoError {
    /// The caller must fix the request: a bad serial or package, or no
    /// package given where the project does not name exactly one.
    InvalidInput(String),
    /// adb or the device failed.
    Failed(String),
}

impl std::fmt::Display for ExitInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExitInfoError::InvalidInput(m) | ExitInfoError::Failed(m) => f.write_str(m),
        }
    }
}

impl From<ExitInfoError> for AppError {
    fn from(e: ExitInfoError) -> Self {
        match e {
            ExitInfoError::InvalidInput(m) => AppError::InvalidInput(m),
            ExitInfoError::Failed(m) => AppError::ProcessFailed(m),
        }
    }
}

/// Read the exit history of `requested_package` on `serial`, or of the open
/// project's app when no package is given (see [`resolve_package`]).
///
/// The serial and package are validated before any adb call. Below Android 11
/// the result says so (`supported: false`) and `dumpsys` is not run.
pub async fn read_exit_reasons(
    adb: &Path,
    serial: &str,
    gradle_root: Option<&Path>,
    requested_package: Option<&str>,
) -> Result<AppExitReasons, ExitInfoError> {
    validate_device_serial(serial).map_err(ExitInfoError::InvalidInput)?;
    if let Some(package) = requested_package {
        validate_package_name(package).map_err(ExitInfoError::InvalidInput)?;
    }

    let package = resolve_package(adb, serial, gradle_root, requested_package).await?;
    // A default comes from the project's build files, which are untrusted too.
    validate_package_name(&package).map_err(ExitInfoError::InvalidInput)?;
    let api_level = device_api_level(adb, serial).await?;

    if let Some(api) = api_level.filter(|api| *api < MIN_EXIT_INFO_API) {
        return Ok(unsupported(
            serial,
            &package,
            Some(api),
            format!(
                "Process exit reasons need Android 11 (API {MIN_EXIT_INFO_API}) or later; \
                 {serial} runs API {api}."
            ),
        ));
    }

    let output = output_with_timeout(
        Command::new(adb).args([
            "-s",
            serial,
            "shell",
            "dumpsys",
            "activity",
            "exit-info",
            &quote_device_shell_arg(&package),
        ]),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| {
        ExitInfoError::Failed(describe_failure(
            "adb dumpsys activity exit-info",
            &e,
            ADB_UNRESPONSIVE_HINT,
        ))
    })?;
    if !output.status.success() {
        return Err(ExitInfoError::Failed(failure_message(
            "adb dumpsys activity exit-info",
            &output,
        )));
    }

    let text = capped_text(&output.stdout);
    if reports_unknown_command(&text) {
        return Ok(unsupported(
            serial,
            &package,
            api_level,
            format!(
                "{serial} does not report process exit reasons: they need Android 11 \
                 (API {MIN_EXIT_INFO_API}) or later."
            ),
        ));
    }

    let mut records = parse_exit_info(&text);
    let total_records = u32::try_from(records.len()).unwrap_or(u32::MAX);
    sort_newest_first(&mut records);
    records.truncate(MAX_EXIT_RECORDS);
    let message = records
        .is_empty()
        .then(|| format!("No process exits are recorded for {package} on {serial}."));

    Ok(AppExitReasons {
        serial: serial.to_string(),
        package,
        api_level,
        supported: true,
        message,
        records,
        total_records,
    })
}

/// The package to read: `requested`, else the open project's app.
///
/// The project must name exactly one application id. When it does, the one
/// build of it installed on the device (`com.example.app.debug`) is used, since
/// the history is kept per installed package; with none installed, the id
/// itself. Several application ids, or several installed builds, need the
/// caller to name the package.
pub async fn resolve_package(
    adb: &Path,
    serial: &str,
    gradle_root: Option<&Path>,
    requested: Option<&str>,
) -> Result<String, ExitInfoError> {
    if let Some(package) = requested {
        return Ok(package.to_string());
    }
    let Some(root) = gradle_root else {
        return Err(ExitInfoError::InvalidInput(
            "No project is open, so there is no default app: pass the package to read.".into(),
        ));
    };
    let root = root.to_path_buf();
    let scope = tokio::task::spawn_blocking(move || {
        crate::services::build_inspector::project_package_scope(&root)
    })
    .await
    .map_err(|e| ExitInfoError::Failed(format!("Could not read the project's build files: {e}")))?;

    let base = match scope.application_ids.as_slice() {
        [only] => only.clone(),
        [] => {
            return Err(ExitInfoError::InvalidInput(
                "The open project's applicationId could not be found in its build files: \
                 pass the package to read."
                    .into(),
            ))
        }
        several => {
            return Err(ExitInfoError::InvalidInput(format!(
                "The open project has several application ids ({}): pass the package to read.",
                several.join(", ")
            )))
        }
    };
    validate_package_name(&base).map_err(|e| {
        ExitInfoError::InvalidInput(format!(
            "The open project's applicationId cannot be read on a device: {e}"
        ))
    })?;

    let installed = crate::services::adb_manager::installed_variant_packages(adb, serial, &base)
        .await
        .map_err(ExitInfoError::Failed)?;
    match installed.as_slice() {
        [] => Ok(base),
        [only] => Ok(only.clone()),
        several => Err(ExitInfoError::InvalidInput(format!(
            "Several builds of {base} are installed on {serial} ({}): pass the package to read.",
            several.join(", ")
        ))),
    }
}

async fn device_api_level(adb: &Path, serial: &str) -> Result<Option<u32>, ExitInfoError> {
    let output = output_with_timeout(
        Command::new(adb).args(["-s", serial, "shell", "getprop", "ro.build.version.sdk"]),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| {
        ExitInfoError::Failed(describe_failure("adb getprop", &e, ADB_UNRESPONSIVE_HINT))
    })?;
    if !output.status.success() {
        return Err(ExitInfoError::Failed(failure_message(
            "adb getprop",
            &output,
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().parse().ok())
}

fn failure_message(what: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    format!("{what} failed: {detail}")
}

fn unsupported(
    serial: &str,
    package: &str,
    api_level: Option<u32>,
    message: String,
) -> AppExitReasons {
    AppExitReasons {
        serial: serial.to_string(),
        package: package.to_string(),
        api_level,
        supported: false,
        message: Some(message),
        records: Vec::new(),
        total_records: 0,
    }
}

/// At most [`MAX_EXIT_INFO_OUTPUT_BYTES`] of `stdout`, invalid UTF-8 replaced.
fn capped_text(stdout: &[u8]) -> String {
    let end = stdout.len().min(MAX_EXIT_INFO_OUTPUT_BYTES);
    String::from_utf8_lossy(&stdout[..end]).into_owned()
}

/// Whether `dumpsys activity` did not know the `exit-info` command (before
/// Android 11 it reads it as an activity name).
fn reports_unknown_command(text: &str) -> bool {
    !text.contains("ApplicationExitInfo")
        && (text.contains("Bad activity command") || text.contains("Unknown command"))
}

/// Newest first when the timestamps can be read; otherwise the dump's order,
/// which is newest first within each uid.
fn sort_newest_first(records: &mut [AppExitRecord]) {
    records.sort_by(|a, b| b.timestamp_local.cmp(&a.timestamp_local));
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse the records of `dumpsys activity exit-info` output, in dump order.
/// Never fails: lines it does not understand are skipped.
pub fn parse_exit_info(output: &str) -> Vec<AppExitRecord> {
    let mut records = Vec::new();
    let mut current: Option<RecordBuilder> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ApplicationExitInfo") {
            finish(&mut records, current.take());
            current = Some(RecordBuilder::default());
            continue;
        }
        if trimmed.starts_with("package:")
            || trimmed.starts_with("Historical Process Exit")
            || trimmed.starts_with("ACTIVITY MANAGER")
        {
            finish(&mut records, current.take());
            continue;
        }
        if let Some(record) = current.as_mut() {
            record.read_line(trimmed);
        }
    }
    finish(&mut records, current);
    records
}

fn finish(records: &mut Vec<AppExitRecord>, record: Option<RecordBuilder>) {
    if let Some(record) = record {
        if records.len() < MAX_PARSED_EXIT_RECORDS {
            records.push(record.build());
        }
    }
}

#[derive(Default)]
struct RecordBuilder {
    record: RecordFields,
    /// The last key read was `description`, so a line without a leading key
    /// continues it (the description contained a line break).
    in_description: bool,
}

#[derive(Default)]
struct RecordFields {
    timestamp: Option<String>,
    pid: Option<u32>,
    process_name: Option<String>,
    reason_code: Option<i32>,
    reason_label: Option<String>,
    sub_reason_code: Option<i32>,
    sub_reason: Option<String>,
    status: Option<i32>,
    importance: Option<i32>,
    pss_kb: Option<u64>,
    rss_kb: Option<u64>,
    description: Option<String>,
}

impl RecordBuilder {
    fn read_line(&mut self, line: &str) {
        let (leading, fields) = split_fields(line, self.in_description);
        if self.in_description && !leading.is_empty() {
            let description = self.record.description.get_or_insert_with(String::new);
            if !description.is_empty() {
                description.push(' ');
            }
            description.push_str(leading);
        }
        for (key, value) in fields {
            self.in_description = key == "description";
            self.set(key, value);
        }
    }

    fn set(&mut self, key: &str, value: &str) {
        let r = &mut self.record;
        match key {
            "timestamp" => r.timestamp = non_empty(value),
            "pid" => r.pid = leading_int(value),
            "process" => r.process_name = non_empty(value),
            "reason" => (r.reason_code, r.reason_label) = code_and_label(value),
            "subreason" => (r.sub_reason_code, r.sub_reason) = code_and_label(value),
            "status" => r.status = leading_int(value),
            "importance" => r.importance = leading_int(value),
            "pss" => r.pss_kb = size_kb(value),
            "rss" => r.rss_kb = size_kb(value),
            "description" => r.description = non_empty(value).filter(|d| d != "null"),
            _ => {}
        }
    }

    fn build(self) -> AppExitRecord {
        let r = self.record;
        let timestamp_local = r.timestamp.as_deref().and_then(parse_device_time);
        AppExitRecord {
            timestamp: r.timestamp,
            timestamp_local,
            pid: r.pid,
            process_name: r.process_name,
            reason: r
                .reason_code
                .map(AppExitReason::from_code)
                .unwrap_or(AppExitReason::Unknown),
            reason_code: r.reason_code,
            reason_label: r.reason_label,
            sub_reason_code: r.sub_reason_code,
            sub_reason: r.sub_reason,
            status: r.status,
            importance_name: r.importance.and_then(importance_name).map(str::to_string),
            importance: r.importance,
            pss_kb: r.pss_kb,
            rss_kb: r.rss_kb,
            description: r.description.map(cap_description),
        }
    }
}

/// Split `line` into the text before its first `key=` and its `key=value`
/// pairs. A key starts a word and is an identifier followed by `=`. While in
/// a description (from `description=` on, or from the line start when
/// `in_description`), only [`DESCRIPTION_FOLLOWERS`] end it.
fn split_fields(line: &str, mut in_description: bool) -> (&str, Vec<(&str, &str)>) {
    let bytes = line.as_bytes();
    // (key start, value start, key)
    let mut keys: Vec<(usize, usize, &str)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let word_start = i == 0 || bytes[i - 1].is_ascii_whitespace();
        if word_start && bytes[i].is_ascii_alphabetic() {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                let key = &line[i..j];
                if !in_description || DESCRIPTION_FOLLOWERS.contains(&key) {
                    in_description = key == "description";
                    keys.push((i, j + 1, key));
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    let leading = line[..keys.first().map_or(line.len(), |k| k.0)].trim();
    let fields = keys
        .iter()
        .enumerate()
        .map(|(n, &(_, value_start, key))| {
            let end = keys.get(n + 1).map_or(line.len(), |next| next.0);
            (key, line[value_start..end].trim())
        })
        .collect();
    (leading, fields)
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// The integer a value starts with: `4` in `4 (APP CRASH(EXCEPTION))`.
fn leading_int<T: std::str::FromStr>(value: &str) -> Option<T> {
    value.split_whitespace().next()?.parse().ok()
}

/// `4 (APP CRASH(EXCEPTION))` → `(Some(4), Some("APP CRASH(EXCEPTION)"))`.
fn code_and_label(value: &str) -> (Option<i32>, Option<String>) {
    let code = leading_int(value);
    let label = match (value.find('('), value.rfind(')')) {
        (Some(open), Some(close)) if close > open => non_empty(&value[open + 1..close]),
        _ => None,
    };
    (code, label)
}

/// A size as `DebugUtils.sizeValueToString` prints it (`0`, `9216KB`,
/// `55MB`, `1GB`: bytes scaled down while at least 10 KiB, then truncated),
/// in KB.
fn size_kb(value: &str) -> Option<u64> {
    let value = value.split_whitespace().next()?;
    let digits_end = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let number: u64 = value[..digits_end].parse().ok()?;
    let factor_kb: u64 = match value[digits_end..].to_ascii_uppercase().as_str() {
        "" | "B" => return Some(number / 1024),
        "K" | "KB" => 1,
        "M" | "MB" => 1024,
        "G" | "GB" => 1024 * 1024,
        "T" | "TB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    number.checked_mul(factor_kb)
}

/// ISO 8601 local time for a device timestamp printed as
/// `yyyy-MM-dd HH:mm:ss[.SSS]`. Other forms (a locale's short date) are
/// ambiguous and left unparsed.
fn parse_device_time(timestamp: &str) -> Option<String> {
    ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"]
        .iter()
        .find_map(|format| chrono::NaiveDateTime::parse_from_str(timestamp.trim(), format).ok())
        .map(|time| time.format("%Y-%m-%dT%H:%M:%S%.3f").to_string())
}

/// `ActivityManager.RunningAppProcessInfo` importance names.
fn importance_name(importance: i32) -> Option<&'static str> {
    Some(match importance {
        100 => "foreground",
        125 => "foreground service",
        150 | 325 => "top sleeping",
        200 => "visible",
        230 => "perceptible",
        300 => "service",
        350 => "can't save state",
        400 => "cached",
        1000 => "gone",
        _ => return None,
    })
}

fn cap_description(description: String) -> String {
    if description.chars().count() <= MAX_EXIT_DESCRIPTION_CHARS {
        return description;
    }
    let mut capped: String = description
        .chars()
        .take(MAX_EXIT_DESCRIPTION_CHARS)
        .collect();
    capped.push('…');
    capped
}

// ── Agent summary ─────────────────────────────────────────────────────────────

/// Compact text for an agent: newest first, at most `limit` records, each
/// with its reason, time, and description, and where to find a crash's stack.
pub fn agent_summary(result: &AppExitReasons, limit: usize) -> String {
    let api = result
        .api_level
        .map(|api| format!(" (API {api})"))
        .unwrap_or_default();
    let mut out = format!("{} on {}{api}: ", result.package, result.serial);
    if result.records.is_empty() {
        out.push_str(
            result
                .message
                .as_deref()
                .unwrap_or("no process exits recorded."),
        );
        return out;
    }

    let shown = result.records.len().min(limit);
    out.push_str(&format!(
        "{} process exit{} recorded, newest first",
        result.total_records,
        if result.total_records == 1 { "" } else { "s" }
    ));
    if shown < result.total_records as usize {
        out.push_str(&format!(
            " (showing {shown}; pass limit, up to {MAX_EXIT_RECORDS}, for more)"
        ));
    }
    out.push_str(".\n");

    for (n, record) in result.records.iter().take(shown).enumerate() {
        out.push_str(&format!("{}. {}\n", n + 1, record_line(record)));
        if let Some(description) = &record.description {
            out.push_str(&format!("   description: {description}\n"));
        }
    }

    let has_crash = result.records.iter().take(shown).any(|r| {
        matches!(
            r.reason,
            AppExitReason::Crash | AppExitReason::CrashNative | AppExitReason::Anr
        )
    });
    if has_crash {
        out.push_str(&format!(
            "For a crash or ANR stack, call get_crash_logs or get_crash_stack_trace with \
             package {}: they read Keynobi's logcat buffer, so they only have crashes that \
             happened while logcat was running (start_logcat).",
            result.package
        ));
    }
    out.trim_end().to_string()
}

fn record_line(record: &AppExitRecord) -> String {
    let mut parts = vec![format!(
        "{} — {}",
        record.timestamp.as_deref().unwrap_or("time unknown"),
        record.reason.name()
    )];
    if let Some(label) = &record.reason_label {
        parts[0].push_str(&format!(" ({label})"));
    }
    if let Some(sub) = record
        .sub_reason
        .as_deref()
        .filter(|_| record.sub_reason_code.unwrap_or(0) != 0)
    {
        parts.push(format!("sub-reason {sub}"));
    }
    if let Some(status) = status_text(record) {
        parts.push(status);
    }
    match (&record.process_name, record.pid) {
        (Some(name), Some(pid)) => parts.push(format!("process {name} (pid {pid})")),
        (Some(name), None) => parts.push(format!("process {name}")),
        (None, Some(pid)) => parts.push(format!("pid {pid}")),
        (None, None) => {}
    }
    if let Some(importance) = &record.importance_name {
        parts.push(format!("was {importance}"));
    }
    match (record.pss_kb, record.rss_kb) {
        (Some(pss), Some(rss)) if pss > 0 || rss > 0 => {
            parts.push(format!("pss {} rss {}", format_kb(pss), format_kb(rss)))
        }
        _ => {}
    }
    parts.join(" · ")
}

fn status_text(record: &AppExitRecord) -> Option<String> {
    let status = record.status?;
    match record.reason {
        AppExitReason::Signaled | AppExitReason::CrashNative if status > 0 => {
            Some(match signal_name(status) {
                Some(name) => format!("signal {status} ({name})"),
                None => format!("signal {status}"),
            })
        }
        AppExitReason::ExitSelf => Some(format!("exit status {status}")),
        _ if status != 0 => Some(format!("status {status}")),
        _ => None,
    }
}

fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        15 => "SIGTERM",
        _ => return None,
    })
}

fn format_kb(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{:.1} GB", kb as f64 / (1024.0 * 1024.0))
    } else if kb >= 1024 {
        format!("{} MB", kb / 1024)
    } else {
        format!("{kb} KB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../tests/fixtures/exit_info/", $name))
        };
    }

    fn reasons(records: &[AppExitRecord]) -> Vec<AppExitReason> {
        records.iter().map(|r| r.reason).collect()
    }

    // ── Parser, per Android version ──────────────────────────────────────────

    #[test]
    fn parses_android_11() {
        let records = parse_exit_info(fixture!("android11.txt"));

        assert_eq!(
            reasons(&records),
            [
                AppExitReason::Crash,
                AppExitReason::Anr,
                AppExitReason::LowMemory,
                AppExitReason::ExitSelf
            ]
        );
        let crash = &records[0];
        assert_eq!(crash.timestamp.as_deref(), Some("2021-03-12 09:31:11.536"));
        assert_eq!(
            crash.timestamp_local.as_deref(),
            Some("2021-03-12T09:31:11.536")
        );
        assert_eq!(crash.pid, Some(8123));
        assert_eq!(crash.process_name.as_deref(), Some("com.example.app"));
        assert_eq!(crash.reason_code, Some(4));
        assert_eq!(crash.reason_label.as_deref(), Some("APP CRASH(EXCEPTION)"));
        assert_eq!(crash.sub_reason_code, Some(0));
        assert_eq!(crash.sub_reason.as_deref(), Some("UNKNOWN"));
        assert_eq!(crash.status, Some(0));
        assert_eq!(crash.importance, Some(100));
        assert_eq!(crash.importance_name.as_deref(), Some("foreground"));
        assert_eq!(crash.pss_kb, Some(55 * 1024));
        assert_eq!(crash.rss_kb, Some(127 * 1024));
        assert_eq!(crash.description.as_deref(), Some("crash"));

        // The description runs to `state=`, parentheses and `hasFocus=true` included.
        assert_eq!(
            records[1].description.as_deref(),
            Some(
                "Input dispatching timed out (8f2c1a3 com.example.app/com.example.app.MainActivity \
                 (server) is not responding. Waited 5001ms for FocusEvent(hasFocus=true))"
            )
        );
        assert_eq!(records[2].description, None, "description=null");
        assert_eq!((records[2].pss_kb, records[2].rss_kb), (Some(0), Some(0)));
        assert_eq!(records[2].importance_name.as_deref(), Some("cached"));
        assert_eq!(records[3].pss_kb, Some(9216));
    }

    #[test]
    fn parses_android_12() {
        let records = parse_exit_info(fixture!("android12.txt"));

        assert_eq!(
            reasons(&records),
            [
                AppExitReason::UserRequested,
                AppExitReason::Signaled,
                AppExitReason::Other
            ]
        );
        assert_eq!(
            records[1].process_name.as_deref(),
            Some("com.example.app.debug:sync")
        );
        assert_eq!(records[1].status, Some(9));
        assert_eq!(records[2].sub_reason_code, Some(1));
        assert_eq!(records[2].sub_reason.as_deref(), Some("TOO MANY CACHED"));
        assert_eq!(records[2].description.as_deref(), Some("cached #17"));
    }

    #[test]
    fn parses_android_13() {
        let records = parse_exit_info(fixture!("android13.txt"));

        assert_eq!(
            reasons(&records),
            [
                AppExitReason::CrashNative,
                AppExitReason::DependencyDied,
                AppExitReason::ExcessiveResourceUsage
            ]
        );
        assert_eq!(records[0].status, Some(11));
        assert_eq!(
            records[1].importance_name.as_deref(),
            Some("foreground service")
        );
        // `dur=` and `limit=` inside the description do not end it.
        assert_eq!(
            records[2].description.as_deref(),
            Some("excessive cpu 12450 during 300000 dur=1200000 limit=2")
        );
    }

    #[test]
    fn parses_android_14_with_two_users() {
        let records = parse_exit_info(fixture!("android14.txt"));

        assert_eq!(
            reasons(&records),
            [
                AppExitReason::Freezer,
                AppExitReason::Crash,
                AppExitReason::Anr
            ]
        );
        assert_eq!(
            records[0].sub_reason.as_deref(),
            Some("FREEZER BINDER IOCTL")
        );
        assert_eq!(records[1].pss_kb, Some(1024 * 1024));
        assert_eq!(records[1].rss_kb, Some(2 * 1024 * 1024));
        assert_eq!(records[2].pid, Some(30577));
    }

    #[test]
    fn parses_android_15() {
        let records = parse_exit_info(fixture!("android15.txt"));

        assert_eq!(
            reasons(&records),
            [AppExitReason::PackageUpdated, AppExitReason::Crash]
        );
        assert_eq!(records[0].sub_reason.as_deref(), Some("PACKAGE UPDATE"));
    }

    #[test]
    fn parses_unknown_codes_keys_and_broken_records_without_failing() {
        let records = parse_exit_info(fixture!("odd_fields.txt"));

        assert_eq!(records.len(), 3);
        let first = &records[0];
        assert_eq!(first.reason, AppExitReason::Unknown);
        assert_eq!(first.reason_code, Some(17));
        assert_eq!(first.reason_label.as_deref(), Some("SOMETHING NEW"));
        assert_eq!(first.sub_reason.as_deref(), Some("NEWER SUBREASON"));
        assert_eq!(
            first.status,
            Some(0),
            "an unknown key after status is ignored"
        );
        assert_eq!(first.importance_name.as_deref(), Some("perceptible"));
        assert_eq!(
            first.description.as_deref(),
            Some("first line of a description that continues on a second line")
        );

        let second = &records[1];
        assert_eq!(second.timestamp.as_deref(), Some("3/20/25, 11:30 AM"));
        assert_eq!(
            second.timestamp_local, None,
            "a locale's short date is ambiguous"
        );
        assert_eq!(second.pid, Some(4400));
        assert_eq!(second.reason, AppExitReason::Unknown);
        assert_eq!(second.reason_code, None);
        assert_eq!(second.process_name, None);
        assert_eq!(second.description, None);

        let third = &records[2];
        assert_eq!(third.timestamp, None);
        assert_eq!(third.process_name.as_deref(), Some("com.example.app:bg"));
        assert_eq!(third.reason_code, None);
        assert_eq!(third.status, Some(-1));
        assert_eq!((third.pss_kb, third.rss_kb), (None, None));
    }

    #[test]
    fn output_without_records_parses_to_nothing() {
        assert!(parse_exit_info(fixture!("no_records.txt")).is_empty());
        assert!(parse_exit_info(fixture!("below_api30.txt")).is_empty());
        assert!(parse_exit_info("").is_empty());
        assert!(reports_unknown_command(fixture!("below_api30.txt")));
        assert!(!reports_unknown_command(fixture!("android11.txt")));
    }

    #[test]
    fn a_long_description_is_capped() {
        let long = "x".repeat(MAX_EXIT_DESCRIPTION_CHARS * 3);
        let dump = format!(
            "ApplicationExitInfo #0:\n  importance=100 pss=0 rss=0 description={long} state=empty\n"
        );

        let records = parse_exit_info(&dump);

        let description = records[0].description.as_deref().unwrap_or_default();
        assert_eq!(description.chars().count(), MAX_EXIT_DESCRIPTION_CHARS + 1);
        assert!(description.ends_with('…'));
    }

    #[test]
    fn sizes_are_read_as_the_device_prints_them() {
        assert_eq!(size_kb("0"), Some(0));
        assert_eq!(size_kb("9216KB"), Some(9216));
        assert_eq!(size_kb("55MB"), Some(55 * 1024));
        assert_eq!(size_kb("3GB"), Some(3 * 1024 * 1024));
        assert_eq!(size_kb("8192"), Some(8), "bytes below 10 KiB");
        assert_eq!(size_kb("lots"), None);
        assert_eq!(size_kb("12XB"), None);
    }

    // ── Reading from a device ────────────────────────────────────────────────

    /// A fake `adb` that records each call's arguments and answers `getprop`
    /// with `sdk`, `pm list packages` with `packages`, and `dumpsys` with the
    /// contents of `dump`.
    struct FakeAdb {
        _dir: tempfile::TempDir,
        adb: PathBuf,
        record: PathBuf,
    }

    impl FakeAdb {
        fn new(sdk: &str, packages: &[&str], dump: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let record = dir.path().join("calls");
            let dump_file = dir.path().join("dump.txt");
            let listing_file = dir.path().join("packages.txt");
            std::fs::write(&dump_file, dump).unwrap();
            let listing: String = packages.iter().map(|p| format!("package:{p}\n")).collect();
            std::fs::write(&listing_file, listing).unwrap();
            let adb = dir.path().join("adb");
            std::fs::write(
                &adb,
                format!(
                    "#!/bin/sh\n[ $# -eq 0 ] && exit 0\necho \"$*\" >> '{record}'\n\
                     case \"$*\" in\n\
                       *getprop*) echo '{sdk}' ;;\n\
                       *'pm list packages'*) cat '{listing}' ;;\n\
                       *exit-info*) cat '{dump}' ;;\n\
                     esac\n",
                    record = record.display(),
                    listing = listing_file.display(),
                    dump = dump_file.display(),
                ),
            )
            .unwrap();
            std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
            crate::utils::process::test_support::run_once(&adb);
            Self {
                _dir: dir,
                adb,
                record,
            }
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(&self.record)
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect()
        }
    }

    /// A Gradle project whose `:app` module applies the application plugin
    /// with `android_block` inside `android { … }`.
    fn project(android_block: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.gradle.kts"),
            "include(\":app\")\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("app")).unwrap();
        std::fs::write(
            dir.path().join("app/build.gradle.kts"),
            format!(
                "plugins {{\n    id(\"com.android.application\")\n}}\n\
                 android {{\n{android_block}\n}}\n"
            ),
        )
        .unwrap();
        dir
    }

    const ONE_APP_WITH_DEBUG_SUFFIX: &str = r#"    defaultConfig {
        applicationId = "com.example.app"
    }
    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
    }"#;

    #[tokio::test]
    async fn reads_newest_first_across_users() {
        let adb = FakeAdb::new("34", &[], fixture!("android14.txt"));

        let result = read_exit_reasons(&adb.adb, "emulator-5554", None, Some("com.example.app"))
            .await
            .unwrap();

        assert!(result.supported);
        assert_eq!(result.api_level, Some(34));
        assert_eq!(result.total_records, 3);
        assert_eq!(result.message, None);
        assert_eq!(
            reasons(&result.records),
            [
                AppExitReason::Freezer,
                AppExitReason::Anr,
                AppExitReason::Crash
            ]
        );
        assert!(
            adb.calls()
                .iter()
                .any(|c| c == "-s emulator-5554 shell dumpsys activity exit-info com.example.app"),
            "{:?}",
            adb.calls()
        );
    }

    #[tokio::test]
    async fn keeps_the_newest_records_up_to_the_cap() {
        let total = MAX_EXIT_RECORDS + 50;
        let mut dump = String::from("  package: com.example.app\n");
        for n in 0..total {
            // Oldest first, so the cap must sort before it truncates.
            dump.push_str(&format!(
                "        ApplicationExitInfo #{n}:\n          \
                 timestamp=2024-01-01 10:{:02}:{:02}.000 pid={n}\n          \
                 process=com.example.app reason=4 (APP CRASH(EXCEPTION))\n",
                n / 60,
                n % 60
            ));
        }
        let adb = FakeAdb::new("34", &[], &dump);

        let result = read_exit_reasons(&adb.adb, "emulator-5554", None, Some("com.example.app"))
            .await
            .unwrap();

        assert_eq!(result.records.len(), MAX_EXIT_RECORDS);
        assert_eq!(result.total_records as usize, total);
        let newest = u32::try_from(total - 1).unwrap();
        let cap = u32::try_from(MAX_EXIT_RECORDS).unwrap();
        assert_eq!(result.records[0].pid, Some(newest));
        assert_eq!(
            result.records[MAX_EXIT_RECORDS - 1].pid,
            Some(newest + 1 - cap)
        );
    }

    #[tokio::test]
    async fn option_shaped_or_shell_packages_are_rejected_before_any_adb_call() {
        let adb = FakeAdb::new("34", &[], fixture!("android14.txt"));

        for package in [
            "-p",
            "--user",
            "com.x;reboot",
            "com.x&&id",
            "com.x$(id)",
            "com.x`id`",
            "com.x app",
            "com.x'",
            "com.x|sh",
        ] {
            let err = read_exit_reasons(&adb.adb, "emulator-5554", None, Some(package))
                .await
                .unwrap_err();
            assert!(
                matches!(err, ExitInfoError::InvalidInput(_)),
                "{package}: {err:?}"
            );
        }
        let err = read_exit_reasons(&adb.adb, "emulator-5554;id", None, Some("com.example.app"))
            .await
            .unwrap_err();
        assert!(matches!(err, ExitInfoError::InvalidInput(_)), "{err:?}");

        assert!(adb.calls().is_empty(), "adb was called: {:?}", adb.calls());
    }

    #[tokio::test]
    async fn a_project_application_id_with_shell_characters_is_rejected_before_any_adb_call() {
        let project = project(
            r#"    defaultConfig {
        applicationId = "com.x;reboot"
    }"#,
        );
        let adb = FakeAdb::new("34", &[], fixture!("android14.txt"));

        let err = read_exit_reasons(&adb.adb, "emulator-5554", Some(project.path()), None)
            .await
            .unwrap_err();

        assert!(matches!(err, ExitInfoError::InvalidInput(_)), "{err:?}");
        assert!(adb.calls().is_empty(), "adb was called: {:?}", adb.calls());
    }

    #[tokio::test]
    async fn below_api_30_says_so_without_running_dumpsys() {
        let adb = FakeAdb::new("29", &[], fixture!("below_api30.txt"));

        let result = read_exit_reasons(&adb.adb, "emulator-5554", None, Some("com.example.app"))
            .await
            .unwrap();

        assert!(!result.supported);
        assert_eq!(result.api_level, Some(29));
        assert!(result.records.is_empty());
        let message = result.message.unwrap_or_default();
        assert!(message.contains("Android 11 (API 30)"), "{message}");
        assert!(message.contains("API 29"), "{message}");
        assert!(
            !adb.calls().iter().any(|c| c.contains("dumpsys")),
            "{:?}",
            adb.calls()
        );
    }

    #[tokio::test]
    async fn a_device_that_does_not_know_the_command_is_unsupported() {
        // The API level could not be read, so dumpsys runs and is refused.
        let adb = FakeAdb::new("", &[], fixture!("below_api30.txt"));

        let result = read_exit_reasons(&adb.adb, "emulator-5554", None, Some("com.example.app"))
            .await
            .unwrap();

        assert!(!result.supported);
        assert_eq!(result.api_level, None);
        assert!(result.records.is_empty());
        assert!(result.message.is_some());
    }

    #[tokio::test]
    async fn no_records_is_an_empty_supported_result() {
        let adb = FakeAdb::new("35", &[], fixture!("no_records.txt"));

        let result = read_exit_reasons(&adb.adb, "emulator-5554", None, Some("com.example.app"))
            .await
            .unwrap();

        assert!(result.supported);
        assert!(result.records.is_empty());
        assert_eq!(result.total_records, 0);
        assert!(result
            .message
            .unwrap_or_default()
            .contains("No process exits are recorded for com.example.app"));
    }

    // ── Default package ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn defaults_to_the_installed_build_of_the_project_app() {
        let project = project(ONE_APP_WITH_DEBUG_SUFFIX);
        let adb = FakeAdb::new(
            "34",
            &["com.example.app.debug", "com.example.apple", "com.other"],
            fixture!("android12.txt"),
        );

        let result = read_exit_reasons(&adb.adb, "emulator-5554", Some(project.path()), None)
            .await
            .unwrap();

        assert_eq!(result.package, "com.example.app.debug");
        assert_eq!(result.records.len(), 3);
        assert!(
            adb.calls()
                .iter()
                .any(|c| c
                    == "-s emulator-5554 shell dumpsys activity exit-info com.example.app.debug"),
            "{:?}",
            adb.calls()
        );
    }

    #[tokio::test]
    async fn defaults_to_the_application_id_when_no_build_is_installed() {
        let project = project(ONE_APP_WITH_DEBUG_SUFFIX);
        let adb = FakeAdb::new("34", &["com.other"], fixture!("no_records.txt"));

        let result = read_exit_reasons(&adb.adb, "emulator-5554", Some(project.path()), None)
            .await
            .unwrap();

        assert_eq!(result.package, "com.example.app");
    }

    #[tokio::test]
    async fn several_installed_builds_need_a_package() {
        let project = project(ONE_APP_WITH_DEBUG_SUFFIX);
        let adb = FakeAdb::new(
            "34",
            &["com.example.app", "com.example.app.debug"],
            fixture!("android12.txt"),
        );

        let err = read_exit_reasons(&adb.adb, "emulator-5554", Some(project.path()), None)
            .await
            .unwrap_err();

        let ExitInfoError::InvalidInput(message) = err else {
            panic!("{err:?}");
        };
        assert!(
            message.contains("com.example.app, com.example.app.debug"),
            "{message}"
        );
        assert!(!adb.calls().iter().any(|c| c.contains("dumpsys")));
    }

    #[tokio::test]
    async fn several_application_ids_need_a_package() {
        let project = project(
            r#"    defaultConfig {
        applicationId = "com.example.app"
    }
    productFlavors {
        free {
            applicationId = "com.example.free"
        }
    }"#,
        );
        let adb = FakeAdb::new("34", &[], fixture!("android12.txt"));

        let err = read_exit_reasons(&adb.adb, "emulator-5554", Some(project.path()), None)
            .await
            .unwrap_err();

        let ExitInfoError::InvalidInput(message) = err else {
            panic!("{err:?}");
        };
        assert!(message.contains("several application ids"), "{message}");
        assert!(
            message.contains("com.example.app, com.example.free"),
            "{message}"
        );
        assert!(adb.calls().is_empty(), "{:?}", adb.calls());
    }

    #[tokio::test]
    async fn no_project_and_no_package_is_invalid_input() {
        let adb = FakeAdb::new("34", &[], fixture!("android12.txt"));

        let err = read_exit_reasons(&adb.adb, "emulator-5554", None, None)
            .await
            .unwrap_err();

        assert!(matches!(err, ExitInfoError::InvalidInput(_)), "{err:?}");
        assert!(adb.calls().is_empty());
    }

    // ── Agent summary ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn agent_summary_lists_the_newest_and_points_to_crash_tools() {
        let adb = FakeAdb::new("33", &[], fixture!("android13.txt"));
        let result = read_exit_reasons(&adb.adb, "emulator-5554", None, Some("com.example.app"))
            .await
            .unwrap();

        let text = agent_summary(&result, 2);

        assert!(
            text.starts_with("com.example.app on emulator-5554 (API 33): 3 process exits"),
            "{text}"
        );
        assert!(text.contains("showing 2"), "{text}");
        assert!(
            text.contains(
                "1. 2023-02-14 15:59:58.021 — crashNative (APP CRASH(NATIVE)) · signal 11 (SIGSEGV)"
            ),
            "{text}"
        );
        assert!(
            text.contains("2. 2023-02-14 14:20:31.448 — dependencyDied"),
            "{text}"
        );
        assert!(!text.contains("3. "), "{text}");
        assert!(text.contains("get_crash_stack_trace"), "{text}");
    }

    #[test]
    fn agent_summary_of_an_empty_result_is_its_message() {
        let result = AppExitReasons {
            serial: "emulator-5554".into(),
            package: "com.example.app".into(),
            api_level: Some(29),
            supported: false,
            message: Some("Process exit reasons need Android 11 (API 30) or later.".into()),
            records: Vec::new(),
            total_records: 0,
        };

        assert_eq!(
            agent_summary(&result, 20),
            "com.example.app on emulator-5554 (API 29): Process exit reasons need Android 11 \
             (API 30) or later."
        );
    }
}
