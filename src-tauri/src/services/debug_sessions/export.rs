//! Exporting a debug session as a zip bundle to share: the manifest, the
//! timeline, and the log lines kept with its crashes, each redacted, plus a
//! report of what redaction replaced. The layout is fixed and allowlisted:
//!
//! - `manifest.json`: bundle and schema versions, Keynobi's version, when it
//!   was exported, the files, and what was left out;
//! - `session.json`: the session's manifest;
//! - `timeline.jsonl`: its events, one per line;
//! - `logs/crash-<seq>.log`: the lines kept with crash or ANR event `seq`;
//! - `attachments/screenshot-<seq>.png`: the screenshot of attachment event
//!   `seq`, when `includeAttachments`; images are not redacted;
//! - `attachments/hierarchy-<seq>.json`: the UI hierarchy of attachment
//!   event `seq`, when `includeAttachments`, redacted like the timeline;
//! - `redaction.json`: the rules, whether each was on, and how many matches
//!   each replaced.
//!
//! Entries are deflated, with a fixed modification time and no symlinks or
//! directories. R8 mappings are never exported: the session names them by
//! SHA-256 and map id. The caps match what a bundle must meet to be read back.

use super::*;
use crate::models::logcat::ProcessedEntry;
use crate::models::redaction::RedactionCount;
use crate::services::redaction::{RedactionContext, Redactor};
use serde_json::{json, Value};
use std::io::Cursor;

/// Version of the bundle layout.
pub const BUNDLE_VERSION: u32 = 1;
/// Most files in a bundle.
pub const MAX_BUNDLE_ENTRIES: usize = 64;
/// Largest file in a bundle, before compression.
pub const MAX_BUNDLE_ENTRY_BYTES: usize = 16 * 1024 * 1024;
/// Largest total of a bundle's files, before compression.
pub const MAX_BUNDLE_UNCOMPRESSED_BYTES: usize = 100 * 1024 * 1024;
/// Largest bundle file.
pub const MAX_BUNDLE_BYTES: usize = 50 * 1024 * 1024;

pub const MANIFEST_ENTRY: &str = "manifest.json";
pub const SESSION_ENTRY: &str = "session.json";
pub const TIMELINE_ENTRY: &str = "timeline.jsonl";
pub const REDACTION_ENTRY: &str = "redaction.json";

fn crash_log_entry(seq: u32) -> String {
    format!("logs/crash-{seq}.log")
}

pub(super) fn attachment_entry(name: &str) -> String {
    format!("{}/{name}", attachments::ATTACHMENTS_DIR)
}

/// A built bundle, not yet saved.
#[derive(Debug)]
pub struct Bundle {
    pub bytes: Vec<u8>,
    pub entries: Vec<String>,
    pub redactions: Vec<RedactionCount>,
    pub omitted: Vec<SessionExportOmission>,
}

/// `keynobi-session-<package>-<opened date>.zip`, for the save dialog.
pub fn export_file_name(id: &str) -> Result<String, AppError> {
    checked_recorded_id(id)?;
    let session = read_session_in(&data_dir(), id, Utc::now())?;
    let date: String = session
        .opened_at
        .chars()
        .take(10)
        .filter(char::is_ascii_digit)
        .collect();
    Ok(format!("keynobi-session-{}-{date}.zip", session.package))
}

/// Build the bundle of session `id` and save it to `path`, which the user
/// chose in the save dialog, through a temporary file and a rename.
pub fn export_session_to(
    id: &str,
    options: &SessionExportOptions,
    path: &Path,
) -> Result<SessionExportResult, AppError> {
    let bundle = build_bundle_in(
        &data_dir(),
        id,
        options,
        dirs::home_dir().map(|h| h.to_string_lossy().into_owned()),
        Utc::now(),
    )?;
    write_new_file(path, &bundle.bytes)?;
    Ok(SessionExportResult {
        path: path.to_string_lossy().into_owned(),
        bytes: bundle.bytes.len() as u64,
        entries: bundle.entries,
        redactions: bundle.redactions,
        omitted: bundle.omitted,
    })
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let tmp = unique_tmp_path(path);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| std::fs::rename(&tmp, path));
    written.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::io(path.display(), e)
    })
}

/// The redaction context of `session`: home, project folder, and serials.
fn context_of(session: &DebugSession, events: &[Value], home: Option<String>) -> RedactionContext {
    let mut project_roots: Vec<String> = session.project_root.iter().cloned().collect();
    if let Some(canonical) = session
        .project_root
        .as_ref()
        .and_then(|root| std::fs::canonicalize(root).ok())
    {
        project_roots.push(canonical.to_string_lossy().into_owned());
    }
    let mut serials = vec![session.device.serial.clone()];
    for event in events {
        collect_serials(event, &mut serials);
    }
    RedactionContext {
        home,
        project_roots,
        serials,
    }
}

/// Every string field named `serial` in `value`.
fn collect_serials(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                match v {
                    Value::String(s) if key == "serial" => out.push(s.clone()),
                    _ => collect_serials(v, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect_serials(v, out)),
        _ => {}
    }
}

/// A kept log line as logcat's `threadtime` format prints it.
pub(super) fn log_line(entry: &ProcessedEntry) -> String {
    format!(
        "{} {:>5} {:>5} {} {}: {}",
        entry.timestamp,
        entry.pid,
        entry.tid,
        agent::level_char(&entry.level),
        entry.tag,
        entry.message
    )
}

fn omission(item: impl Into<String>, reason: impl Into<String>) -> SessionExportOmission {
    SessionExportOmission {
        item: item.into(),
        reason: reason.into(),
    }
}

/// `hierarchy` with every string redacted, fitted again to the caps an
/// import checks, since a placeholder can be longer than what it replaced.
fn redacted_hierarchy(
    redactor: &mut Redactor,
    hierarchy: &DebugSessionHierarchy,
) -> Result<Vec<u8>, AppError> {
    let failed = |e: serde_json::Error| AppError::Other(format!("Cannot redact a hierarchy: {e}"));
    let mut value = serde_json::to_value(hierarchy).map_err(failed)?;
    redactor.redact_json(&mut value);
    let redacted: DebugSessionHierarchy = serde_json::from_value(value).map_err(failed)?;
    attachments::fitted_hierarchy(redacted).map(|(json, _)| json)
}

/// Build the bundle of session `id` in memory.
pub(super) fn build_bundle_in(
    data_dir: &Path,
    id: &str,
    options: &SessionExportOptions,
    home: Option<String>,
    now: DateTime<Utc>,
) -> Result<Bundle, AppError> {
    checked_recorded_id(id)?;
    let session = read_session_in(data_dir, id, now)?;
    let events = read_events(data_dir, id)?;
    let event_values: Vec<Value> = events
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    let mut redactor = Redactor::new(options.redaction, context_of(&session, &event_values, home));
    let mut omitted = vec![omission(
        "R8 mappings",
        "never exported; the session names each by SHA-256 and map id",
    )];
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    let mut manifest_value = serde_json::to_value(&session)
        .map_err(|e| AppError::Other(format!("Cannot serialize the session: {e}")))?;
    redactor.redact_json(&mut manifest_value);
    let session_json = serde_json::to_vec_pretty(&manifest_value)
        .map_err(|e| AppError::Other(format!("Cannot serialize the session: {e}")))?;
    files.push((SESSION_ENTRY.into(), session_json));

    let mut timeline = Vec::new();
    for (event, mut value) in events.iter().zip(event_values) {
        redactor.redact_json(&mut value);
        let mut line = value.to_string().into_bytes();
        line.push(b'\n');
        if timeline.len() + line.len() > MAX_BUNDLE_ENTRY_BYTES {
            omitted.push(omission(
                format!("timeline events from #{}", event.seq),
                format!("{TIMELINE_ENTRY} reached {MAX_BUNDLE_ENTRY_BYTES} bytes"),
            ));
            break;
        }
        timeline.extend_from_slice(&line);
    }
    files.push((TIMELINE_ENTRY.into(), timeline));

    let captured: Vec<u32> = events
        .iter()
        .filter_map(|e| match &e.event {
            DebugSessionEventData::Crash(c) | DebugSessionEventData::Anr(c) => {
                c.capture.as_ref().map(|_| e.seq)
            }
            _ => None,
        })
        .collect();
    if !options.include_crash_logs && !captured.is_empty() {
        omitted.push(omission("crash log lines", "not selected"));
    }
    let mut total: usize = files.iter().map(|(_, b)| b.len()).sum();
    for seq in captured.iter().filter(|_| options.include_crash_logs) {
        let name = crash_log_entry(*seq);
        let capture = match crashes::get_capture_in(data_dir, id, *seq, None) {
            Ok(capture) => capture,
            Err(e) => {
                omitted.push(omission(name, e.to_string()));
                continue;
            }
        };
        let mut text = String::new();
        for entry in &capture.entries {
            text.push_str(&redactor.redact(&log_line(entry)));
            text.push('\n');
        }
        let fits = text.len() <= MAX_BUNDLE_ENTRY_BYTES
            && total + text.len() <= MAX_BUNDLE_UNCOMPRESSED_BYTES
            // Room for the manifest and the redaction report.
            && files.len() + 3 <= MAX_BUNDLE_ENTRIES;
        if !fits {
            omitted.push(omission(name, "the bundle reached its size or file limit"));
            continue;
        }
        total += text.len();
        files.push((name, text.into_bytes()));
    }

    let attached: Vec<(u32, &DebugSessionAttachment)> = events
        .iter()
        .filter_map(|e| match &e.event {
            DebugSessionEventData::Attachment(a) => Some((e.seq, a)),
            _ => None,
        })
        .collect();
    if !options.include_attachments && !attached.is_empty() {
        omitted.push(omission("attachments", "not selected"));
    }
    let mut images = false;
    for (seq, a) in attached.iter().filter(|_| options.include_attachments) {
        let entry = attachment_entry(&a.name);
        let data = match a.kind {
            DebugSessionAttachmentKind::Screenshot => {
                attachments::read_attachment_in(data_dir, id, *seq)
            }
            DebugSessionAttachmentKind::Hierarchy => {
                attachments::read_hierarchy_in(data_dir, id, *seq)
                    .and_then(|h| redacted_hierarchy(&mut redactor, &h))
            }
        };
        let data = match data {
            Ok(data) => data,
            Err(e) => {
                omitted.push(omission(entry, e.to_string()));
                continue;
            }
        };
        let fits = data.len() <= MAX_BUNDLE_ENTRY_BYTES
            && total + data.len() <= MAX_BUNDLE_UNCOMPRESSED_BYTES
            && files.len() + 3 <= MAX_BUNDLE_ENTRIES;
        if !fits {
            omitted.push(omission(entry, "the bundle reached its size or file limit"));
            continue;
        }
        total += data.len();
        images |= a.kind == DebugSessionAttachmentKind::Screenshot;
        files.push((entry, data));
    }

    let redactions = redactor.counts();
    let note = if images {
        "Redaction replaces what its rules recognise and nothing else; attached screenshots \
         are not redacted. Check the files before sharing them."
    } else {
        "Redaction replaces what its rules recognise and nothing else. Check the files before sharing them."
    };
    let report = json!({
        "bestEffort": true,
        "note": note,
        "rules": redactions,
    });
    files.push((
        REDACTION_ENTRY.into(),
        serde_json::to_vec_pretty(&report)
            .map_err(|e| AppError::Other(format!("Cannot serialize the report: {e}")))?,
    ));

    let mut entries: Vec<String> = vec![MANIFEST_ENTRY.into()];
    entries.extend(files.iter().map(|(name, _)| name.clone()));
    let mut manifest = json!({
        "bundleVersion": BUNDLE_VERSION,
        "schemaVersion": DEBUG_SESSION_SCHEMA_VERSION,
        "keynobiVersion": env!("CARGO_PKG_VERSION"),
        "exportedAt": stamp(now),
        "sessionId": session.id,
        "package": session.package,
        "entries": entries,
        "redaction": REDACTION_ENTRY,
        "omitted": omitted,
    });
    redactor.redact_json(&mut manifest);
    let manifest = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| AppError::Other(format!("Cannot serialize the manifest: {e}")))?;
    files.insert(0, (MANIFEST_ENTRY.into(), manifest));

    let bytes = write_zip(&files)?;
    Ok(Bundle {
        bytes,
        entries,
        redactions,
        omitted,
    })
}

/// Deflate `files` into a zip, in order, with a fixed modification time.
fn write_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, AppError> {
    let total: usize = files.iter().map(|(_, b)| b.len()).sum();
    if files.len() > MAX_BUNDLE_ENTRIES
        || total > MAX_BUNDLE_UNCOMPRESSED_BYTES
        || files.iter().any(|(_, b)| b.len() > MAX_BUNDLE_ENTRY_BYTES)
    {
        return Err(AppError::Other(
            "The bundle is over its file or size limit".into(),
        ));
    }
    let failed =
        |e: &dyn std::fmt::Display| AppError::Other(format!("Cannot write the bundle: {e}"));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(name.as_str(), options)
            .map_err(|e| failed(&e))?;
        zip.write_all(bytes).map_err(|e| failed(&e))?;
    }
    let bytes = zip.finish().map_err(|e| failed(&e))?.into_inner();
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(AppError::Other(format!(
            "The bundle would be larger than {} MiB",
            MAX_BUNDLE_BYTES / (1024 * 1024)
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel};
    use crate::models::redaction::{RedactionRule, RedactionRules};
    use std::collections::BTreeMap;
    use std::io::Read;
    use tempfile::TempDir;

    const RETAIN: Retention = Retention {
        days: 14,
        max_folder_mb: 200,
    };

    fn phone() -> InstallTarget {
        InstallTarget {
            serial: "R5CT1234ABC".into(),
            avd_name: None,
            model: Some("Pixel 8".into()),
        }
    }

    fn open(dir: &Path) -> DebugSession {
        let target = phone();
        let entry = InstalledBuild {
            serial: target.serial.clone(),
            avd_name: None,
            model: target.model.clone(),
            package: "com.example".into(),
            apk_sha256: "a".repeat(64),
            build_id: None,
            version_code: Some(7),
            mappings: vec![],
            installed_at: "2026-09-25T10:32:00+00:00".into(),
        };
        let mut session =
            open_in(dir, &target, &entry, BuildActor::App, RETAIN, Utc::now()).expect("opened");
        session.project_root = Some("/Users/jane/work/MyApp".into());
        save_manifest(dir, &session).expect("saved");
        session
    }

    fn entry(message: &str) -> ProcessedEntry {
        ProcessedEntry {
            id: 1,
            timestamp: "09-25 10:32:05.123".into(),
            pid: 4242,
            tid: 4242,
            level: LogcatLevel::Error,
            tag: "AndroidRuntime".into(),
            message: message.into(),
            package: Some("com.example".into()),
            kind: LogcatKind::Normal,
            is_crash: true,
            flags: 0,
            category: EntryCategory::General,
            crash_group_id: Some(1),
            json_body: None,
        }
    }

    /// Add a crash event whose kept lines are `lines`.
    fn crash_with_capture(dir: &Path, session: &DebugSession, lines: &[&str]) -> u32 {
        let mut s = read_manifest(dir, &session.id).unwrap();
        let seq = s.event_count + 1;
        let capture_dir = session_dir(dir, &session.id).join("captures");
        std::fs::create_dir_all(&capture_dir).unwrap();
        let text: String = lines
            .iter()
            .map(|l| format!("{}\n", serde_json::to_string(&entry(l)).unwrap()))
            .collect();
        std::fs::write(capture_dir.join(format!("crash-{seq}.jsonl")), text).unwrap();
        append_locked(
            dir,
            &mut s,
            None,
            DebugSessionEventData::Crash(DebugSessionCrash {
                serial: "R5CT1234ABC".into(),
                pid: Some(4242),
                summary: "java.lang.IllegalStateException: jane@example.com".into(),
                signature: "00000000deadbeef".into(),
                received_at: stamp(Utc::now()),
                device_time: "09-25 10:32:05.123".into(),
                attribution: DebugSessionAttribution {
                    method: DebugSessionAttributionMethod::InstallRecord,
                    verified: true,
                    reason: None,
                },
                capture: Some(DebugSessionCaptureRef {
                    entries: lines.len() as u32,
                    bytes: 1,
                    truncated: false,
                }),
                dropped_lines: 0,
            }),
            Utc::now(),
        )
        .unwrap();
        save_manifest(dir, &s).unwrap();
        seq
    }

    /// Every file of `bytes`, by name, after checking each is deflated.
    fn unzip(bytes: &[u8]) -> Vec<(String, String)> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip");
        let mut files = Vec::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            assert_eq!(file.compression(), zip::CompressionMethod::Deflated);
            assert!(file.is_file(), "{} is not a file", file.name());
            assert_eq!(file.last_modified(), Some(zip::DateTime::default()));
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            files.push((file.name().to_string(), text));
        }
        files
    }

    fn file<'a>(files: &'a [(String, String)], name: &str) -> &'a str {
        &files
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .1
    }

    fn build(dir: &Path, id: &str, options: &SessionExportOptions) -> Bundle {
        build_bundle_in(dir, id, options, Some("/Users/jane".into()), Utc::now()).unwrap()
    }

    #[test]
    fn a_bundle_has_the_allowlisted_layout_and_reads_back() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let seq = crash_with_capture(
            dir.path(),
            &session,
            &["FATAL EXCEPTION: main", "at com.example.Main(Main.kt:12)"],
        );
        add_bookmark_in(
            dir.path(),
            Some(&session.id),
            None,
            "saw it at /Users/jane/work/MyApp/app",
            None,
            Utc::now(),
        )
        .unwrap();

        let bundle = build(dir.path(), &session.id, &SessionExportOptions::default());
        let files = unzip(&bundle.bytes);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        let log = format!("logs/crash-{seq}.log");
        assert_eq!(
            names,
            [
                MANIFEST_ENTRY,
                SESSION_ENTRY,
                TIMELINE_ENTRY,
                log.as_str(),
                REDACTION_ENTRY
            ]
        );
        assert_eq!(bundle.entries, names);

        let manifest: Value = serde_json::from_str(file(&files, MANIFEST_ENTRY)).unwrap();
        assert_eq!(manifest["bundleVersion"], BUNDLE_VERSION);
        assert_eq!(manifest["schemaVersion"], DEBUG_SESSION_SCHEMA_VERSION);
        assert_eq!(manifest["sessionId"], session.id.as_str());
        assert_eq!(manifest["entries"].as_array().unwrap().len(), names.len());
        assert_eq!(manifest["omitted"][0]["item"], "R8 mappings");

        let restored: DebugSession = serde_json::from_str(file(&files, SESSION_ENTRY)).unwrap();
        assert_eq!(restored.id, session.id);
        assert_eq!(restored.project_root.as_deref(), Some("<project>"));
        assert_eq!(restored.device.serial, "<device-1>");

        let timeline: Vec<DebugSessionEvent> = file(&files, TIMELINE_ENTRY)
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(timeline.len(), 3);
        let text = file(&files, TIMELINE_ENTRY);
        assert!(text.contains("<project>/app"), "{text}");
        assert!(text.contains("<email-1>"), "{text}");
        assert!(!text.contains("jane@example.com"), "{text}");
        assert!(!text.contains("R5CT1234ABC"), "{text}");

        assert_eq!(
            file(&files, &log),
            "09-25 10:32:05.123  4242  4242 E AndroidRuntime: FATAL EXCEPTION: main\n\
             09-25 10:32:05.123  4242  4242 E AndroidRuntime: at com.example.Main(Main.kt:12)\n"
        );

        let report: Value = serde_json::from_str(file(&files, REDACTION_ENTRY)).unwrap();
        assert_eq!(report["bestEffort"], true);
        let counts: BTreeMap<String, u64> = report["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["rule"].as_str().unwrap().to_string(),
                    r["count"].as_u64().unwrap(),
                )
            })
            .collect();
        assert_eq!(counts.len(), RedactionRule::ALL.len());
        assert!(counts["emails"] >= 1, "{counts:?}");
        assert!(counts["paths"] >= 2, "{counts:?}");
        assert!(counts["deviceSerials"] >= 2, "{counts:?}");
    }

    #[test]
    fn every_redaction_rule_reaches_the_crash_log_lines() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let seq = crash_with_capture(
            dir.path(),
            &session,
            &[
                "POST https://api.example.com Authorization: Bearer abc123def456",
                "user jane@example.com from 192.168.0.12 on R5CT1234ABC",
                "cache /Users/jane/Library/x, host 10.0.2.2, loopback 127.0.0.1",
            ],
        );
        let bundle = build(dir.path(), &session.id, &SessionExportOptions::default());
        let files = unzip(&bundle.bytes);
        let log = file(&files, &format!("logs/crash-{seq}.log"));
        assert!(log.contains("Authorization: Bearer <secret-1>"), "{log}");
        assert!(
            log.contains("user <email-1> from <ip-1> on <device-1>"),
            "{log}"
        );
        assert!(
            log.contains("cache ~/Library/x, host 10.0.2.2, loopback 127.0.0.1"),
            "{log}"
        );
        for secret in [
            "abc123def456",
            "jane@example.com",
            "192.168.0.12",
            "R5CT1234ABC",
            "/Users/jane",
        ] {
            let all: String = files.iter().map(|(_, t)| t.as_str()).collect();
            assert!(!all.contains(secret), "{secret} left in the bundle");
        }
    }

    #[test]
    fn redaction_turned_off_keeps_the_text_and_says_so() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        crash_with_capture(dir.path(), &session, &["user jane@example.com"]);
        let options = SessionExportOptions {
            redaction: RedactionRules {
                emails: false,
                ..RedactionRules::default()
            },
            include_crash_logs: true,
            include_attachments: true,
        };
        let bundle = build(dir.path(), &session.id, &options);
        let files = unzip(&bundle.bytes);
        let all: String = files.iter().map(|(_, t)| t.as_str()).collect();
        assert!(all.contains("jane@example.com"));
        let emails = bundle
            .redactions
            .iter()
            .find(|c| c.rule == RedactionRule::Emails)
            .unwrap();
        assert_eq!((emails.enabled, emails.count), (false, 0));
    }

    #[test]
    fn crash_logs_can_be_left_out_and_mappings_never_go_in() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        crash_with_capture(dir.path(), &session, &["boom"]);
        let options = SessionExportOptions {
            include_crash_logs: false,
            ..SessionExportOptions::default()
        };
        let bundle = build(dir.path(), &session.id, &options);
        let files = unzip(&bundle.bytes);
        assert!(files.iter().all(|(n, _)| !n.starts_with("logs/")));
        assert!(files.iter().all(|(n, _)| !n.contains("mapping")));
        let items: Vec<&str> = bundle.omitted.iter().map(|o| o.item.as_str()).collect();
        assert_eq!(items, ["R8 mappings", "crash log lines"]);
    }

    #[test]
    fn a_capture_that_is_gone_is_listed_as_omitted() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let seq = crash_with_capture(dir.path(), &session, &["boom"]);
        std::fs::remove_file(
            session_dir(dir.path(), &session.id)
                .join("captures")
                .join(format!("crash-{seq}.jsonl")),
        )
        .unwrap();
        let bundle = build(dir.path(), &session.id, &SessionExportOptions::default());
        assert!(bundle
            .omitted
            .iter()
            .any(|o| o.item == format!("logs/crash-{seq}.log")));
        assert!(!bundle.entries.iter().any(|e| e.starts_with("logs/")));
    }

    #[test]
    fn caps_refuse_an_oversized_bundle() {
        let too_many: Vec<(String, Vec<u8>)> = (0..=MAX_BUNDLE_ENTRIES)
            .map(|i| (format!("logs/crash-{i}.log"), vec![b'x']))
            .collect();
        assert!(write_zip(&too_many).is_err());
        let too_big = vec![(
            "timeline.jsonl".to_string(),
            vec![b'x'; MAX_BUNDLE_ENTRY_BYTES + 1],
        )];
        assert!(write_zip(&too_big).is_err());
        let fine = vec![("timeline.jsonl".to_string(), vec![b'x'; 1024])];
        assert!(write_zip(&fine).is_ok());
    }

    #[test]
    fn the_same_session_exports_the_same_files() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        crash_with_capture(dir.path(), &session, &["boom"]);
        let now = Utc::now();
        let options = SessionExportOptions::default();
        let a = build_bundle_in(dir.path(), &session.id, &options, None, now).unwrap();
        let b = build_bundle_in(dir.path(), &session.id, &options, None, now).unwrap();
        assert_eq!(a.bytes, b.bytes);
    }

    #[test]
    fn a_bad_or_unknown_id_is_refused() {
        let dir = TempDir::new().unwrap();
        let options = SessionExportOptions::default();
        let err = build_bundle_in(dir.path(), "../x", &options, None, Utc::now()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err}");
        let err = build_bundle_in(
            dir.path(),
            "s-20260925T103200Z-000000000001",
            &options,
            None,
            Utc::now(),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
    }

    #[test]
    fn the_bundle_is_written_through_a_new_temporary_file() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("out.zip");
        std::fs::write(&target, b"old").unwrap();
        write_new_file(&target, b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
