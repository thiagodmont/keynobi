//! Importing a debug session someone exported (see [`export`](super::export)).
//!
//! The bundle is untrusted. It is read whole into memory (at most
//! [`MAX_BUNDLE_BYTES`]) and refused unless:
//!
//! - its central directory, read here as well as by the zip reader (which
//!   keeps only the last of two entries with one name), lists at most
//!   [`MAX_BUNDLE_ENTRIES`] distinct names, and no two entries share data;
//! - every name is one the export writes, exactly: `manifest.json`,
//!   `session.json`, `timeline.jsonl`, `redaction.json`, and
//!   `logs/crash-<seq>.log`, so no absolute path, `..`, backslash, or folder
//!   gets through;
//! - every entry is a regular file, stored or deflated, not encrypted, and its
//!   bytes, counted as they are inflated, stay within
//!   [`MAX_BUNDLE_ENTRY_BYTES`], [`MAX_BUNDLE_UNCOMPRESSED_BYTES`] in total,
//!   and [`MAX_BUNDLE_COMPRESSION_RATIO`];
//! - `bundleVersion` and `schemaVersion` are ones this version writes, and
//!   every JSON file parses with no field the schema does not know.
//!
//! What is accepted is rewritten, not copied: at most the caps of a recorded
//! session ([`MAX_EVENTS_PER_SESSION`], [`MAX_SESSION_BYTES`],
//! [`MAX_CAPTURES_PER_SESSION`], [`MAX_CAPTURE_ENTRIES`], [`MAX_CAPTURE_BYTES`]),
//! into `<data dir>/imports/<new id>/` with files created new and private.
//! Imported sessions live apart from recorded ones: the index, attribution,
//! mapping pins, and retention never see them. They are read-only and kept
//! until the user deletes them, at most [`MAX_IMPORTED_SESSIONS`] and
//! [`MAX_IMPORTS_BYTES`].

use super::export::{MANIFEST_ENTRY, REDACTION_ENTRY, SESSION_ENTRY, TIMELINE_ENTRY};
use super::*;
use crate::models::logcat::{EntryCategory, LogcatKind, LogcatLevel, ProcessedEntry};
use crate::models::redaction::RedactionCount;
use crate::services::log_pipeline::parse_logcat_line;
use crate::utils::path::validate_within_root;
use crate::utils::validation::validate_package_name;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::io::Cursor;

/// Most imported sessions kept; another is refused until one is deleted.
pub const MAX_IMPORTED_SESSIONS: usize = 20;
/// Largest total size of the imported sessions' files.
pub const MAX_IMPORTS_BYTES: u64 = 200 * 1024 * 1024;
/// Most bytes a bundle entry may inflate to per compressed byte, past its
/// first [`RATIO_FREE_BYTES`].
pub const MAX_BUNDLE_COMPRESSION_RATIO: u64 = 200;
/// Bytes of an entry the compression ratio is not checked on.
const RATIO_FREE_BYTES: u64 = 1024 * 1024;
/// Most `omitted` items kept from a bundle's manifest.
const MAX_IMPORT_OMISSIONS: usize = 100;
/// Longest bundle file name or Keynobi version kept.
const MAX_IMPORT_NAME_CHARS: usize = 255;
const IMPORTS_DIR: &str = "imports";
const READ_CHUNK: usize = 64 * 1024;

pub fn imports_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(IMPORTS_DIR)
}

/// Whether `id` (validated) names an imported session.
pub fn is_imported_id(id: &str) -> bool {
    id.starts_with("i-")
}

fn refused(reason: impl std::fmt::Display) -> AppError {
    AppError::InvalidInput(format!("This bundle cannot be imported: {reason}"))
}

// ── Reading the zip ───────────────────────────────────────────────────────────

/// A file a bundle may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Entry {
    Manifest,
    Session,
    Timeline,
    Redaction,
    CrashLog(u32),
}

/// The entry `name` is, when it is exactly a name the export writes.
fn allowlisted(name: &[u8]) -> Option<Entry> {
    let name = std::str::from_utf8(name).ok()?;
    match name {
        MANIFEST_ENTRY => Some(Entry::Manifest),
        SESSION_ENTRY => Some(Entry::Session),
        TIMELINE_ENTRY => Some(Entry::Timeline),
        REDACTION_ENTRY => Some(Entry::Redaction),
        _ => {
            let seq = name.strip_prefix("logs/crash-")?.strip_suffix(".log")?;
            let canonical = (1..=10).contains(&seq.len())
                && seq.bytes().all(|b| b.is_ascii_digit())
                && !seq.starts_with('0');
            canonical
                .then(|| seq.parse().ok())
                .flatten()
                .map(Entry::CrashLog)
        }
    }
}

/// `name` as an error message may show it.
fn shown(name: &[u8]) -> String {
    let text: String = String::from_utf8_lossy(name).escape_debug().collect();
    truncate_chars(&text, 100)
}

fn le16(bytes: &[u8], at: usize) -> Option<usize> {
    let b = bytes.get(at..at.checked_add(2)?)?;
    Some(usize::from(u16::from_le_bytes([b[0], b[1]])))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The names the central directory lists, in order. The export never writes
/// ZIP64, so a bundle that needs it is refused.
fn central_directory_names(bytes: &[u8]) -> Result<Vec<Vec<u8>>, AppError> {
    const END_SIGNATURE: u32 = 0x0605_4b50;
    const ENTRY_SIGNATURE: u32 = 0x0201_4b50;
    const END_LEN: usize = 22;
    const ENTRY_LEN: usize = 46;
    let damaged = || refused("it is not a zip file, or it is damaged");
    let last = bytes.len().checked_sub(END_LEN).ok_or_else(damaged)?;
    let end = (last.saturating_sub(usize::from(u16::MAX))..=last)
        .rev()
        .find(|&at| le32(bytes, at) == Some(END_SIGNATURE))
        .ok_or_else(damaged)?;
    let count = le16(bytes, end + 10).ok_or_else(damaged)?;
    let offset = le32(bytes, end + 16).ok_or_else(damaged)?;
    if count == usize::from(u16::MAX) || offset == u32::MAX {
        return Err(refused("it uses ZIP64, which a bundle never needs"));
    }
    if count > MAX_BUNDLE_ENTRIES {
        return Err(refused(format!(
            "it holds {count} files; a bundle holds at most {MAX_BUNDLE_ENTRIES}"
        )));
    }
    let mut at = offset as usize;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        if le32(bytes, at) != Some(ENTRY_SIGNATURE) {
            return Err(damaged());
        }
        let name_len = le16(bytes, at + 28).ok_or_else(damaged)?;
        let extra_len = le16(bytes, at + 30).ok_or_else(damaged)?;
        let comment_len = le16(bytes, at + 32).ok_or_else(damaged)?;
        let name = bytes
            .get(at + ENTRY_LEN..at + ENTRY_LEN + name_len)
            .ok_or_else(damaged)?;
        names.push(name.to_vec());
        at += ENTRY_LEN + name_len + extra_len + comment_len;
    }
    Ok(names)
}

/// Read `reader` to its end, counting what it inflates to against the caps.
fn read_capped(
    reader: &mut impl Read,
    name: &str,
    compressed: u64,
    total: &mut usize,
) -> Result<Vec<u8>, AppError> {
    let mut data = Vec::new();
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        let n = reader
            .read(&mut chunk)
            .map_err(|e| refused(format!("{name} is damaged ({e})")))?;
        if n == 0 {
            return Ok(data);
        }
        data.extend_from_slice(&chunk[..n]);
        *total += n;
        if data.len() > MAX_BUNDLE_ENTRY_BYTES {
            return Err(refused(format!(
                "{name} is larger than {} MiB",
                MAX_BUNDLE_ENTRY_BYTES / (1024 * 1024)
            )));
        }
        if *total > MAX_BUNDLE_UNCOMPRESSED_BYTES {
            return Err(refused(format!(
                "its files are larger than {} MiB in total",
                MAX_BUNDLE_UNCOMPRESSED_BYTES / (1024 * 1024)
            )));
        }
        let inflated = data.len() as u64;
        if inflated > RATIO_FREE_BYTES
            && inflated
                > compressed
                    .max(1)
                    .saturating_mul(MAX_BUNDLE_COMPRESSION_RATIO)
        {
            return Err(refused(format!(
                "{name} inflates more than {MAX_BUNDLE_COMPRESSION_RATIO} times"
            )));
        }
    }
}

/// The bundle's files, checked against the layout and the caps, in the
/// order they are stored.
fn read_entries(bytes: &[u8]) -> Result<Vec<(Entry, Vec<u8>)>, AppError> {
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(refused(format!(
            "it is larger than {} MiB",
            MAX_BUNDLE_BYTES / (1024 * 1024)
        )));
    }
    let names = central_directory_names(bytes)?;
    let mut seen = HashSet::new();
    for name in &names {
        if !seen.insert(name.as_slice()) {
            return Err(refused(format!("it holds {} twice", shown(name))));
        }
        if allowlisted(name).is_none() {
            return Err(refused(format!(
                "{} is not a file a debug session bundle holds",
                shown(name)
            )));
        }
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| refused(format!("it is not a zip file, or it is damaged ({e})")))?;
    if archive.len() != names.len() {
        return Err(refused("its file list is inconsistent"));
    }
    if archive.has_overlapping_files().unwrap_or(true) {
        return Err(refused("its files overlap"));
    }
    let mut total = 0usize;
    let mut files = Vec::with_capacity(names.len());
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| refused(format!("file {} cannot be read ({e})", i + 1)))?;
        let name = shown(file.name_raw());
        let entry = allowlisted(file.name_raw())
            .ok_or_else(|| refused(format!("{name} is not a file a debug session bundle holds")))?;
        if file.encrypted() {
            return Err(refused(format!("{name} is encrypted")));
        }
        if !matches!(
            file.compression(),
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            return Err(refused(format!(
                "{name} is compressed with {}; a bundle is deflated",
                file.compression()
            )));
        }
        let file_type = file.unix_mode().map_or(0, |mode| mode & 0o170_000);
        if file.is_dir() || file.is_symlink() || !matches!(file_type, 0 | 0o100_000) {
            return Err(refused(format!("{name} is not a regular file")));
        }
        if file.size() > MAX_BUNDLE_ENTRY_BYTES as u64 {
            return Err(refused(format!(
                "{name} is larger than {} MiB",
                MAX_BUNDLE_ENTRY_BYTES / (1024 * 1024)
            )));
        }
        let compressed = file.compressed_size();
        let data = read_capped(&mut file, &name, compressed, &mut total)?;
        files.push((entry, data));
    }
    Ok(files)
}

// ── Parsing the files ─────────────────────────────────────────────────────────

/// The path of a field of `raw` that `known` (the same value parsed and
/// serialized again) does not have.
fn unknown_field(raw: &Value, known: &Value, at: &str) -> Option<String> {
    match (raw, known) {
        (Value::Object(raw), Value::Object(known)) => raw.iter().find_map(|(key, value)| {
            let path = format!("{at}.{key}");
            match known.get(key) {
                Some(k) => unknown_field(value, k, &path),
                // An empty optional value is left out when serialized.
                None if value.is_null() => None,
                None => Some(path),
            }
        }),
        (Value::Array(raw), Value::Array(known)) => raw
            .iter()
            .zip(known)
            .enumerate()
            .find_map(|(i, (r, k))| unknown_field(r, k, &format!("{at}[{i}]"))),
        _ => None,
    }
}

/// Parse `text` as `T`, refusing any field `T` does not know. The session
/// schema flattens its events, which `deny_unknown_fields` does not support,
/// so the parsed value is serialized again and compared with the input.
fn parse_strict<T: DeserializeOwned + Serialize>(text: &[u8], what: &str) -> Result<T, AppError> {
    let raw: Value = serde_json::from_slice(text)
        .map_err(|e| refused(format!("{what} is not valid JSON ({e})")))?;
    let parsed: T = serde_json::from_value(raw.clone())
        .map_err(|e| refused(format!("{what} does not match its schema ({e})")))?;
    let known = serde_json::to_value(&parsed)
        .map_err(|e| AppError::Other(format!("Cannot serialize {what}: {e}")))?;
    if let Some(field) = unknown_field(&raw, &known, "") {
        return Err(refused(format!(
            "{what} has a field this version of Keynobi does not know ({})",
            field.trim_start_matches('.')
        )));
    }
    Ok(parsed)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BundleManifest {
    bundle_version: u32,
    schema_version: u32,
    keynobi_version: String,
    exported_at: String,
    session_id: String,
    package: String,
    entries: Vec<String>,
    redaction: String,
    omitted: Vec<SessionExportOmission>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RedactionReport {
    best_effort: bool,
    note: String,
    rules: Vec<RedactionCount>,
}

/// Refuse a bundle of another layout version before its fields are checked,
/// so the message names the version.
fn check_bundle_version(manifest: &[u8]) -> Result<(), AppError> {
    let raw: Value = serde_json::from_slice(manifest)
        .map_err(|e| refused(format!("{MANIFEST_ENTRY} is not valid JSON ({e})")))?;
    match raw.get("bundleVersion").and_then(Value::as_u64) {
        Some(v) if v == u64::from(BUNDLE_VERSION) => Ok(()),
        Some(v) if v > u64::from(BUNDLE_VERSION) => Err(refused(format!(
            "it is bundle version {v}, and this version of Keynobi reads version \
             {BUNDLE_VERSION}. Update Keynobi to import it."
        ))),
        Some(v) => Err(refused(format!("bundle version {v} is unknown"))),
        None => Err(refused(format!("{MANIFEST_ENTRY} has no bundleVersion"))),
    }
}

/// A capture's `threadtime` lines as entries, the newest that fit the
/// capture caps, and whether older ones were left out.
fn capture_entries(text: &str) -> (Vec<ProcessedEntry>, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(MAX_CAPTURE_ENTRIES);
    let mut entries: Vec<ProcessedEntry> = lines[skip..]
        .iter()
        .zip(1u64..)
        .map(|(line, id)| {
            let raw = parse_logcat_line(line);
            ProcessedEntry {
                id,
                timestamp: raw
                    .as_ref()
                    .map(|r| r.timestamp.clone())
                    .unwrap_or_default(),
                pid: raw.as_ref().map_or(0, |r| r.pid),
                tid: raw.as_ref().map_or(0, |r| r.tid),
                level: raw
                    .as_ref()
                    .map_or(LogcatLevel::Unknown, |r| r.level.clone()),
                tag: raw.as_ref().map(|r| r.tag.clone()).unwrap_or_default(),
                message: raw.map_or_else(|| (*line).to_string(), |r| r.message),
                package: None,
                kind: LogcatKind::Normal,
                is_crash: false,
                flags: 0,
                category: EntryCategory::General,
                crash_group_id: None,
                json_body: None,
            }
        })
        .collect();
    let mut truncated = skip > 0;
    let mut bytes: u64 = entries.iter().map(entry_bytes).sum();
    while bytes > MAX_CAPTURE_BYTES && !entries.is_empty() {
        bytes -= entry_bytes(&entries.remove(0));
        truncated = true;
    }
    (entries, truncated)
}

fn entry_bytes(entry: &ProcessedEntry) -> u64 {
    serde_json::to_string(entry).map_or(0, |s| s.len() as u64 + 1)
}

fn short(text: &str) -> String {
    truncate_chars(text, MAX_EVENT_TEXT_CHARS)
}

/// A bundle checked and rewritten, ready to store.
#[derive(Debug)]
pub(super) struct PreparedImport {
    /// Everything but the id, which is given when it is stored.
    pub session: DebugSession,
    /// `events.jsonl`.
    pub events: Vec<u8>,
    /// `captures/crash-<seq>.jsonl`, by seq.
    pub captures: BTreeMap<u32, Vec<u8>>,
}

impl PreparedImport {
    fn bytes(&self) -> u64 {
        let session = serde_json::to_vec_pretty(&self.session).map_or(0, |s| s.len());
        (session + self.events.len() + self.captures.values().map(Vec::len).sum::<usize>()) as u64
    }
}

/// Check the bundle `bytes`, named `file_name`, and rewrite what it holds as
/// an imported session.
pub(super) fn prepare_import(
    bytes: &[u8],
    file_name: &str,
    now: DateTime<Utc>,
) -> Result<PreparedImport, AppError> {
    let mut files: BTreeMap<Entry, Vec<u8>> = read_entries(bytes)?.into_iter().collect();
    let mut take = |entry: Entry, name: &str| {
        files
            .remove(&entry)
            .ok_or_else(|| refused(format!("it has no {name}")))
    };
    let manifest_bytes = take(Entry::Manifest, MANIFEST_ENTRY)?;
    check_bundle_version(&manifest_bytes)?;
    let manifest: BundleManifest = parse_strict(&manifest_bytes, MANIFEST_ENTRY)?;
    let session: DebugSession = parse_strict(&take(Entry::Session, SESSION_ENTRY)?, SESSION_ENTRY)?;
    let timeline = take(Entry::Timeline, TIMELINE_ENTRY)?;
    let report: RedactionReport =
        parse_strict(&take(Entry::Redaction, REDACTION_ENTRY)?, REDACTION_ENTRY)?;
    let logs: BTreeMap<u32, Vec<u8>> = files
        .into_iter()
        .filter_map(|(entry, data)| match entry {
            Entry::CrashLog(seq) => Some((seq, data)),
            _ => None,
        })
        .collect();

    if manifest.schema_version != DEBUG_SESSION_SCHEMA_VERSION
        || session.schema_version != DEBUG_SESSION_SCHEMA_VERSION
    {
        return Err(refused(format!(
            "its session is schema version {}, and this version of Keynobi reads version \
             {DEBUG_SESSION_SCHEMA_VERSION}",
            session.schema_version.max(manifest.schema_version)
        )));
    }
    if DateTime::parse_from_rfc3339(&manifest.exported_at).is_err() {
        return Err(refused("its export time is not a date"));
    }
    if validate_debug_session_id(&manifest.session_id).is_err()
        || is_imported_id(&manifest.session_id)
        || manifest.session_id != session.id
    {
        return Err(refused("its session id is not one Keynobi records"));
    }
    validate_package_name(&session.package).map_err(refused)?;
    if manifest.package != session.package {
        return Err(refused("its manifest and session name different apps"));
    }
    if manifest.redaction != REDACTION_ENTRY {
        return Err(refused(format!(
            "its manifest does not name {REDACTION_ENTRY}"
        )));
    }
    let listed: HashSet<&str> = manifest.entries.iter().map(String::as_str).collect();
    let present: Vec<String> = [
        MANIFEST_ENTRY,
        SESSION_ENTRY,
        TIMELINE_ENTRY,
        REDACTION_ENTRY,
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(logs.keys().map(|seq| format!("logs/crash-{seq}.log")))
    .collect();
    if listed.len() != manifest.entries.len()
        || listed.len() != present.len()
        || present.iter().any(|name| !listed.contains(name.as_str()))
    {
        return Err(refused("its manifest does not list the files it holds"));
    }

    let mut omitted: Vec<SessionExportOmission> = manifest
        .omitted
        .iter()
        .take(MAX_IMPORT_OMISSIONS)
        .map(|o| SessionExportOmission {
            item: short(&o.item),
            reason: short(&o.reason),
        })
        .collect();

    // The timeline, up to the caps of a recorded session.
    let text = std::str::from_utf8(&timeline)
        .map_err(|_| refused(format!("{TIMELINE_ENTRY} is not UTF-8 text")))?;
    let mut events: Vec<DebugSessionEvent> = Vec::new();
    let mut events_file = Vec::new();
    let mut counts = DebugSessionCounts::default();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: DebugSessionEvent =
            parse_strict(line.as_bytes(), &format!("{TIMELINE_ENTRY} line {}", i + 1))?;
        if events.last().is_some_and(|last| event.seq <= last.seq) {
            return Err(refused(format!(
                "{TIMELINE_ENTRY} line {} is out of order",
                i + 1
            )));
        }
        let mut json = serde_json::to_vec(&event)
            .map_err(|e| AppError::Other(format!("Cannot serialize an event: {e}")))?;
        json.push(b'\n');
        let full = events.len() >= MAX_EVENTS_PER_SESSION as usize
            || (events_file.len() + json.len()) as u64 > MAX_SESSION_BYTES;
        if full {
            omitted.push(SessionExportOmission {
                item: format!("timeline events from #{}", event.seq),
                reason: format!(
                    "an imported session keeps at most {MAX_EVENTS_PER_SESSION} events"
                ),
            });
            break;
        }
        count_event(&mut counts, &event.event);
        events_file.extend_from_slice(&json);
        events.push(event);
    }

    // The log lines of crashes the timeline holds, up to the capture caps.
    let mut captures = BTreeMap::new();
    for (seq, data) in logs {
        let name = format!("logs/crash-{seq}.log");
        let has_capture = events.iter().any(|e| {
            e.seq == seq
                && matches!(
                    &e.event,
                    DebugSessionEventData::Crash(c) | DebugSessionEventData::Anr(c)
                        if c.capture.is_some()
                )
        });
        if !has_capture {
            omitted.push(omission(name, "no crash of the timeline kept these lines"));
            continue;
        }
        if captures.len() >= MAX_CAPTURES_PER_SESSION as usize {
            omitted.push(omission(
                name,
                format!("an imported session keeps at most {MAX_CAPTURES_PER_SESSION} captures"),
            ));
            continue;
        }
        let text =
            std::str::from_utf8(&data).map_err(|_| refused(format!("{name} is not UTF-8 text")))?;
        let (entries, truncated) = capture_entries(text);
        if truncated {
            omitted.push(omission(
                format!("older lines of {name}"),
                format!(
                    "a capture keeps at most {MAX_CAPTURE_ENTRIES} lines and {} KiB",
                    MAX_CAPTURE_BYTES / 1024
                ),
            ));
        }
        let mut file = Vec::new();
        for entry in &entries {
            let json = serde_json::to_vec(entry)
                .map_err(|e| AppError::Other(format!("Cannot serialize a log line: {e}")))?;
            file.extend_from_slice(&json);
            file.push(b'\n');
        }
        captures.insert(seq, file);
    }
    counts.captures = captures.len() as u32;

    let mut session = session;
    session.recorded_by = DebugSessionRecorder::Imported;
    session.kept = false;
    session.counts = counts;
    session.event_count = events.len() as u32;
    session.bytes = events_file.len() as u64;
    session.last_event_at = events
        .last()
        .map_or_else(|| session.opened_at.clone(), |e| e.at.clone());
    session.imported = Some(DebugSessionImport {
        file_name: truncate_chars(file_name, MAX_IMPORT_NAME_CHARS),
        exported_at: manifest.exported_at,
        imported_at: stamp(now),
        original_id: manifest.session_id,
        keynobi_version: truncate_chars(&manifest.keynobi_version, MAX_IMPORT_NAME_CHARS),
        omitted,
        redactions: report.rules,
    });
    Ok(PreparedImport {
        session,
        events: events_file,
        captures,
    })
}

fn omission(item: impl Into<String>, reason: impl Into<String>) -> SessionExportOmission {
    SessionExportOmission {
        item: item.into(),
        reason: reason.into(),
    }
}

// ── Storing ───────────────────────────────────────────────────────────────────

/// Create `path` for writing, failing when it exists, readable only by the user.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| AppError::io(path.display(), e))
}

fn new_import_id(now: DateTime<Utc>) -> String {
    format!("i-{}-{}", now.format("%Y%m%dT%H%M%SZ"), random_suffix())
}

/// Folders of imports that never got their `session.json` (a process that
/// died mid-import) and are older than [`ORPHAN_AGE`].
fn remove_unfinished(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = std::fs::symlink_metadata(entry.path())
            .ok()
            .filter(std::fs::Metadata::is_dir)
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > ORPHAN_AGE);
        if old
            && is_imported_id(&name)
            && validate_debug_session_id(&name).is_ok()
            && !entry.path().join(SESSION_FILE).is_file()
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Store `prepared` as a new imported session, within the import caps.
pub(super) fn store_import_in(
    data_dir: &Path,
    mut prepared: PreparedImport,
    now: DateTime<Utc>,
) -> Result<DebugSessionSummary, AppError> {
    let root = imports_dir(data_dir);
    with_data_lock_in(data_dir, || {
        std::fs::create_dir_all(&root).map_err(|e| AppError::io(root.display(), e))?;
        remove_unfinished(&root);
        if list_imported_in(data_dir).len() >= MAX_IMPORTED_SESSIONS {
            return Err(AppError::InvalidInput(format!(
                "Keynobi keeps at most {MAX_IMPORTED_SESSIONS} imported debug sessions. \
                 Delete one first."
            )));
        }
        if dir_size(&root, 2) + prepared.bytes() > MAX_IMPORTS_BYTES {
            return Err(AppError::InvalidInput(format!(
                "Imported debug sessions would take more than {} MiB. Delete one first.",
                MAX_IMPORTS_BYTES / (1024 * 1024)
            )));
        }
        let mut created = None;
        for _ in 0..8 {
            let id = new_import_id(now);
            match std::fs::create_dir(root.join(&id)) {
                Ok(()) => {
                    created = Some(id);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(AppError::io(root.display(), e)),
            }
        }
        let id = created.ok_or_else(|| AppError::Other("Cannot allocate a session id".into()))?;
        prepared.session.id = id.clone();
        let written = validate_within_root(&root, &id).and_then(|dir| {
            write_new(&dir.join(EVENTS_FILE), &prepared.events)?;
            if !prepared.captures.is_empty() {
                let captures = dir.join(crashes::CAPTURES_DIR);
                std::fs::create_dir(&captures).map_err(|e| AppError::io(captures.display(), e))?;
                for (seq, data) in &prepared.captures {
                    write_new(&captures.join(crashes::capture_file(*seq)), data)?;
                }
            }
            let manifest = serde_json::to_vec_pretty(&prepared.session)
                .map_err(|e| AppError::Other(format!("Cannot serialize the session: {e}")))?;
            // Last: a folder without it is an unfinished import.
            write_new(&dir.join(SESSION_FILE), &manifest)
        });
        if let Err(e) = written {
            let _ = std::fs::remove_dir_all(root.join(&id));
            return Err(e);
        }
        Ok(DebugSessionSummary::from(&prepared.session))
    })
    .map_err(AppError::Other)?
}

/// Import the bundle at `path`, which the user chose in the open dialog.
pub fn import_session_from(path: &Path) -> Result<DebugSessionSummary, AppError> {
    let file = std::fs::File::open(path).map_err(|e| AppError::io(path.display(), e))?;
    let meta = file
        .metadata()
        .map_err(|e| AppError::io(path.display(), e))?;
    if !meta.is_file() {
        return Err(refused("it is not a file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BUNDLE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| AppError::io(path.display(), e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    import_bytes_in(&data_dir(), &bytes, &name, Utc::now())
}

pub(super) fn import_bytes_in(
    data_dir: &Path,
    bytes: &[u8],
    file_name: &str,
    now: DateTime<Utc>,
) -> Result<DebugSessionSummary, AppError> {
    let prepared = prepare_import(bytes, file_name, now)?;
    store_import_in(data_dir, prepared, now)
}

// ── Listing and deleting ──────────────────────────────────────────────────────

/// The imported sessions, newest import first.
pub fn list_imported_sessions() -> Vec<DebugSessionSummary> {
    list_imported_in(&data_dir())
}

pub(super) fn list_imported_in(data_dir: &Path) -> Vec<DebugSessionSummary> {
    let Ok(entries) = std::fs::read_dir(imports_dir(data_dir)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|name| is_imported_id(name) && validate_debug_session_id(name).is_ok())
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.iter()
        .filter_map(|id| read_manifest(data_dir, id).ok())
        .filter(|s| s.recorded_by == DebugSessionRecorder::Imported)
        .take(MAX_IMPORTED_SESSIONS)
        .map(|s| DebugSessionSummary::from(&s))
        .collect()
}

/// Delete imported session `id`. Recorded sessions are never deleted here.
pub fn delete_imported_session(id: &str) -> Result<(), AppError> {
    delete_imported_in(&data_dir(), id)
}

pub(super) fn delete_imported_in(data_dir: &Path, id: &str) -> Result<(), AppError> {
    checked_id(id)?;
    if !is_imported_id(id) {
        return Err(AppError::InvalidInput(
            "Only an imported debug session can be deleted".into(),
        ));
    }
    with_data_lock_in(data_dir, || {
        match std::fs::remove_dir_all(imports_dir(data_dir).join(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(not_found(id)),
            Err(e) => Err(AppError::io(id, e)),
        }
    })
    .map_err(AppError::Other)?
}

#[cfg(test)]
mod tests {
    use super::super::export::{build_bundle_in, log_line};
    use super::*;
    use crate::models::build::{BuildActor, InstalledBuild};
    use crate::models::redaction::RedactionRule;
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

    fn install(dir: &Path) -> DebugSession {
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

    fn log_entry(id: u64, message: &str) -> ProcessedEntry {
        ProcessedEntry {
            id,
            timestamp: "09-25 10:32:05.123".into(),
            pid: 4242,
            tid: 4250,
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

    /// Add a crash event whose kept lines are `lines`; returns its seq.
    fn crash(dir: &Path, id: &str, lines: &[&str]) -> u32 {
        let mut s = read_manifest(dir, id).unwrap();
        let seq = s.event_count + 1;
        let captures = session_dir(dir, id).join(crashes::CAPTURES_DIR);
        std::fs::create_dir_all(&captures).unwrap();
        let text: String = lines
            .iter()
            .zip(1u64..)
            .map(|(l, i)| format!("{}\n", serde_json::to_string(&log_entry(i, l)).unwrap()))
            .collect();
        std::fs::write(captures.join(crashes::capture_file(seq)), text).unwrap();
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

    /// A recorded session with a crash and a bookmark, exported.
    fn exported(dir: &Path) -> (DebugSession, Vec<u8>) {
        let session = install(dir);
        crash(
            dir,
            &session.id,
            &[
                "FATAL EXCEPTION: main",
                "user jane@example.com at /Users/jane/work/MyApp/app",
                "    at com.example.Main(Main.kt:12)",
            ],
        );
        add_bookmark_in(
            dir,
            Some(&session.id),
            None,
            "saw it on R5CT1234ABC",
            None,
            Utc::now(),
        )
        .unwrap();
        let bundle = build_bundle_in(
            dir,
            &session.id,
            &SessionExportOptions::default(),
            Some("/Users/jane".into()),
            Utc::now(),
        )
        .unwrap();
        (session, bundle.bytes)
    }

    fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut file = archive.by_index(i).unwrap();
                let mut data = Vec::new();
                file.read_to_end(&mut data).unwrap();
                (file.name().to_string(), data)
            })
            .collect()
    }

    fn deflated() -> zip::write::SimpleFileOptions {
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
    }

    fn stored() -> zip::write::SimpleFileOptions {
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)
    }

    fn zip_with(
        files: &[(String, Vec<u8>)],
        options: zip::write::SimpleFileOptions,
    ) -> zip::ZipWriter<Cursor<Vec<u8>>> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            zip.start_file(name.as_str(), options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip
    }

    fn zip_of(files: &[(String, Vec<u8>)]) -> Vec<u8> {
        zip_with(files, deflated()).finish().unwrap().into_inner()
    }

    /// `bundle` with `change` applied to its files.
    fn edited(bundle: &[u8], change: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
        let mut files = unzip(bundle);
        change(&mut files);
        zip_of(&files)
    }

    fn file_mut<'a>(files: &'a mut [(String, Vec<u8>)], name: &str) -> &'a mut Vec<u8> {
        &mut files.iter_mut().find(|(n, _)| n == name).unwrap().1
    }

    /// Rename every `from` in `bytes` to `to`, of the same length, in the
    /// local headers and the central directory alike.
    fn renamed(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
        assert_eq!(from.len(), to.len());
        let mut out = bytes.to_vec();
        let mut at = 0;
        while let Some(i) = out[at..]
            .windows(from.len())
            .position(|w| w == from.as_bytes())
        {
            out[at + i..at + i + from.len()].copy_from_slice(to.as_bytes());
            at += i + from.len();
        }
        out
    }

    /// The manifest's `entries` rewritten to list `files`.
    fn relist(files: &mut [(String, Vec<u8>)]) {
        let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
        let manifest = file_mut(files, MANIFEST_ENTRY);
        let mut value: Value = serde_json::from_slice(manifest).unwrap();
        value["entries"] = serde_json::json!(names);
        *manifest = serde_json::to_vec(&value).unwrap();
    }

    fn import(dir: &Path, bytes: &[u8]) -> Result<DebugSessionSummary, AppError> {
        import_bytes_in(dir, bytes, "shared.zip", Utc::now())
    }

    fn refusal(dir: &Path, bytes: &[u8]) -> String {
        match import(dir, bytes) {
            Err(AppError::InvalidInput(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn nothing_imported(dir: &Path) {
        assert!(list_imported_in(dir).is_empty());
        let leftovers = std::fs::read_dir(imports_dir(dir))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftovers, 0, "an import left files");
    }

    #[test]
    fn an_exported_session_imports_as_its_redacted_content() {
        let dir = TempDir::new().unwrap();
        let (original, bytes) = exported(dir.path());
        let files = unzip(&bytes);
        let file = |name: &str| files.iter().find(|(n, _)| n == name).unwrap().1.clone();
        let summary = import(dir.path(), &bytes).unwrap();

        assert!(is_imported_id(&summary.id), "{}", summary.id);
        assert_eq!(summary.recorded_by, DebugSessionRecorder::Imported);

        let bundled: DebugSession = serde_json::from_slice(&file(SESSION_ENTRY)).unwrap();
        let detail = get_session_in(dir.path(), &summary.id, Utc::now()).unwrap();
        let mut imported = detail.session.clone();
        let origin = imported.imported.take().expect("import details");
        assert_eq!(origin.file_name, "shared.zip");
        assert_eq!(origin.original_id, original.id);
        assert_eq!(origin.keynobi_version, env!("CARGO_PKG_VERSION"));
        assert!(DateTime::parse_from_rfc3339(&origin.exported_at).is_ok());
        assert_eq!(origin.omitted[0].item, "R8 mappings");
        assert!(origin
            .redactions
            .iter()
            .any(|r| r.rule == RedactionRule::Emails && r.count > 0));
        imported.id = bundled.id.clone();
        imported.recorded_by = bundled.recorded_by;
        // The events are stored redacted, so their size differs.
        let stored = std::fs::metadata(session_dir(dir.path(), &summary.id).join(EVENTS_FILE));
        assert_eq!(imported.bytes, stored.unwrap().len());
        imported.bytes = bundled.bytes;
        assert_eq!(imported, bundled);
        assert_eq!(imported.device.serial, "<device-1>");
        assert_eq!(imported.project_root.as_deref(), Some("<project>"));

        let timeline: Vec<DebugSessionEvent> = std::str::from_utf8(&file(TIMELINE_ENTRY))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(detail.events, timeline);
        assert_eq!(detail.crashes.len(), 1);
        let text = serde_json::to_string(&detail.events).unwrap();
        assert!(!text.contains("jane@example.com"), "{text}");
        assert!(!text.contains("R5CT1234ABC"), "{text}");

        let seq = detail.crashes[0].seq;
        let capture = crashes::get_capture_in(dir.path(), &summary.id, seq, None).unwrap();
        let lines: String = capture
            .entries
            .iter()
            .map(|e| format!("{}\n", log_line(e)))
            .collect();
        assert_eq!(
            lines.as_bytes(),
            file(&format!("logs/crash-{seq}.log")).as_slice()
        );
        assert!(lines.contains("user <email-1> at <project>/app"), "{lines}");

        assert_eq!(list_imported_in(dir.path()), vec![summary.clone()]);
        let recorded = list_sessions_in(dir.path(), Utc::now());
        assert!(recorded.iter().all(|s| s.id != summary.id));
    }

    #[test]
    fn a_stored_bundle_imports_too() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        let stored = zip_with(&unzip(&bytes), stored())
            .finish()
            .unwrap()
            .into_inner();
        assert!(import(dir.path(), &stored).is_ok());
    }

    #[test]
    fn names_outside_the_layout_are_refused() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        for name in [
            "../session.json",
            "/session.json",
            "logs/../session.json",
            "./session.json",
            "logs\\crash-2.log",
            "logs/crash-02.log",
            "logs/crash-0.log",
            "logs/crash-2.log/",
            "LOGS/crash-2.log",
            "logs/crash-99999999999.log",
            "notes.txt",
            "attachments/screenshot.png",
        ] {
            let bundle = edited(&bytes, |files| {
                files.push((name.to_string(), b"x".to_vec()));
                relist(files);
            });
            let message = refusal(dir.path(), &bundle);
            assert!(
                message.contains("is not a file a debug session bundle holds"),
                "{name}: {message}"
            );
        }
        nothing_imported(dir.path());
    }

    #[test]
    fn a_traversal_name_is_refused_whatever_the_writer_allows() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        for (from, to) in [
            ("session.json", "/ession.json"),
            ("session.json", "../sion.json"),
            ("session.json", "..\\sion.json"),
            ("session.json", "session.jso\0"),
        ] {
            let message = refusal(dir.path(), &renamed(&bytes, from, to));
            assert!(
                message.contains("is not a file a debug session bundle holds"),
                "{to}: {message}"
            );
        }
        nothing_imported(dir.path());
    }

    #[test]
    fn symlink_and_directory_entries_are_refused() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        let mut zip = zip_with(&unzip(&bytes), deflated());
        zip.add_symlink("logs/crash-9.log", "/etc/passwd", deflated())
            .unwrap();
        let message = refusal(dir.path(), &zip.finish().unwrap().into_inner());
        assert!(
            message.contains("logs/crash-9.log is not a regular file"),
            "{message}"
        );

        let mut zip = zip_with(&unzip(&bytes), deflated());
        zip.add_directory("logs/", deflated()).unwrap();
        let message = refusal(dir.path(), &zip.finish().unwrap().into_inner());
        assert!(
            message.contains("logs/ is not a file a debug session bundle holds"),
            "{message}"
        );
        nothing_imported(dir.path());
    }

    #[test]
    fn a_name_held_twice_is_refused() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        let bundle = edited(&bytes, |files| {
            files.push(("sessioX.json".into(), b"{}".to_vec()));
        });
        let message = refusal(
            dir.path(),
            &renamed(&bundle, "sessioX.json", "session.json"),
        );
        assert!(message.contains("holds session.json twice"), "{message}");
        nothing_imported(dir.path());
    }

    #[test]
    fn caps_are_enforced_while_reading() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());

        let too_many = edited(&bytes, |files| {
            for seq in 100..(100 + MAX_BUNDLE_ENTRIES as u32) {
                files.push((format!("logs/crash-{seq}.log"), b"x".to_vec()));
            }
        });
        let message = refusal(dir.path(), &too_many);
        assert!(message.contains("a bundle holds at most 64"), "{message}");

        // Spaces inflate far past the ratio.
        let bomb = edited(&bytes, |files| {
            *file_mut(files, TIMELINE_ENTRY) = vec![b' '; 4 * 1024 * 1024];
        });
        let message = refusal(dir.path(), &bomb);
        assert!(
            message.contains("inflates more than 200 times"),
            "{message}"
        );

        let data = vec![0u8; MAX_BUNDLE_ENTRY_BYTES + 1];
        let mut total = 0;
        let err = read_capped(&mut data.as_slice(), "big", u64::MAX, &mut total).unwrap_err();
        assert!(
            err.to_string().contains("big is larger than 16 MiB"),
            "{err}"
        );

        let data = vec![0u8; 1024];
        let mut total = MAX_BUNDLE_UNCOMPRESSED_BYTES - 100;
        let err = read_capped(&mut data.as_slice(), "last", 1024, &mut total).unwrap_err();
        assert!(
            err.to_string().contains("larger than 100 MiB in total"),
            "{err}"
        );

        let mut total = 0;
        let fine = read_capped(&mut data.as_slice(), "small", 10, &mut total).unwrap();
        assert_eq!((fine.len(), total), (1024, 1024));

        let too_big = vec![0u8; MAX_BUNDLE_BYTES + 1];
        let message = refusal(dir.path(), &too_big);
        assert!(message.contains("larger than 50 MiB"), "{message}");
        nothing_imported(dir.path());
    }

    #[test]
    fn a_declared_size_over_the_cap_is_refused() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        let files: Vec<(String, Vec<u8>)> = unzip(&bytes)
            .into_iter()
            .map(|(name, data)| {
                if name == TIMELINE_ENTRY {
                    (name, vec![b'\n'; MAX_BUNDLE_ENTRY_BYTES + 1])
                } else {
                    (name, data)
                }
            })
            .collect();
        // Stored, so there is no ratio: only the size cap applies.
        let bundle = zip_with(&files, stored()).finish().unwrap().into_inner();
        let message = refusal(dir.path(), &bundle);
        assert!(
            message.contains("timeline.jsonl is larger than 16 MiB"),
            "{message}"
        );
    }

    #[test]
    fn a_truncated_or_corrupt_zip_is_refused() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        for cut in [0, 10, bytes.len() / 2, bytes.len() - 1] {
            let message = refusal(dir.path(), &bytes[..cut]);
            assert!(message.contains("damaged"), "cut {cut}: {message}");
        }
        // Flip bytes inside the first entry's deflated data.
        let mut corrupt = bytes.clone();
        for b in &mut corrupt[60..90] {
            *b ^= 0xff;
        }
        let message = refusal(dir.path(), &corrupt);
        assert!(
            message.starts_with("This bundle cannot be imported"),
            "{message}"
        );
        nothing_imported(dir.path());
    }

    #[test]
    fn an_unknown_bundle_version_is_refused_by_number() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        for (version, expected) in [
            (
                2,
                "it is bundle version 2, and this version of Keynobi reads version 1",
            ),
            (0, "bundle version 0 is unknown"),
        ] {
            let bundle = edited(&bytes, |files| {
                let manifest = file_mut(files, MANIFEST_ENTRY);
                let mut value: Value = serde_json::from_slice(manifest).unwrap();
                value["bundleVersion"] = serde_json::json!(version);
                // A newer manifest may have fields this version does not know.
                value["newField"] = serde_json::json!(true);
                *manifest = serde_json::to_vec(&value).unwrap();
            });
            let message = refusal(dir.path(), &bundle);
            assert!(message.contains(expected), "{message}");
        }
        nothing_imported(dir.path());
    }

    fn edit_json(files: &mut [(String, Vec<u8>)], name: &str, change: impl FnOnce(&mut Value)) {
        let file = file_mut(files, name);
        let mut value: Value = serde_json::from_slice(file).unwrap();
        change(&mut value);
        *file = serde_json::to_vec(&value).unwrap();
    }

    fn edit_text(
        files: &mut [(String, Vec<u8>)],
        name: &str,
        change: impl FnOnce(String) -> String,
    ) {
        let file = file_mut(files, name);
        *file = change(String::from_utf8(file.clone()).unwrap()).into_bytes();
    }

    #[test]
    fn malformed_or_unknown_json_is_refused() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        type Change = fn(&mut Vec<(String, Vec<u8>)>);
        let cases: [(Change, &str); 9] = [
            (
                |f| *file_mut(f, SESSION_ENTRY) = b"{".to_vec(),
                "session.json is not valid JSON",
            ),
            (
                |f| *file_mut(f, SESSION_ENTRY) = b"{\"id\": 1}".to_vec(),
                "session.json does not match its schema",
            ),
            (
                |f| edit_json(f, SESSION_ENTRY, |v| v["device"]["mood"] = "happy".into()),
                "does not know (device.mood)",
            ),
            (
                |f| edit_json(f, MANIFEST_ENTRY, |v| v["extra"] = 1.into()),
                "manifest.json does not match its schema",
            ),
            (
                |f| {
                    edit_text(f, TIMELINE_ENTRY, |t| {
                        let mut lines: Vec<String> = t.lines().map(str::to_string).collect();
                        let mut first: Value = serde_json::from_str(&lines[0]).unwrap();
                        first["data"]["extra"] = 0.into();
                        lines[0] = first.to_string();
                        lines.join("\n")
                    })
                },
                "timeline.jsonl line 1 has a field this version of Keynobi does not know (data.extra)",
            ),
            (
                |f| {
                    edit_text(f, TIMELINE_ENTRY, |t| {
                        t.replacen("\"kind\":\"install\"", "\"kind\":\"teleport\"", 1)
                    })
                },
                "does not match its schema",
            ),
            (
                |f| edit_json(f, REDACTION_ENTRY, |v| v["rules"][0]["extra"] = 1.into()),
                "redaction.json has a field this version of Keynobi does not know (rules[0].extra)",
            ),
            (
                |f| {
                    edit_text(f, TIMELINE_ENTRY, |t| {
                        let mut lines: Vec<&str> = t.lines().collect();
                        lines.swap(0, 1);
                        lines.join("\n")
                    })
                },
                "is out of order",
            ),
            (
                |f| edit_json(f, SESSION_ENTRY, |v| v["package"] = "com.other".into()),
                "its manifest and session name different apps",
            ),
        ];
        for (change, expected) in cases {
            let message = match import(dir.path(), &edited(&bytes, change)) {
                Err(AppError::InvalidInput(message)) => message,
                other => panic!("{expected}: {other:?}"),
            };
            assert!(message.contains(expected), "{expected}: {message}");
        }
        let unlisted = edited(&bytes, |f| {
            f.push(("logs/crash-77.log".into(), b"x".to_vec()));
        });
        let message = refusal(dir.path(), &unlisted);
        assert!(message.contains("does not list the files"), "{message}");
        nothing_imported(dir.path());
    }

    #[test]
    fn only_as_many_events_and_captures_as_a_recorded_session_keeps() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        let files = unzip(&bytes);
        let crash =
            std::str::from_utf8(&files.iter().find(|(n, _)| n == TIMELINE_ENTRY).unwrap().1)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str::<DebugSessionEvent>(l).unwrap())
                .find(|e| matches!(e.event, DebugSessionEventData::Crash(_)))
                .unwrap();
        let log = files
            .iter()
            .find(|(n, _)| n.starts_with("logs/"))
            .unwrap()
            .1
            .clone();
        let bundle = edited(&bytes, |files| {
            files.retain(|(n, _)| !n.starts_with("logs/"));
            let mut lines = String::new();
            for seq in 1..=MAX_EVENTS_PER_SESSION + 5 {
                let event = DebugSessionEvent {
                    seq,
                    ..crash.clone()
                };
                lines.push_str(&serde_json::to_string(&event).unwrap());
                lines.push('\n');
            }
            *file_mut(files, TIMELINE_ENTRY) = lines.into_bytes();
            let many_lines: String = (0..MAX_CAPTURE_ENTRIES + 3)
                .map(|i| format!("09-25 10:32:05.123  4242  4242 I Tag: line {i}\n"))
                .collect();
            files.push(("logs/crash-1.log".into(), many_lines.into_bytes()));
            for seq in 2..=(MAX_CAPTURES_PER_SESSION + 2) {
                files.push((format!("logs/crash-{seq}.log"), log.clone()));
            }
            // Past the imported timeline.
            let past = MAX_EVENTS_PER_SESSION + 3;
            files.push((format!("logs/crash-{past}.log"), log.clone()));
            relist(files);
        });
        let summary = import(dir.path(), &bundle).unwrap();
        assert_eq!(summary.event_count, MAX_EVENTS_PER_SESSION);
        assert_eq!(summary.counts.crashes, MAX_EVENTS_PER_SESSION);
        assert_eq!(summary.counts.captures, MAX_CAPTURES_PER_SESSION);
        let omitted: Vec<String> = read_manifest(dir.path(), &summary.id)
            .unwrap()
            .imported
            .unwrap()
            .omitted
            .into_iter()
            .map(|o| o.item)
            .collect();
        for item in [
            format!("timeline events from #{}", MAX_EVENTS_PER_SESSION + 1),
            "older lines of logs/crash-1.log".to_string(),
            format!("logs/crash-{}.log", MAX_CAPTURES_PER_SESSION + 1),
            format!("logs/crash-{}.log", MAX_EVENTS_PER_SESSION + 3),
        ] {
            assert!(omitted.contains(&item), "{item} not in {omitted:?}");
        }
        let first = crashes::get_capture_in(dir.path(), &summary.id, 1, None).unwrap();
        assert_eq!(first.entries.len(), MAX_CAPTURE_ENTRIES);
        assert_eq!(
            first.entries.last().unwrap().message,
            format!("line {}", MAX_CAPTURE_ENTRIES + 2)
        );
        let kept =
            std::fs::read_dir(session_dir(dir.path(), &summary.id).join(crashes::CAPTURES_DIR))
                .unwrap()
                .count();
        assert_eq!(kept, MAX_CAPTURES_PER_SESSION as usize);
    }

    #[test]
    fn imported_sessions_are_never_attributed_written_or_pruned_with_recorded_ones() {
        let dir = TempDir::new().unwrap();
        let (recorded, bytes) = exported(dir.path());
        let summary = import(dir.path(), &bytes).unwrap();
        let id = summary.id.as_str();
        let before = read_manifest(dir.path(), id).unwrap();
        let events_before = read_events(dir.path(), id).unwrap();

        // Not in the index, even rebuilt.
        assert!(load_index_from(dir.path()).iter().all(|s| s.id != id));
        std::fs::remove_file(sessions_dir(dir.path()).join(INDEX_FILE)).unwrap();
        assert!(load_index_from(dir.path()).iter().all(|s| s.id != id));

        // Events of its app on its original device go to the recorded session.
        let recorded_now = record_for_device_in(
            dir.path(),
            &phone(),
            Some("com.example"),
            None,
            DebugSessionEventData::DeviceOnline(DebugSessionDeviceChange {
                serial: "R5CT1234ABC".into(),
            }),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(recorded_now, 1);
        // A new install supersedes the recorded session only.
        install(dir.path());
        assert!(read_manifest(dir.path(), &recorded.id)
            .unwrap()
            .closed_at
            .is_some());

        // Retention with no room left removes every recorded session, not it.
        let tight = Retention {
            days: 1,
            max_folder_mb: 0,
        };
        let later = Utc::now() + chrono::Duration::days(30);
        prune_persisted_in(dir.path(), tight, later).unwrap();
        assert!(load_index_from(dir.path()).is_empty());

        assert_eq!(read_manifest(dir.path(), id).unwrap(), before);
        assert_eq!(read_events(dir.path(), id).unwrap(), events_before);
        assert_eq!(list_imported_in(dir.path()).len(), 1);
    }

    #[test]
    fn imported_sessions_are_read_only_and_can_be_deleted() {
        let dir = TempDir::new().unwrap();
        let (recorded, bytes) = exported(dir.path());
        let id = import(dir.path(), &bytes).unwrap().id;
        let now = Utc::now();
        let read_only = |result: Result<(), AppError>| match result {
            Err(AppError::InvalidInput(m)) => assert!(m.contains("read-only"), "{m}"),
            other => panic!("{other:?}"),
        };
        read_only(end_session_in(dir.path(), &id, RETAIN, now));
        read_only(set_kept_in(dir.path(), &id, true, now));
        read_only(add_bookmark_in(dir.path(), Some(&id), None, "note", None, now).map(|_| ()));
        read_only(
            build_bundle_in(dir.path(), &id, &SessionExportOptions::default(), None, now)
                .map(|_| ()),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        read_only(
            runtime
                .block_on(crashes::refresh_exit_reasons_in(
                    dir.path(),
                    &id,
                    Path::new("/nonexistent/adb"),
                ))
                .map(|_| ()),
        );

        // A bookmark that names no session never lands on it.
        let event = add_bookmark_in(dir.path(), None, None, "note", None, now).unwrap();
        assert_eq!(
            event.seq,
            read_manifest(dir.path(), &recorded.id).unwrap().event_count
        );

        let err = delete_imported_in(dir.path(), &recorded.id).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err}");
        assert!(read_manifest(dir.path(), &recorded.id).is_ok());
        delete_imported_in(dir.path(), &id).unwrap();
        assert!(list_imported_in(dir.path()).is_empty());
        let err = delete_imported_in(dir.path(), &id).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
        let err = get_session_in(dir.path(), &id, now).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
    }

    #[test]
    fn agents_list_read_and_compare_imported_sessions() {
        let dir = TempDir::new().unwrap();
        let (recorded, bytes) = exported(dir.path());
        let id = import(dir.path(), &bytes).unwrap().id;
        let now = Utc::now();

        let mut all = list_sessions_in(dir.path(), now);
        all.extend(list_imported_in(dir.path()));
        let (listed, total) =
            agent::filter_sessions(all.clone(), &agent::SessionFilter::default(), 10);
        assert_eq!(total, 2);
        assert_eq!(listed.last().map(|s| s.id.as_str()), Some(id.as_str()));
        assert!(agent::agent_line(&listed[1]).contains(" | imported | "));
        for state in ["open", "closed"] {
            let filter = agent::SessionFilter {
                state: Some(agent::StateFilter::parse(state).unwrap()),
                ..Default::default()
            };
            let (listed, _) = agent::filter_sessions(all.clone(), &filter, 10);
            assert!(listed.iter().all(|s| s.id != id), "{state}");
        }

        let request = agent::AgentSessionRequest {
            id: id.clone(),
            before_seq: None,
            max_events: 10,
            capture_seq: None,
            log_lines: 10,
        };
        let read = agent::session_for_agent_in(dir.path(), &request, now).unwrap();
        assert_eq!(read["session"]["recorded_by"], "imported");
        assert_eq!(read["session"]["imported"]["file_name"], "shared.zip");

        let compared =
            compare::compare_sessions_in(dir.path(), Some(&recorded.id), Some(&id), now).unwrap();
        assert_eq!(compared.to.recorded_by, "imported");
        // The default pair is chosen among recorded sessions only.
        assert!(compare::default_pair(&list_sessions_in(dir.path(), now))
            .is_none_or(|(a, b)| a != id && b != id));
    }

    #[test]
    fn at_most_the_import_cap_is_kept() {
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        for _ in 0..MAX_IMPORTED_SESSIONS {
            import(dir.path(), &bytes).unwrap();
        }
        let message = match import(dir.path(), &bytes) {
            Err(AppError::InvalidInput(m)) => m,
            other => panic!("{other:?}"),
        };
        assert!(message.contains("at most 20 imported"), "{message}");
        assert_eq!(list_imported_in(dir.path()).len(), MAX_IMPORTED_SESSIONS);
    }

    #[test]
    fn imported_files_are_private_and_an_unfinished_import_is_ignored() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let (_, bytes) = exported(dir.path());
        let id = import(dir.path(), &bytes).unwrap().id;
        for name in [SESSION_FILE, EVENTS_FILE] {
            let mode = std::fs::metadata(session_dir(dir.path(), &id).join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
        let unfinished = imports_dir(dir.path()).join("i-20260101T000000Z-000000000000");
        std::fs::create_dir(&unfinished).unwrap();
        std::fs::write(unfinished.join(EVENTS_FILE), "").unwrap();
        assert_eq!(list_imported_in(dir.path()).len(), 1);
    }
}
