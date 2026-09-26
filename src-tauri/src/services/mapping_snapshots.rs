//! Copies of the R8 mappings a successful build wrote, kept after the next
//! build of the variant overwrites `build/outputs/mapping/<variant>/mapping.txt`.
//!
//! A snapshot is `<data dir>/mappings/<sha256>.txt`, so identical mappings are
//! stored once. A mapping is first copied to a private temporary file without
//! the data lock (mappings can be hundreds of MB). Publishing the copy, linking
//! it to its build record, saving the history, and pruning snapshots neither
//! the kept history nor an installed build (`installed_builds`) references all
//! happen in one critical section under the data lock, so another process
//! never prunes a snapshot its history names.

use crate::models::build::{BuildRecord, InstalledBuild, MappingSnapshot};
use crate::models::debug_session::DebugSessionSummary;
use crate::models::error::AppError;
use crate::services::gradle_modules;
use crate::services::settings_manager::unique_tmp_path;
use crate::utils::path::{resolve_project_file, validate_within_root};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Largest mapping copied; larger ones are skipped with a logged reason.
/// Large apps write 50–150 MB mappings, so this leaves room for them while
/// bounding one copy to about a second of disk I/O and the snapshot folder to
/// `MAX_MAPPING_SNAPSHOTS` times this.
pub const MAX_MAPPING_BYTES: u64 = 256 * 1024 * 1024;

/// Most snapshot files kept for the build history. Retention normally keeps
/// only what the history references; past this backstop the least recently
/// saved go first, even when a record still names them. Snapshots pinned by an
/// installed build are never removed and do not count; there are at most
/// `installed_builds::MAX_INSTALLED_TARGETS` of those.
pub const MAX_MAPPING_SNAPSHOTS: usize = 32;

/// Most mappings recorded for one build (application modules × variants).
pub const MAX_MAPPINGS_PER_BUILD: usize = 8;

/// Longest `pg_map_id` kept. R8 writes a short hex hash.
const MAX_PG_MAP_ID_LEN: usize = 64;

/// Bytes at the start of a mapping searched for the `# pg_map_id:` header.
const MAX_HEADER_BYTES: usize = 64 * 1024;

/// A temporary copy this old was left by a process that stopped mid-copy.
const STALE_TMP_AGE: Duration = Duration::from_secs(60 * 60);

const COPY_CHUNK_BYTES: usize = 1024 * 1024;

const MAPPINGS_DIR: &str = "mappings";
const TMP_PREFIX: &str = "snapshot";

/// The folder holding the snapshots.
pub fn mappings_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(MAPPINGS_DIR)
}

/// The saved copy of the mapping with this SHA-256, or `None` when `sha256`
/// is not a lowercase hex SHA-256 (records are read from a file on disk).
pub fn snapshot_path(data_dir: &Path, sha256: &str) -> Option<PathBuf> {
    is_sha256_hex(sha256).then(|| mappings_dir(data_dir).join(format!("{sha256}.txt")))
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Where a finished build's mappings (and APKs, see `installed_builds`) are,
/// and when it started.
#[derive(Debug, Clone)]
pub struct MappingSource {
    pub gradle_root: PathBuf,
    /// Files modified before this were written by an earlier build.
    pub build_started: SystemTime,
}

/// A mapping copied to a private temporary file, waiting to be published by
/// [`publish_snapshots`]. Dropping it unpublished removes the copy.
#[derive(Debug)]
pub struct PreparedMapping {
    pub snapshot: MappingSnapshot,
    tmp: Option<PathBuf>,
}

impl Drop for PreparedMapping {
    fn drop(&mut self) {
        if let Some(tmp) = self.tmp.take() {
            let _ = std::fs::remove_file(tmp);
        }
    }
}

/// A mapping that was not saved, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedMapping {
    pub path: PathBuf,
    pub reason: String,
}

/// What [`prepare_snapshots`] copied and what it skipped.
#[derive(Debug, Default)]
pub struct Prepared {
    pub mappings: Vec<PreparedMapping>,
    pub skipped: Vec<SkippedMapping>,
}

/// A `mapping.txt` found in an application module's build outputs.
#[derive(Debug, Clone)]
struct Candidate {
    module: String,
    variant: String,
    /// Canonical, inside the project and the module's `build/outputs`.
    path: PathBuf,
}

/// Copy every mapping the build at `source` wrote to a temporary file in the
/// snapshot folder, hashing it on the way. Stale mappings (modified before
/// the build started) are left alone. Failures are logged and skipped; they
/// never fail the build. Runs without the data lock.
pub fn prepare_snapshots(data_dir: &Path, source: &MappingSource) -> Prepared {
    let mut prepared = Prepared::default();
    let candidates = find_mappings(&source.gradle_root, &mut prepared.skipped);
    for candidate in candidates {
        if prepared.mappings.len() >= MAX_MAPPINGS_PER_BUILD {
            prepared.skipped.push(SkippedMapping {
                path: candidate.path,
                reason: format!("more than {MAX_MAPPINGS_PER_BUILD} mappings in one build"),
            });
            continue;
        }
        match copy_mapping(data_dir, &candidate, source.build_started) {
            Ok(Some(mapping)) => prepared.mappings.push(mapping),
            Ok(None) => {}
            Err(reason) => prepared.skipped.push(SkippedMapping {
                path: candidate.path,
                reason,
            }),
        }
    }
    for skipped in &prepared.skipped {
        tracing::warn!(
            "R8 mapping {} was not saved: {}",
            skipped.path.display(),
            skipped.reason
        );
    }
    prepared
}

/// `mapping/*/mapping.txt` under each application module's `build/outputs`.
/// Every directory is resolved to its canonical path inside the project
/// before it is listed, and every file to a regular file inside the module's
/// canonical `build/outputs`, so no symlink leads out of the project.
fn find_mappings(gradle_root: &Path, skipped: &mut Vec<SkippedMapping>) -> Vec<Candidate> {
    let Ok(root) = gradle_root.canonicalize() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for module in gradle_modules::application_modules(&root) {
        let outputs_rel = match module.relative_dir(&root).as_str() {
            "." => "build/outputs".to_string(),
            dir => format!("{dir}/build/outputs"),
        };
        let resolved = validate_within_root(&root, &outputs_rel)
            .and_then(|outputs| Ok((validate_within_root(&outputs, "mapping")?, outputs)));
        let (mapping_dir, outputs) = match resolved {
            Ok(dirs) => dirs,
            Err(AppError::NotFound(_)) => continue,
            Err(e) => {
                skipped.push(SkippedMapping {
                    path: module.dir.join("build/outputs/mapping"),
                    reason: e.to_string(),
                });
                continue;
            }
        };
        let Ok(entries) = std::fs::read_dir(&mapping_dir) else {
            continue;
        };
        let mut variants: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || t.is_symlink()))
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        variants.sort();
        for variant in variants {
            match resolve_project_file(&outputs, &format!("mapping/{variant}/mapping.txt")) {
                Ok(path) => found.push(Candidate {
                    module: module.path.clone(),
                    variant,
                    path,
                }),
                // A variant folder without a mapping (only `missing_rules.txt`).
                Err(AppError::NotFound(_)) => {}
                Err(e) => skipped.push(SkippedMapping {
                    path: mapping_dir.join(&variant).join("mapping.txt"),
                    reason: e.to_string(),
                }),
            }
        }
    }
    found
}

/// Copy `candidate` to a temporary file while hashing it, in one pass and in
/// bounded chunks. `Ok(None)` when the mapping predates the build.
fn copy_mapping(
    data_dir: &Path,
    candidate: &Candidate,
    build_started: SystemTime,
) -> Result<Option<PreparedMapping>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    // The path is canonical; refuse a symlink swapped in since it was resolved.
    let source = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&candidate.path)
        .map_err(|e| format!("cannot open it: {e}"))?;
    let meta = source
        .metadata()
        .map_err(|e| format!("cannot read its metadata: {e}"))?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    let modified = meta
        .modified()
        .map_err(|e| format!("cannot read its modification time: {e}"))?;
    if modified < build_started {
        return Ok(None);
    }
    if meta.len() > MAX_MAPPING_BYTES {
        return Err(too_large(meta.len()));
    }

    let dir = mappings_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let tmp = unique_tmp_path(&dir.join(TMP_PREFIX));
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("cannot create {}: {e}", tmp.display()))?;
    // From here on, dropping `prepared` removes the partial copy.
    let mut prepared = PreparedMapping {
        snapshot: MappingSnapshot {
            module: candidate.module.clone(),
            variant: candidate.variant.clone(),
            sha256: String::new(),
            bytes: 0,
            pg_map_id: None,
        },
        tmp: Some(tmp),
    };

    let mut hasher = Sha256::new();
    let mut header: Vec<u8> = Vec::new();
    let mut bytes: u64 = 0;
    let mut buf = vec![0u8; COPY_CHUNK_BYTES];
    // One byte past the cap tells a file that grew since it was measured.
    let mut reader = source.take(MAX_MAPPING_BYTES + 1);
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("cannot read it: {e}")),
        };
        bytes += n as u64;
        if bytes > MAX_MAPPING_BYTES {
            return Err(too_large(bytes));
        }
        let chunk = &buf[..n];
        hasher.update(chunk);
        if header.len() < MAX_HEADER_BYTES {
            let take = n.min(MAX_HEADER_BYTES - header.len());
            header.extend_from_slice(&chunk[..take]);
        }
        out.write_all(chunk)
            .map_err(|e| format!("cannot write the copy: {e}"))?;
    }
    out.flush()
        .map_err(|e| format!("cannot write the copy: {e}"))?;

    prepared.snapshot.sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    prepared.snapshot.bytes = bytes;
    prepared.snapshot.pg_map_id = parse_pg_map_id(&header);
    Ok(Some(prepared))
}

fn too_large(bytes: u64) -> String {
    format!(
        "{bytes} bytes is over the {} MiB limit for saved mappings",
        MAX_MAPPING_BYTES / (1024 * 1024)
    )
}

/// The value of the `# pg_map_id:` line in the comment header R8 writes at
/// the top of a mapping. Only the leading `#` lines are searched.
pub fn parse_pg_map_id(header: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(header);
    for line in text.lines() {
        let comment = line.strip_prefix('#')?;
        if let Some(value) = comment.trim_start().strip_prefix("pg_map_id:") {
            let id = value.trim();
            let valid = !id.is_empty()
                && id.len() <= MAX_PG_MAP_ID_LEN
                && id.bytes().all(|b| b.is_ascii_alphanumeric());
            return valid.then(|| id.to_string());
        }
    }
    None
}

/// Move prepared copies to `mappings/<sha256>.txt` and return what was saved.
/// A mapping already saved is kept (and marked as saved now) and the copy
/// discarded. A copy that cannot be published is logged and left out.
///
/// Callers hold the data lock and link the result to a record in the same
/// critical section, before [`prune_snapshots`] can run.
pub fn publish_snapshots(data_dir: &Path, prepared: Vec<PreparedMapping>) -> Vec<MappingSnapshot> {
    let mut published = Vec::new();
    for mut mapping in prepared {
        let Some(tmp) = mapping.tmp.take() else {
            continue;
        };
        let Some(target) = snapshot_path(data_dir, &mapping.snapshot.sha256) else {
            let _ = std::fs::remove_file(&tmp);
            continue;
        };
        let existing = std::fs::symlink_metadata(&target).is_ok_and(|m| m.is_file());
        if existing {
            let _ = std::fs::remove_file(&tmp);
            // Retention's backstop removes the least recently saved first.
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .open(&target)
                .and_then(|f| f.set_modified(SystemTime::now()));
        } else if let Err(e) = std::fs::rename(&tmp, &target) {
            let _ = std::fs::remove_file(&tmp);
            tracing::warn!(
                "R8 mapping of {} {} was not saved: {e}",
                mapping.snapshot.module,
                mapping.snapshot.variant
            );
            continue;
        }
        published.push(mapping.snapshot.clone());
    }
    published
}

/// The snapshots retention keeps, by SHA-256.
#[derive(Debug, Default)]
pub struct KeptMappings {
    /// Named by the kept build history.
    pub referenced: HashSet<String>,
    /// Named by what Keynobi installed on a device (`installed_builds`),
    /// whether or not the build is still in the history, or by a debug
    /// session marked Keep. Never removed.
    pub pinned: HashSet<String>,
}

/// The snapshots retention keeps: those the kept build history references,
/// those pinned by the APKs installed on devices, and those of debug sessions
/// marked Keep. Anything else that must outlive its build record is added here.
pub fn mappings_to_keep<'a>(
    history: impl IntoIterator<Item = &'a BuildRecord>,
    installed: &[InstalledBuild],
    sessions: &[DebugSessionSummary],
) -> KeptMappings {
    KeptMappings {
        referenced: history
            .into_iter()
            .flat_map(|record| record.mappings.iter().map(|m| m.sha256.clone()))
            .collect(),
        pinned: installed
            .iter()
            .flat_map(|install| install.mappings.iter().map(|m| m.sha256.clone()))
            .chain(
                sessions
                    .iter()
                    .filter(|s| s.kept)
                    .flat_map(|s| s.mapping_sha256s.iter().cloned()),
            )
            .collect(),
    }
}

/// Delete snapshots not in `keep`, temporary copies abandoned more than
/// `STALE_TMP_AGE` ago, and, past `MAX_MAPPING_SNAPSHOTS` unpinned ones, the
/// least recently saved that are not pinned. Returns how many files were
/// removed. Callers hold the data lock.
pub fn prune_snapshots(data_dir: &Path, keep: &KeptMappings) -> usize {
    let Ok(entries) = std::fs::read_dir(mappings_dir(data_dir)) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    let mut kept: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        if let Some(sha) = name.strip_suffix(".txt").filter(|s| is_sha256_hex(s)) {
            if keep.pinned.contains(sha) {
                // A device runs the APK it belongs to: kept, backstop or not.
                continue;
            }
            if keep.referenced.contains(sha) {
                kept.push((path, mtime));
            } else if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        } else if name.starts_with(TMP_PREFIX) && name.ends_with(".tmp") {
            let abandoned = now
                .duration_since(mtime)
                .is_ok_and(|age| age > STALE_TMP_AGE);
            if abandoned && std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    if kept.len() > MAX_MAPPING_SNAPSHOTS {
        kept.sort_by_key(|(_, mtime)| *mtime);
        let excess = kept.len() - MAX_MAPPING_SNAPSHOTS;
        for (path, _) in kept.into_iter().take(excess) {
            if std::fs::remove_file(&path).is_ok() {
                tracing::warn!(
                    "Removed R8 mapping {} still named by the build history: more than \
                     {MAX_MAPPING_SNAPSHOTS} saved",
                    path.display()
                );
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const HEADER: &str = "# compiler: R8\n# compiler_version: 8.5.35\n# min_api: 24\n\
        # {\"id\":\"com.android.tools.r8.mapping\",\"version\":\"2.2\"}\n\
        # pg_map_id: 6b1c2f0\n# pg_map_hash: SHA-256 6b1c2f0aa\n";

    /// A project whose `app` module applies the application plugin.
    fn project() -> TempDir {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("settings.gradle.kts"),
            "include(\":app\")\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("app")).unwrap();
        std::fs::write(
            tmp.path().join("app/build.gradle.kts"),
            "plugins { id(\"com.android.application\") }\n",
        )
        .unwrap();
        tmp
    }

    fn write_mapping(root: &Path, variant: &str, text: &str) -> PathBuf {
        let dir = root.join("app/build/outputs/mapping").join(variant);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mapping.txt");
        std::fs::write(&path, text).unwrap();
        path
    }

    fn set_mtime(path: &Path, when: SystemTime) {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    fn source(root: &Path, started: SystemTime) -> MappingSource {
        MappingSource {
            gradle_root: root.to_path_buf(),
            build_started: started,
        }
    }

    fn an_hour_ago() -> SystemTime {
        SystemTime::now() - Duration::from_secs(3600)
    }

    fn saved_files(data: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(mappings_dir(data))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn a_mapping_written_by_the_build_is_saved_under_its_hash() {
        let project = project();
        let data = TempDir::new().unwrap();
        let text = format!("{HEADER}com.example.Main -> a:\n");
        write_mapping(project.path(), "release", &text);

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));
        assert!(prepared.skipped.is_empty(), "{:?}", prepared.skipped);
        let saved = publish_snapshots(data.path(), prepared.mappings);

        let sha = sha256_hex(text.as_bytes());
        assert_eq!(
            saved,
            vec![MappingSnapshot {
                module: ":app".into(),
                variant: "release".into(),
                sha256: sha.clone(),
                bytes: text.len() as u64,
                pg_map_id: Some("6b1c2f0".into()),
            }]
        );
        assert_eq!(saved_files(data.path()), vec![format!("{sha}.txt")]);
        let copy = std::fs::read_to_string(snapshot_path(data.path(), &sha).unwrap()).unwrap();
        assert_eq!(copy, text);
    }

    #[test]
    fn a_mapping_older_than_the_build_is_not_saved() {
        let project = project();
        let data = TempDir::new().unwrap();
        let stale = write_mapping(project.path(), "release", HEADER);
        set_mtime(&stale, an_hour_ago());
        write_mapping(project.path(), "debug", "fresh -> a:\n");

        let started = SystemTime::now() - Duration::from_secs(60);
        let prepared = prepare_snapshots(data.path(), &source(project.path(), started));
        let saved = publish_snapshots(data.path(), prepared.mappings);

        let variants: Vec<&str> = saved.iter().map(|m| m.variant.as_str()).collect();
        assert_eq!(variants, vec!["debug"]);
        assert_eq!(saved_files(data.path()).len(), 1);
    }

    #[test]
    fn identical_mappings_are_stored_once() {
        let project = project();
        let data = TempDir::new().unwrap();
        write_mapping(project.path(), "release", HEADER);
        write_mapping(project.path(), "staging", HEADER);

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));
        let saved = publish_snapshots(data.path(), prepared.mappings);
        // Once more, as the next build of an unchanged app would.
        let again = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));
        let saved_again = publish_snapshots(data.path(), again.mappings);

        assert_eq!(saved.len(), 2);
        assert_eq!(saved_again.len(), 2);
        assert_eq!(saved[0].sha256, saved[1].sha256);
        assert_eq!(
            saved_files(data.path()),
            vec![format!("{}.txt", saved[0].sha256)]
        );
    }

    #[test]
    fn a_mapping_linked_outside_the_project_is_refused() {
        let project = project();
        let outside = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        let secret = outside.path().join("mapping.txt");
        std::fs::write(&secret, "secret -> a:\n").unwrap();
        let dir = project.path().join("app/build/outputs/mapping/release");
        std::fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&secret, dir.join("mapping.txt")).unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        assert!(prepared.mappings.is_empty());
        assert_eq!(prepared.skipped.len(), 1);
        assert!(
            prepared.skipped[0].reason.contains("outside the project"),
            "{:?}",
            prepared.skipped
        );
        assert!(saved_files(data.path()).is_empty());
    }

    #[test]
    fn a_mapping_folder_linked_outside_the_project_is_not_listed() {
        let project = project();
        let outside = TempDir::new().unwrap();
        let data = TempDir::new().unwrap();
        std::fs::create_dir_all(outside.path().join("release")).unwrap();
        std::fs::write(outside.path().join("release/mapping.txt"), "x -> a:\n").unwrap();
        let outputs = project.path().join("app/build/outputs");
        std::fs::create_dir_all(&outputs).unwrap();
        std::os::unix::fs::symlink(outside.path(), outputs.join("mapping")).unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        assert!(prepared.mappings.is_empty());
        assert_eq!(prepared.skipped.len(), 1, "{:?}", prepared.skipped);
    }

    #[test]
    fn a_mapping_linked_inside_the_build_outputs_is_saved() {
        let project = project();
        let data = TempDir::new().unwrap();
        let real = write_mapping(project.path(), "release", HEADER);
        let dir = project.path().join("app/build/outputs/mapping/copy");
        std::fs::create_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&real, dir.join("mapping.txt")).unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        assert_eq!(prepared.mappings.len(), 2, "{:?}", prepared.skipped);
    }

    #[test]
    fn a_mapping_over_the_size_cap_is_skipped() {
        let project = project();
        let data = TempDir::new().unwrap();
        let path = write_mapping(project.path(), "release", HEADER);
        // Sparse: no disk space used.
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_MAPPING_BYTES + 1)
            .unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        assert!(prepared.mappings.is_empty());
        assert_eq!(prepared.skipped.len(), 1);
        assert!(prepared.skipped[0].reason.contains("256 MiB"));
        assert!(saved_files(data.path()).is_empty());
    }

    #[test]
    fn a_mapping_at_the_size_cap_is_saved() {
        let project = project();
        let data = TempDir::new().unwrap();
        let path = write_mapping(project.path(), "release", HEADER);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_MAPPING_BYTES)
            .unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        assert_eq!(prepared.mappings.len(), 1, "{:?}", prepared.skipped);
        assert_eq!(prepared.mappings[0].snapshot.bytes, MAX_MAPPING_BYTES);
        assert_eq!(
            prepared.mappings[0].snapshot.pg_map_id.as_deref(),
            Some("6b1c2f0")
        );
    }

    #[test]
    fn an_unwritable_snapshot_folder_is_reported_and_leaves_nothing() {
        let project = project();
        let data = TempDir::new().unwrap();
        write_mapping(project.path(), "release", HEADER);
        // The folder cannot be created: a file has its name.
        std::fs::write(mappings_dir(data.path()), "").unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        assert!(prepared.mappings.is_empty());
        assert_eq!(prepared.skipped.len(), 1);
        assert!(prepared.skipped[0].reason.contains("cannot create"));
    }

    #[test]
    fn an_unreadable_mapping_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let project = project();
        let data = TempDir::new().unwrap();
        let path = write_mapping(project.path(), "release", HEADER);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(prepared.mappings.is_empty());
        assert_eq!(prepared.skipped.len(), 1);
        assert!(prepared.skipped[0].reason.contains("cannot open"));
    }

    #[test]
    fn a_mapping_that_vanished_is_reported() {
        let data = TempDir::new().unwrap();
        let candidate = Candidate {
            module: ":app".into(),
            variant: "release".into(),
            path: data.path().join("gone/mapping.txt"),
        };
        let result = copy_mapping(data.path(), &candidate, an_hour_ago());
        assert!(matches!(result, Err(reason) if reason.contains("cannot open")));
    }

    #[test]
    fn an_unpublished_copy_is_removed() {
        let project = project();
        let data = TempDir::new().unwrap();
        write_mapping(project.path(), "release", HEADER);

        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));
        assert_eq!(saved_files(data.path()).len(), 1);
        drop(prepared);

        assert!(saved_files(data.path()).is_empty());
    }

    #[test]
    fn mappings_are_capped_per_build() {
        let project = project();
        let data = TempDir::new().unwrap();
        for i in 0..MAX_MAPPINGS_PER_BUILD + 2 {
            write_mapping(project.path(), &format!("v{i:02}"), &format!("{i} -> a:\n"));
        }
        let prepared = prepare_snapshots(data.path(), &source(project.path(), an_hour_ago()));
        assert_eq!(prepared.mappings.len(), MAX_MAPPINGS_PER_BUILD);
        assert_eq!(prepared.skipped.len(), 2);
    }

    #[test]
    fn pg_map_id_comes_from_the_header() {
        assert_eq!(
            parse_pg_map_id(HEADER.as_bytes()).as_deref(),
            Some("6b1c2f0")
        );
        assert_eq!(
            parse_pg_map_id(b"#pg_map_id:abc123\n").as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn pg_map_id_is_none_without_the_header() {
        assert_eq!(parse_pg_map_id(b"# compiler: R8\ncom.a.B -> a:\n"), None);
        assert_eq!(parse_pg_map_id(b""), None);
        // Only the leading comment block counts.
        assert_eq!(
            parse_pg_map_id(b"com.a.B -> a:\n# pg_map_id: 6b1c2f0\n"),
            None
        );
        assert_eq!(parse_pg_map_id(b"# pg_map_id: not/an id\n"), None);
    }

    fn record_with(id: u32, shas: &[&str]) -> BuildRecord {
        BuildRecord {
            id,
            task: "assembleRelease".into(),
            status: crate::models::build::BuildStatus::Cancelled,
            errors: vec![],
            started_at: String::new(),
            project_root: None,
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings: shas
                .iter()
                .map(|sha| MappingSnapshot {
                    module: ":app".into(),
                    variant: "release".into(),
                    sha256: sha.to_string(),
                    bytes: 1,
                    pg_map_id: None,
                })
                .collect(),
            apks: Vec::new(),
        }
    }

    fn put_snapshot(data: &Path, n: usize) -> String {
        let sha = format!("{n:064x}");
        std::fs::create_dir_all(mappings_dir(data)).unwrap();
        std::fs::write(snapshot_path(data, &sha).unwrap(), "x").unwrap();
        sha
    }

    #[test]
    fn pruning_removes_what_no_record_references() {
        let data = TempDir::new().unwrap();
        let kept = put_snapshot(data.path(), 1);
        let dropped = put_snapshot(data.path(), 2);
        let history = [record_with(1, &[&kept])];

        let removed = prune_snapshots(data.path(), &mappings_to_keep(&history, &[], &[]));

        assert_eq!(removed, 1);
        assert_eq!(saved_files(data.path()), vec![format!("{kept}.txt")]);
        assert!(!snapshot_path(data.path(), &dropped).unwrap().exists());
    }

    #[test]
    fn pruning_keeps_fresh_copies_and_removes_abandoned_ones() {
        let data = TempDir::new().unwrap();
        std::fs::create_dir_all(mappings_dir(data.path())).unwrap();
        let fresh = unique_tmp_path(&mappings_dir(data.path()).join(TMP_PREFIX));
        let abandoned = unique_tmp_path(&mappings_dir(data.path()).join(TMP_PREFIX));
        std::fs::write(&fresh, "x").unwrap();
        std::fs::write(&abandoned, "x").unwrap();
        set_mtime(&abandoned, SystemTime::now() - STALE_TMP_AGE * 2);
        let other = mappings_dir(data.path()).join("notes.md");
        std::fs::write(&other, "x").unwrap();

        prune_snapshots(data.path(), &KeptMappings::default());

        assert!(fresh.exists());
        assert!(!abandoned.exists());
        assert!(other.exists());
    }

    #[test]
    fn pruning_past_the_backstop_removes_the_least_recently_saved() {
        let data = TempDir::new().unwrap();
        let shas: Vec<String> = (0..MAX_MAPPING_SNAPSHOTS + 2)
            .map(|n| put_snapshot(data.path(), n))
            .collect();
        for (n, sha) in shas.iter().enumerate() {
            let when = an_hour_ago() + Duration::from_secs(n as u64);
            set_mtime(&snapshot_path(data.path(), sha).unwrap(), when);
        }
        let refs: Vec<&str> = shas.iter().map(String::as_str).collect();
        let history = [record_with(1, &refs)];

        let removed = prune_snapshots(data.path(), &mappings_to_keep(&history, &[], &[]));

        assert_eq!(removed, 2);
        assert_eq!(saved_files(data.path()).len(), MAX_MAPPING_SNAPSHOTS);
        assert!(!snapshot_path(data.path(), &shas[0]).unwrap().exists());
        assert!(!snapshot_path(data.path(), &shas[1]).unwrap().exists());
        assert!(snapshot_path(data.path(), &shas[2]).unwrap().exists());
    }

    fn installed_with(sha: &str) -> InstalledBuild {
        InstalledBuild {
            serial: "emulator-5554".into(),
            avd_name: Some("Pixel_7".into()),
            model: None,
            package: "com.example".into(),
            apk_sha256: "ab".repeat(32),
            build_id: Some(1),
            version_code: None,
            mappings: record_with(1, &[sha]).mappings,
            installed_at: String::new(),
        }
    }

    #[test]
    fn an_installed_build_keeps_its_mapping_after_the_history_drops_it() {
        let data = TempDir::new().unwrap();
        let pinned = put_snapshot(data.path(), 1);
        let dropped = put_snapshot(data.path(), 2);

        let removed = prune_snapshots(
            data.path(),
            &mappings_to_keep(&[], &[installed_with(&pinned)], &[]),
        );

        assert_eq!(removed, 1);
        assert!(snapshot_path(data.path(), &pinned).unwrap().exists());
        assert!(!snapshot_path(data.path(), &dropped).unwrap().exists());
    }

    #[test]
    fn the_backstop_never_removes_a_pinned_mapping() {
        let data = TempDir::new().unwrap();
        let shas: Vec<String> = (0..MAX_MAPPING_SNAPSHOTS + 2)
            .map(|n| put_snapshot(data.path(), n))
            .collect();
        for (n, sha) in shas.iter().enumerate() {
            let when = an_hour_ago() + Duration::from_secs(n as u64);
            set_mtime(&snapshot_path(data.path(), sha).unwrap(), when);
        }
        let refs: Vec<&str> = shas.iter().map(String::as_str).collect();
        let history = [record_with(1, &refs)];
        // The least recently saved, which the backstop would remove first.
        let installed = [installed_with(&shas[0])];

        let removed = prune_snapshots(data.path(), &mappings_to_keep(&history, &installed, &[]));

        assert_eq!(removed, 1);
        assert!(snapshot_path(data.path(), &shas[0]).unwrap().exists());
        assert!(!snapshot_path(data.path(), &shas[1]).unwrap().exists());
        assert!(snapshot_path(data.path(), &shas[2]).unwrap().exists());
        assert_eq!(saved_files(data.path()).len(), MAX_MAPPING_SNAPSHOTS + 1);
    }

    #[test]
    fn snapshot_paths_are_only_for_hashes() {
        let data = Path::new("/data");
        assert!(snapshot_path(data, &"a".repeat(64)).is_some());
        assert!(snapshot_path(data, "../settings").is_none());
        assert!(snapshot_path(data, &"A".repeat(64)).is_none());
        assert!(snapshot_path(data, &"a".repeat(63)).is_none());
    }
}
