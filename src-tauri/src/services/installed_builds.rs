//! Which build produced the APK Keynobi installed on each device.
//!
//! A successful build's APKs are hashed when it finishes and listed on its
//! history record (`BuildRecord.apks`). When Keynobi installs an APK (Run App,
//! MCP `install_apk`), that APK is hashed too and matched to the newest record
//! listing the hash. The install is saved per device and package in
//! `installed-builds.json` in the data directory, with the build's R8 mappings
//! for the APK's module and variant, so it outlives the build's history
//! record. Mapping retention keeps the mappings saved installs name.

use crate::models::build::{BuildRecord, BuiltApk, InstalledBuild, MappingSnapshot, RunApk};
use crate::models::error::AppError;
use crate::services::adb_manager::{self, DeviceState};
use crate::services::build_runner::{self, OutputMetadata};
use crate::services::gradle_modules;
use crate::services::mapping_snapshots::{self, MappingSource};
use crate::services::settings_manager::{data_dir, unique_tmp_path, with_data_lock_in};
use crate::utils::path::{
    resolve_project_file, validate_apk_within_build_outputs, validate_within_root,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Most (device, package) installs kept; the oldest is dropped first. Each
/// can keep one R8 mapping past the build history's retention.
pub const MAX_INSTALLED_TARGETS: usize = 16;

/// Most APKs recorded for one build (application modules × variants).
pub const MAX_APKS_PER_BUILD: usize = 8;

/// Most `output-metadata.json` directories read in one module's APK outputs.
const MAX_METADATA_DIRS: usize = 64;

/// Directory levels searched below `build/outputs/apk` (flavor dimensions and
/// the build type).
const APK_SEARCH_DEPTH: usize = 6;

/// Largest `output-metadata.json` read; AGP writes a few hundred bytes.
const MAX_OUTPUT_METADATA_BYTES: u64 = 1024 * 1024;

const HASH_CHUNK_BYTES: usize = 1024 * 1024;

const INSTALLED_BUILDS_FILE: &str = "installed-builds.json";

// ── APKs a build wrote ────────────────────────────────────────────────────────

/// An APK listed in an application module's `output-metadata.json`.
struct ApkCandidate {
    module: String,
    variant: String,
    application_id: Option<String>,
    version_code: Option<u32>,
    /// Canonical, inside the module's `build/outputs`.
    path: PathBuf,
}

/// Hash every APK the build at `source` wrote: those listed in an application
/// module's `build/outputs/apk/**/output-metadata.json` and modified at or
/// after the build started. Paths are resolved canonically inside the
/// module's `build/outputs`. An APK that cannot be hashed is logged and left
/// out; this never fails the build. Runs without the data lock.
pub fn hash_build_apks(source: &MappingSource) -> Vec<BuiltApk> {
    let Ok(root) = source.gradle_root.canonicalize() else {
        return Vec::new();
    };
    let mut apks = Vec::new();
    for candidate in find_apks(&root) {
        if apks.len() >= MAX_APKS_PER_BUILD {
            tracing::warn!(
                "APK {} was not hashed: more than {MAX_APKS_PER_BUILD} APKs in one build",
                candidate.path.display()
            );
            continue;
        }
        match hash_file(&candidate.path, Some(source.build_started)) {
            Ok(Some((sha256, bytes))) => apks.push(BuiltApk {
                module: candidate.module,
                variant: candidate.variant,
                application_id: candidate.application_id,
                version_code: candidate.version_code,
                sha256,
                bytes,
                path: candidate
                    .path
                    .strip_prefix(&root)
                    .unwrap_or(&candidate.path)
                    .to_string_lossy()
                    .into_owned(),
            }),
            // Written by an earlier build.
            Ok(None) => {}
            Err(reason) => {
                tracing::warn!("APK {} was not hashed: {reason}", candidate.path.display())
            }
        }
    }
    apks
}

/// The APKs listed in the application modules' output metadata, each resolved
/// to a canonical `.apk` inside its module's `build/outputs`.
fn find_apks(root: &Path) -> Vec<ApkCandidate> {
    let mut found: Vec<ApkCandidate> = Vec::new();
    for module in gradle_modules::application_modules(root) {
        let outputs_rel = match module.relative_dir(root).as_str() {
            "." => "build/outputs".to_string(),
            dir => format!("{dir}/build/outputs"),
        };
        let outputs = match validate_within_root(root, &outputs_rel) {
            Ok(dir) => dir,
            Err(AppError::NotFound(_)) => continue,
            Err(e) => {
                tracing::warn!("APKs of {} were not hashed: {e}", module.path);
                continue;
            }
        };
        for dir in metadata_dirs(&outputs) {
            let Some(meta) = read_metadata(&outputs, &format!("{dir}/output-metadata.json")) else {
                continue;
            };
            let application_id = valid_package(meta.application_id);
            for element in meta.elements {
                let path =
                    match resolve_project_file(&outputs, &format!("{dir}/{}", element.output_file))
                    {
                        Ok(path) if is_apk(&path) => path,
                        Ok(_) | Err(AppError::NotFound(_)) => continue,
                        Err(e) => {
                            tracing::warn!("APK {dir}/{} was not hashed: {e}", element.output_file);
                            continue;
                        }
                    };
                if found.iter().any(|c| c.path == path) {
                    continue;
                }
                found.push(ApkCandidate {
                    module: module.path.clone(),
                    variant: meta.variant_name.clone(),
                    application_id: application_id.clone(),
                    version_code: element.version_code.and_then(|v| u32::try_from(v).ok()),
                    path,
                });
            }
        }
    }
    found
}

/// Directories under `outputs/apk`, relative to `outputs`, that hold an
/// `output-metadata.json`. Symlinked directories are not entered, so the walk
/// stays inside `outputs` (which is canonical).
fn metadata_dirs(outputs: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![("apk".to_string(), 0)];
    while let Some((rel, depth)) = pending.pop() {
        let dir = outputs.join(&rel);
        if !std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        if std::fs::symlink_metadata(dir.join("output-metadata.json")).is_ok() {
            found.push(rel.clone());
            if found.len() >= MAX_METADATA_DIRS {
                break;
            }
        }
        if depth >= APK_SEARCH_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // `file_type` does not follow symlinks.
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if let Some(name) = entry.file_name().to_str() {
                    pending.push((format!("{rel}/{name}"), depth + 1));
                }
            }
        }
    }
    found.sort();
    found
}

/// `relative` under `dir` as AGP output metadata, when it resolves to a
/// regular file inside `dir` of at most `MAX_OUTPUT_METADATA_BYTES`.
fn read_metadata(dir: &Path, relative: &str) -> Option<OutputMetadata> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = resolve_project_file(dir, relative).ok()?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .ok()?;
    let mut text = String::new();
    file.take(MAX_OUTPUT_METADATA_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_OUTPUT_METADATA_BYTES {
        return None;
    }
    serde_json::from_str(&text).ok()
}

fn is_apk(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("apk")
}

/// A package name read from a project file, when it is a valid one.
fn valid_package(id: Option<String>) -> Option<String> {
    id.filter(|id| crate::utils::validation::validate_package_name(id).is_ok())
}

/// SHA-256 (lowercase hex) and size of the regular file at `path`, streamed
/// in bounded chunks. `path` is canonical, so it is opened with `O_NOFOLLOW`
/// to refuse a symlink swapped in since. With `since`, `Ok(None)` when the
/// file was modified before it.
fn hash_file(path: &Path, since: Option<SystemTime>) -> Result<Option<(String, u64)>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| format!("cannot open it: {e}"))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("cannot read its metadata: {e}"))?;
    if !meta.is_file() {
        return Err("not a regular file".into());
    }
    if let Some(since) = since {
        let modified = meta
            .modified()
            .map_err(|e| format!("cannot read its modification time: {e}"))?;
        if modified < since {
            return Ok(None);
        }
    }
    let mut hasher = Sha256::new();
    let mut bytes: u64 = 0;
    let mut buf = vec![0u8; HASH_CHUNK_BYTES];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("cannot read it: {e}")),
        };
        hasher.update(&buf[..n]);
        bytes += n as u64;
    }
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(Some((sha256, bytes)))
}

// ── The APK Run App installs ──────────────────────────────────────────────────

/// The APK to install after build `build_id` of `variant` in `module` (an
/// application module; without it, the project's only one).
///
/// The APK that build's record lists for the module and variant. When it
/// lists none, because Gradle found the APK up to date and did not rewrite
/// it, the module's build outputs are searched for the variant
/// ([`build_runner::find_output_apk`]) and the APK is matched by hash to the
/// newest record in `history` (oldest first) that wrote it. Another variant's
/// or module's APK is never returned.
pub fn run_apk(
    gradle_root: &Path,
    history: &[BuildRecord],
    build_id: Option<u32>,
    module: Option<&str>,
    variant: &str,
) -> Result<RunApk, String> {
    let module = gradle_modules::resolve_application_module(gradle_root, module)?;
    let is_wanted =
        |a: &BuiltApk| a.module == module.path && a.variant.eq_ignore_ascii_case(variant);

    if let Some(record) = build_id.and_then(|id| history.iter().find(|r| r.id == id)) {
        let built: Vec<&BuiltApk> = record.apks.iter().filter(|a| is_wanted(a)).collect();
        let signed: Vec<&BuiltApk> = built
            .iter()
            .copied()
            .filter(|a| {
                !a.path
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .contains("-unsigned")
            })
            .collect();
        match (if signed.is_empty() { built } else { signed }).as_slice() {
            [] => {}
            [only] => {
                let path = resolve_project_file(gradle_root, &only.path)
                    .and_then(|p| validate_apk_within_build_outputs(gradle_root, p))
                    .map_err(|e| {
                        format!(
                            "The APK build #{} wrote ({}) cannot be installed: {e}",
                            record.id, only.path
                        )
                    })?;
                return Ok(RunApk {
                    path: path.to_string_lossy().into_owned(),
                    build_id: Some(record.id),
                    from_this_build: true,
                });
            }
            many => {
                return Err(format!(
                    "Build #{} wrote more than one APK for {} variant '{variant}': {}. \
                     Split or multi-output APKs are not supported yet; install one with \
                     install_apk.",
                    record.id,
                    module.path,
                    many.iter()
                        .map(|a| a.path.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
    }

    let path = build_runner::find_output_apk(gradle_root, Some(&module.path), variant)?;
    let written_by = match hash_file(&path, None) {
        Ok(Some((sha256, _))) => history
            .iter()
            .rev()
            .find(|r| r.apks.iter().any(|a| a.sha256 == sha256 && is_wanted(a)))
            .map(|r| r.id),
        Ok(None) => None,
        Err(reason) => {
            tracing::warn!("APK {} was not hashed: {reason}", path.display());
            None
        }
    };
    Ok(RunApk {
        path: path.to_string_lossy().into_owned(),
        build_id: written_by,
        from_this_build: false,
    })
}

// ── Installs ──────────────────────────────────────────────────────────────────

/// The device an APK is installed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTarget {
    pub serial: String,
    /// An emulator's AVD name. Emulator serials are reused by other AVDs, so
    /// an emulator is identified by this; without it, by its serial.
    pub avd_name: Option<String>,
    /// For display only.
    pub model: Option<String>,
}

impl InstallTarget {
    /// Whether `entry` was installed on this device: the same AVD, or, for a
    /// device without an AVD name, the same serial.
    fn is_device_of(&self, entry: &InstalledBuild) -> bool {
        match (&self.avd_name, &entry.avd_name) {
            (Some(mine), Some(theirs)) => mine == theirs,
            (None, None) => self.serial == entry.serial,
            _ => false,
        }
    }
}

/// What is known about an APK being installed, read before the data lock.
#[derive(Debug, Clone)]
pub struct InstalledApk {
    pub sha256: String,
    /// From the output metadata next to the APK (or aapt2); used when no
    /// recorded build wrote the APK.
    pub application_id: Option<String>,
    pub version_code: Option<u32>,
}

/// What an install did.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    /// `adb install`'s output.
    pub output: String,
    /// The saved install, or `None` when it could not be recorded (logged).
    pub recorded: Option<InstalledBuild>,
}

/// Install `apk` (a canonical path already validated inside the project's
/// build outputs) on `serial` and, when the install succeeds, record which
/// build produced it. The Tauri command and the MCP tool both call this.
/// A failed install records nothing; a failure to record is logged and does
/// not fail the install.
pub async fn install_and_record(
    adb: &Path,
    aapt2: Option<&Path>,
    serial: &str,
    apk: &Path,
    device_state: &DeviceState,
) -> Result<InstallOutcome, String> {
    install_and_record_in(&data_dir(), adb, aapt2, serial, apk, device_state).await
}

async fn install_and_record_in(
    dir: &Path,
    adb: &Path,
    aapt2: Option<&Path>,
    serial: &str,
    apk: &Path,
    device_state: &DeviceState,
) -> Result<InstallOutcome, String> {
    // Hashing reads the same bytes adb is sending, at the same time.
    let to_hash = apk.to_path_buf();
    let hashing = tokio::task::spawn_blocking(move || hash_file(&to_hash, None));
    let output = adb_manager::install_apk(adb, serial, &apk.to_string_lossy()).await?;

    let sha256 = match hashing.await {
        Ok(Ok(Some((sha256, _)))) => sha256,
        Ok(Ok(None)) => return Ok(unrecorded(output)),
        Ok(Err(reason)) => {
            tracing::warn!(
                "Install of {} on {serial} not recorded: the APK could not be hashed: {reason}",
                apk.display()
            );
            return Ok(unrecorded(output));
        }
        Err(e) => {
            tracing::warn!("Install on {serial} not recorded: {e}");
            return Ok(unrecorded(output));
        }
    };
    let (mut application_id, version_code) = metadata_beside(apk);
    if application_id.is_none() {
        if let Some(aapt2) = aapt2 {
            application_id =
                valid_package(adb_manager::get_package_name_from_apk(aapt2, apk).await);
        }
    }
    let target = resolve_target(adb, serial, device_state).await;
    let installed = InstalledApk {
        sha256,
        application_id,
        version_code,
    };

    let dir = dir.to_path_buf();
    let installed_at = chrono::Utc::now().to_rfc3339();
    let recorded = tokio::task::spawn_blocking(move || {
        record_install_in(&dir, &target, &installed, installed_at)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    match recorded {
        Ok(entry) => Ok(InstallOutcome {
            output,
            recorded: Some(entry),
        }),
        Err(e) => {
            tracing::warn!("Install of {} on {serial} not recorded: {e}", apk.display());
            Ok(unrecorded(output))
        }
    }
}

/// One line for an agent: which build the install was matched to.
pub fn describe_install(entry: &InstalledBuild) -> String {
    let device = entry.avd_name.as_deref().unwrap_or(&entry.serial);
    let build = match entry.build_id {
        Some(id) => format!("build #{id}"),
        None => "an APK no recorded build wrote".to_string(),
    };
    let mappings = match entry.mappings.as_slice() {
        [] => String::new(),
        mappings => format!(
            "; R8 mapping kept ({})",
            mappings
                .iter()
                .map(|m| m.pg_map_id.as_deref().unwrap_or(&m.variant))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!(
        "Recorded {} on {device} as {build}{mappings}.",
        entry.package
    )
}

fn unrecorded(output: String) -> InstallOutcome {
    InstallOutcome {
        output,
        recorded: None,
    }
}

/// The application ID and version code the `output-metadata.json` next to
/// `apk` (canonical) records for it.
fn metadata_beside(apk: &Path) -> (Option<String>, Option<u32>) {
    let Some(dir) = apk.parent() else {
        return (None, None);
    };
    let Some(meta) = read_metadata(dir, "output-metadata.json") else {
        return (None, None);
    };
    let version_code = meta
        .elements
        .iter()
        .find(|e| dir.join(&e.output_file).canonicalize().ok().as_deref() == Some(apk))
        .and_then(|e| e.version_code)
        .and_then(|v| u32::try_from(v).ok());
    (valid_package(meta.application_id), version_code)
}

/// The device `serial` names. An emulator is asked for its AVD name, since a
/// cached name could belong to an emulator that has since left the serial.
pub async fn resolve_target(adb: &Path, serial: &str, device_state: &DeviceState) -> InstallTarget {
    let model = device_state
        .0
        .lock()
        .await
        .devices
        .iter()
        .find(|d| d.serial == serial)
        .and_then(|d| d.model.clone());
    // `adb devices` lists every emulator as `emulator-<port>`.
    let avd_name = if serial.starts_with("emulator-") {
        adb_manager::resolve_avd_name(adb, serial).await
    } else {
        None
    };
    InstallTarget {
        serial: serial.to_string(),
        avd_name,
        model,
    }
}

/// Record that `apk` was installed on `target`, under the data lock: match it
/// to the newest build record listing its hash, copy that build's mappings for
/// the APK's module and variant, and replace any earlier install of the same
/// package on the same device. An APK no recorded build wrote is recorded
/// without a build. Fails when the package is unknown.
pub fn record_install_in(
    dir: &Path,
    target: &InstallTarget,
    apk: &InstalledApk,
    installed_at: String,
) -> Result<InstalledBuild, String> {
    with_data_lock_in(dir, || {
        let history = build_runner::load_build_history_from(dir);
        let built = history.iter().rev().find_map(|record| {
            record
                .apks
                .iter()
                .find(|a| a.sha256 == apk.sha256)
                .map(|a| (record, a))
        });
        let package = built
            .and_then(|(_, a)| a.application_id.clone())
            .or_else(|| apk.application_id.clone())
            .ok_or_else(|| "the APK's package name is unknown".to_string())?;
        let entry = InstalledBuild {
            serial: target.serial.clone(),
            avd_name: target.avd_name.clone(),
            model: target.model.clone(),
            package,
            apk_sha256: apk.sha256.clone(),
            build_id: built.map(|(record, _)| record.id),
            version_code: built.and_then(|(_, a)| a.version_code).or(apk.version_code),
            mappings: built
                .map(|(record, a)| mappings_of(dir, &record.mappings, a))
                .unwrap_or_default(),
            installed_at,
        };

        let mut installed = load_installed_builds_from(dir);
        installed.retain(|e| !(e.package == entry.package && target.is_device_of(e)));
        installed.push(entry.clone());
        if installed.len() > MAX_INSTALLED_TARGETS {
            installed.drain(..installed.len() - MAX_INSTALLED_TARGETS);
        }
        save_installed_builds_to(dir, &installed)?;
        Ok(entry)
    })?
}

/// The build's saved mappings of `apk`'s module and variant, when the saved
/// copy still exists. Recording calls it under the data lock, so pruning
/// cannot remove one before the install that names it is saved.
pub(crate) fn mappings_of(
    dir: &Path,
    mappings: &[MappingSnapshot],
    apk: &BuiltApk,
) -> Vec<MappingSnapshot> {
    mappings
        .iter()
        .filter(|m| m.module == apk.module && m.variant.eq_ignore_ascii_case(&apk.variant))
        .filter(|m| {
            mapping_snapshots::snapshot_path(dir, &m.sha256).is_some_and(|path| path.is_file())
        })
        .cloned()
        .collect()
}

/// The saved installs, oldest first. A missing or unreadable file is empty.
pub fn load_installed_builds_from(dir: &Path) -> Vec<InstalledBuild> {
    let path = dir.join(INSTALLED_BUILDS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!("Cannot read {}: {e}", path.display());
            return Vec::new();
        }
    };
    match serde_json::from_str::<Vec<InstalledBuild>>(&text) {
        Ok(mut installed) => {
            if installed.len() > MAX_INSTALLED_TARGETS {
                installed.drain(..installed.len() - MAX_INSTALLED_TARGETS);
            }
            installed
        }
        Err(e) => {
            tracing::warn!("Ignoring {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Callers hold the data lock.
fn save_installed_builds_to(dir: &Path, installed: &[InstalledBuild]) -> Result<(), String> {
    let path = dir.join(INSTALLED_BUILDS_FILE);
    let json = serde_json::to_string_pretty(installed)
        .map_err(|e| format!("Failed to serialize installed builds: {e}"))?;
    let tmp = unique_tmp_path(&path);
    std::fs::write(&tmp, json).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to write installed builds: {e}")
    })?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to save installed builds: {e}")
    })
}

/// Every saved install, oldest first.
pub fn list_installed_builds() -> Vec<InstalledBuild> {
    load_installed_builds_from(&data_dir())
}

/// What Keynobi last installed of `package` on `target`, if it recorded it.
pub fn installed_build(target: &InstallTarget, package: &str) -> Option<InstalledBuild> {
    installed_build_in(&data_dir(), target, package)
}

/// [`installed_build`] in the data directory `dir`.
pub fn installed_build_in(
    dir: &Path,
    target: &InstallTarget,
    package: &str,
) -> Option<InstalledBuild> {
    load_installed_builds_from(dir)
        .into_iter()
        .rev()
        .find(|e| e.package == package && target.is_device_of(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::build::BuildStatus;
    use std::time::Duration;
    use tempfile::TempDir;

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

    /// `app/build/outputs/apk/<dir>/<file>` with AGP's metadata next to it.
    fn write_apk(root: &Path, dir: &str, file: &str, variant: &str, bytes: &[u8]) -> PathBuf {
        let folder = root.join("app/build/outputs/apk").join(dir);
        std::fs::create_dir_all(&folder).unwrap();
        let apk = folder.join(file);
        std::fs::write(&apk, bytes).unwrap();
        write_metadata(&folder, variant, file);
        apk
    }

    fn write_metadata(folder: &Path, variant: &str, output_file: &str) {
        std::fs::write(
            folder.join("output-metadata.json"),
            serde_json::json!({
                "version": 3,
                "artifactType": { "type": "APK", "kind": "Directory" },
                "applicationId": format!("com.example.{variant}"),
                "variantName": variant,
                "elements": [{
                    "type": "SINGLE", "filters": [], "attributes": [],
                    "versionCode": 7, "versionName": "1.0", "outputFile": output_file
                }],
                "elementType": "File"
            })
            .to_string(),
        )
        .unwrap();
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

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    // ── Hashing a build's APKs ───────────────────────────────────────────────

    #[test]
    fn an_apk_the_build_wrote_is_hashed_with_its_metadata() {
        let project = project();
        write_apk(
            project.path(),
            "debug",
            "app-debug.apk",
            "debug",
            b"apk one",
        );

        let apks = hash_build_apks(&source(project.path(), an_hour_ago()));

        assert_eq!(
            apks,
            vec![BuiltApk {
                module: ":app".into(),
                variant: "debug".into(),
                application_id: Some("com.example.debug".into()),
                version_code: Some(7),
                sha256: sha256_hex(b"apk one"),
                bytes: 7,
                path: "app/build/outputs/apk/debug/app-debug.apk".into(),
            }]
        );
    }

    #[test]
    fn an_apk_older_than_the_build_is_not_hashed() {
        let project = project();
        let stale = write_apk(
            project.path(),
            "release",
            "app-release.apk",
            "release",
            b"old",
        );
        set_mtime(&stale, an_hour_ago());
        write_apk(project.path(), "debug", "app-debug.apk", "debug", b"new");

        let started = SystemTime::now() - Duration::from_secs(60);
        let apks = hash_build_apks(&source(project.path(), started));

        let variants: Vec<&str> = apks.iter().map(|a| a.variant.as_str()).collect();
        assert_eq!(variants, vec!["debug"]);
    }

    #[test]
    fn an_apk_linked_outside_the_build_outputs_is_refused() {
        let project = project();
        let outside = TempDir::new().unwrap();
        let secret = outside.path().join("secret.apk");
        std::fs::write(&secret, b"not this project's").unwrap();
        let folder = project.path().join("app/build/outputs/apk/debug");
        std::fs::create_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(&secret, folder.join("app-debug.apk")).unwrap();
        write_metadata(&folder, "debug", "app-debug.apk");
        // Metadata that names a file above the build outputs.
        let release = project.path().join("app/build/outputs/apk/release");
        std::fs::create_dir_all(&release).unwrap();
        write_metadata(&release, "release", "../../../../../secret.apk");
        std::fs::write(project.path().join("secret.apk"), b"x").unwrap();

        let apks = hash_build_apks(&source(project.path(), an_hour_ago()));

        assert!(apks.is_empty(), "{apks:?}");
    }

    #[test]
    fn an_apk_folder_linked_outside_the_build_outputs_is_not_searched() {
        let project = project();
        let outside = TempDir::new().unwrap();
        write_apk(
            outside.path(),
            "debug",
            "app-debug.apk",
            "debug",
            b"elsewhere",
        );
        let apk_dir = project.path().join("app/build/outputs/apk");
        std::fs::create_dir_all(&apk_dir).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("app/build/outputs/apk/debug"),
            apk_dir.join("debug"),
        )
        .unwrap();

        let apks = hash_build_apks(&source(project.path(), an_hour_ago()));

        assert!(apks.is_empty(), "{apks:?}");
    }

    #[test]
    fn apks_are_capped_per_build() {
        let project = project();
        for i in 0..MAX_APKS_PER_BUILD + 2 {
            let dir = format!("v{i:02}");
            write_apk(project.path(), &dir, "app.apk", &dir, dir.as_bytes());
        }

        let apks = hash_build_apks(&source(project.path(), an_hour_ago()));

        assert_eq!(apks.len(), MAX_APKS_PER_BUILD);
    }

    // ── The APK Run App installs ─────────────────────────────────────────────

    /// What `hash_build_apks` records for an APK written under `app/`.
    fn built_at(root: &Path, apk: &Path, variant: &str) -> BuiltApk {
        BuiltApk {
            module: ":app".into(),
            variant: variant.into(),
            application_id: None,
            version_code: None,
            sha256: sha256_hex(&std::fs::read(apk).unwrap()),
            bytes: 1,
            path: apk
                .strip_prefix(root.canonicalize().unwrap())
                .or_else(|_| apk.strip_prefix(root))
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        }
    }

    #[test]
    fn run_apk_is_the_one_this_run_recorded() {
        let project = project();
        let apk = write_apk(project.path(), "debug", "app-debug.apk", "debug", b"new");
        let history = vec![record(
            5,
            vec![built_at(project.path(), &apk, "debug")],
            vec![],
        )];

        let found = run_apk(project.path(), &history, Some(5), Some(":app"), "debug").unwrap();

        assert_eq!(
            found,
            RunApk {
                path: apk.canonicalize().unwrap().to_string_lossy().into_owned(),
                build_id: Some(5),
                from_this_build: true,
            }
        );
    }

    #[test]
    fn run_apk_unchanged_by_this_run_names_the_build_that_wrote_it() {
        let project = project();
        let apk = write_apk(project.path(), "debug", "app-debug.apk", "debug", b"same");
        // Build #4 wrote the APK; build #6 found it up to date and wrote none.
        let history = vec![
            record(4, vec![built_at(project.path(), &apk, "debug")], vec![]),
            record(6, vec![], vec![]),
        ];

        let found = run_apk(project.path(), &history, Some(6), None, "debug").unwrap();

        assert_eq!(found.build_id, Some(4));
        assert!(!found.from_this_build);
        assert!(found
            .path
            .ends_with("app/build/outputs/apk/debug/app-debug.apk"));
    }

    #[test]
    fn run_apk_no_kept_build_wrote_is_found_without_a_build() {
        let project = project();
        write_apk(
            project.path(),
            "debug",
            "app-debug.apk",
            "debug",
            b"by hand",
        );
        let history = vec![record(6, vec![], vec![])];

        let found = run_apk(project.path(), &history, Some(6), None, "debug").unwrap();

        assert_eq!(found.build_id, None);
        assert!(!found.from_this_build);
    }

    #[test]
    fn run_apk_never_takes_another_variants_apk() {
        let project = project();
        let release = write_apk(
            project.path(),
            "release",
            "app-release.apk",
            "release",
            b"release",
        );
        // This run's record lists only the release APK, and the outputs have
        // no debug APK.
        let history = vec![record(
            7,
            vec![built_at(project.path(), &release, "release")],
            vec![],
        )];

        let err = run_apk(project.path(), &history, Some(7), None, "debug").unwrap_err();
        assert!(err.contains("No APK for variant 'debug'"), "{err}");

        // With a debug APK in the outputs, that one is used, not the release.
        let debug = write_apk(project.path(), "debug", "app-debug.apk", "debug", b"debug");
        let found = run_apk(project.path(), &history, Some(7), None, "debug").unwrap();
        assert_eq!(
            found.path,
            debug.canonicalize().unwrap().to_string_lossy().into_owned()
        );
        assert!(!found.from_this_build);
    }

    #[test]
    fn run_apk_never_takes_another_modules_apk() {
        let project = project();
        std::fs::write(
            project.path().join("settings.gradle.kts"),
            "include(\":app\", \":wear\")\n",
        )
        .unwrap();
        std::fs::create_dir_all(project.path().join("wear")).unwrap();
        std::fs::write(
            project.path().join("wear/build.gradle.kts"),
            "plugins { id(\"com.android.application\") }\n",
        )
        .unwrap();
        let app = write_apk(project.path(), "debug", "app-debug.apk", "debug", b"app");
        let mut wear = built_at(project.path(), &app, "debug");
        wear.module = ":wear".into();
        let history = vec![record(8, vec![wear], vec![])];

        let found = run_apk(project.path(), &history, Some(8), Some(":app"), "debug").unwrap();
        assert!(!found.from_this_build, "took the :wear module's APK");

        let err = run_apk(project.path(), &history, Some(8), None, "debug").unwrap_err();
        assert!(err.contains(":app, :wear"), "{err}");
    }

    #[test]
    fn run_apk_prefers_the_signed_apk_this_run_recorded() {
        let project = project();
        let signed = write_apk(
            project.path(),
            "release",
            "app-release.apk",
            "release",
            b"signed",
        );
        let unsigned = write_apk(
            project.path(),
            "release-unsigned",
            "app-release-unsigned.apk",
            "release",
            b"unsigned",
        );
        let history = vec![record(
            9,
            vec![
                built_at(project.path(), &unsigned, "release"),
                built_at(project.path(), &signed, "release"),
            ],
            vec![],
        )];

        let found = run_apk(project.path(), &history, Some(9), None, "release").unwrap();

        assert_eq!(
            found.path,
            signed
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        );
        assert!(found.from_this_build);
    }

    // ── Recording installs ───────────────────────────────────────────────────

    fn built(sha256: &str, variant: &str) -> BuiltApk {
        BuiltApk {
            module: ":app".into(),
            variant: variant.into(),
            application_id: Some("com.example".into()),
            version_code: Some(12),
            sha256: sha256.into(),
            bytes: 1,
            path: "app/build/outputs/apk/release/app-release.apk".into(),
        }
    }

    fn mapping(n: u8, variant: &str) -> MappingSnapshot {
        MappingSnapshot {
            module: ":app".into(),
            variant: variant.into(),
            sha256: format!("{n:064x}"),
            bytes: 1,
            pg_map_id: Some(format!("map{n}")),
        }
    }

    fn record(id: u32, apks: Vec<BuiltApk>, mappings: Vec<MappingSnapshot>) -> BuildRecord {
        BuildRecord {
            id,
            task: "assembleRelease".into(),
            status: BuildStatus::Cancelled,
            errors: vec![],
            started_at: String::new(),
            project_root: None,
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings,
            apks,
        }
    }

    /// Write `records` (oldest first) as the persisted history and save their
    /// mapping snapshots.
    fn write_history(dir: &Path, records: &[BuildRecord]) {
        let newest_first: Vec<&BuildRecord> = records.iter().rev().collect();
        std::fs::write(
            dir.join("build-history.json"),
            serde_json::to_string(&newest_first).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(mapping_snapshots::mappings_dir(dir)).unwrap();
        for m in records.iter().flat_map(|r| &r.mappings) {
            std::fs::write(
                mapping_snapshots::snapshot_path(dir, &m.sha256).unwrap(),
                "x",
            )
            .unwrap();
        }
    }

    fn emulator(serial: &str, avd: &str) -> InstallTarget {
        InstallTarget {
            serial: serial.into(),
            avd_name: Some(avd.into()),
            model: None,
        }
    }

    fn phone(serial: &str) -> InstallTarget {
        InstallTarget {
            serial: serial.into(),
            avd_name: None,
            model: Some("Pixel 8".into()),
        }
    }

    fn apk(sha256: &str, package: Option<&str>) -> InstalledApk {
        InstalledApk {
            sha256: sha256.into(),
            application_id: package.map(str::to_string),
            version_code: None,
        }
    }

    fn record_install(
        dir: &Path,
        target: &InstallTarget,
        installed: &InstalledApk,
    ) -> InstalledBuild {
        record_install_in(dir, target, installed, "2026-09-25T10:32:00Z".into()).unwrap()
    }

    #[test]
    fn an_install_is_matched_to_the_newest_build_that_wrote_the_apk() {
        let dir = TempDir::new().unwrap();
        let sha = "a".repeat(64);
        write_history(
            dir.path(),
            &[
                record(3, vec![built(&sha, "release")], vec![]),
                record(
                    4,
                    vec![built(&sha, "release")],
                    vec![mapping(1, "release"), mapping(2, "debug")],
                ),
                record(5, vec![built(&"b".repeat(64), "release")], vec![]),
            ],
        );

        let entry = record_install(
            dir.path(),
            &emulator("emulator-5554", "Pixel_7"),
            &apk(&sha, None),
        );

        assert_eq!(entry.build_id, Some(4));
        assert_eq!(entry.package, "com.example");
        assert_eq!(entry.version_code, Some(12));
        assert_eq!(entry.mappings, vec![mapping(1, "release")]);
        assert_eq!(load_installed_builds_from(dir.path()), vec![entry]);
    }

    #[test]
    fn a_mapping_no_longer_saved_is_not_named() {
        let dir = TempDir::new().unwrap();
        let sha = "a".repeat(64);
        write_history(
            dir.path(),
            &[record(
                4,
                vec![built(&sha, "release")],
                vec![mapping(1, "release")],
            )],
        );
        let saved = mapping_snapshots::snapshot_path(dir.path(), &mapping(1, "").sha256).unwrap();
        std::fs::remove_file(saved).unwrap();

        let entry = record_install(dir.path(), &phone("R5CT"), &apk(&sha, None));

        assert_eq!(entry.build_id, Some(4));
        assert!(entry.mappings.is_empty());
    }

    #[test]
    fn an_apk_no_recorded_build_wrote_is_recorded_without_a_build() {
        let dir = TempDir::new().unwrap();
        write_history(
            dir.path(),
            &[record(4, vec![built(&"a".repeat(64), "release")], vec![])],
        );

        let entry = record_install(
            dir.path(),
            &phone("R5CT"),
            &InstalledApk {
                version_code: Some(3),
                ..apk(&"c".repeat(64), Some("com.example.studio"))
            },
        );

        assert_eq!(entry.build_id, None);
        assert_eq!(entry.package, "com.example.studio");
        assert_eq!(entry.version_code, Some(3));
        assert!(entry.mappings.is_empty());
    }

    #[test]
    fn an_install_whose_package_is_unknown_is_not_recorded() {
        let dir = TempDir::new().unwrap();

        let result = record_install_in(
            dir.path(),
            &phone("R5CT"),
            &apk(&"c".repeat(64), None),
            String::new(),
        );

        assert!(result.is_err());
        assert!(load_installed_builds_from(dir.path()).is_empty());
    }

    #[test]
    fn a_later_install_replaces_the_one_on_the_same_device_and_package() {
        let dir = TempDir::new().unwrap();
        let first = record_install(
            dir.path(),
            &phone("R5CT"),
            &apk(&"1".repeat(64), Some("com.a")),
        );
        let other_package = record_install(
            dir.path(),
            &phone("R5CT"),
            &apk(&"2".repeat(64), Some("com.b")),
        );
        let other_device = record_install(
            dir.path(),
            &phone("ZX1G"),
            &apk(&"3".repeat(64), Some("com.a")),
        );
        let second = record_install(
            dir.path(),
            &phone("R5CT"),
            &apk(&"4".repeat(64), Some("com.a")),
        );

        assert_eq!(
            load_installed_builds_from(dir.path()),
            vec![other_package, other_device, second.clone()]
        );
        assert_ne!(first, second);
        assert_eq!(
            installed_build_in(dir.path(), &phone("R5CT"), "com.a"),
            Some(second)
        );
    }

    #[test]
    fn an_emulator_is_identified_by_its_avd_across_serials() {
        let dir = TempDir::new().unwrap();
        record_install(
            dir.path(),
            &emulator("emulator-5554", "Pixel_7"),
            &apk(&"1".repeat(64), Some("com.a")),
        );
        // Pixel_8 takes Pixel_7's old serial; Pixel_7 comes back on another.
        let pixel_8 = record_install(
            dir.path(),
            &emulator("emulator-5554", "Pixel_8"),
            &apk(&"2".repeat(64), Some("com.a")),
        );
        let pixel_7 = record_install(
            dir.path(),
            &emulator("emulator-5556", "Pixel_7"),
            &apk(&"3".repeat(64), Some("com.a")),
        );

        assert_eq!(
            load_installed_builds_from(dir.path()),
            vec![pixel_8.clone(), pixel_7.clone()]
        );
        assert_eq!(
            installed_build_in(dir.path(), &emulator("emulator-5558", "Pixel_7"), "com.a"),
            Some(pixel_7)
        );
        assert_eq!(
            installed_build_in(dir.path(), &emulator("emulator-5554", "Pixel_8"), "com.a"),
            Some(pixel_8)
        );
        // An emulator that did not report its AVD is not guessed to be one.
        let unnamed = InstallTarget {
            avd_name: None,
            ..emulator("emulator-5554", "")
        };
        assert_eq!(installed_build_in(dir.path(), &unnamed, "com.a"), None);
    }

    #[test]
    fn installs_are_capped_and_the_oldest_goes_first() {
        let dir = TempDir::new().unwrap();
        let entries: Vec<InstalledBuild> = (0..MAX_INSTALLED_TARGETS + 2)
            .map(|n| {
                record_install(
                    dir.path(),
                    &phone(&format!("SERIAL{n}")),
                    &apk(&"1".repeat(64), Some("com.a")),
                )
            })
            .collect();

        let kept = load_installed_builds_from(dir.path());

        assert_eq!(kept.len(), MAX_INSTALLED_TARGETS);
        assert_eq!(kept, entries[2..]);
    }

    #[test]
    fn recording_waits_for_another_process_holding_the_data_lock() {
        let dir = TempDir::new().unwrap();
        // Another process opens the lock file itself; a second handle stands in for it.
        let other_process = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.path().join(".lock"))
            .unwrap();
        other_process.lock().unwrap();
        let theirs = InstalledBuild {
            serial: "ZX1G".into(),
            avd_name: None,
            model: None,
            package: "com.a".into(),
            apk_sha256: "9".repeat(64),
            build_id: None,
            version_code: None,
            mappings: vec![],
            installed_at: String::new(),
        };

        let path = dir.path().to_path_buf();
        let recording = std::thread::spawn(move || {
            record_install(&path, &phone("R5CT"), &apk(&"1".repeat(64), Some("com.a")))
        });
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !recording.is_finished(),
            "recorded while another process held the lock"
        );
        // The other process saves its install, then releases the lock.
        save_installed_builds_to(dir.path(), std::slice::from_ref(&theirs)).unwrap();
        other_process.unlock().unwrap();
        let mine = recording.join().unwrap();

        assert_eq!(load_installed_builds_from(dir.path()), vec![theirs, mine]);
    }

    #[test]
    fn a_missing_or_corrupt_file_is_empty() {
        let dir = TempDir::new().unwrap();
        assert!(load_installed_builds_from(dir.path()).is_empty());
        std::fs::write(dir.path().join(INSTALLED_BUILDS_FILE), "{not json").unwrap();
        assert!(load_installed_builds_from(dir.path()).is_empty());
    }

    // ── Installing ───────────────────────────────────────────────────────────

    /// An `adb` that answers `emu avd name` with `avd` and runs `install` for an install.
    fn fake_adb(dir: &Path, avd: &str, install: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let adb = dir.join("adb");
        std::fs::write(
            &adb,
            format!(
                "#!/bin/sh\ncase \"$*\" in\n  *\"emu avd name\"*) printf '{avd}\\nOK\\n' ;;\n  \
                 *install*) {install} ;;\nesac\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        crate::utils::process::test_support::run_once(&adb);
        adb
    }

    /// Run App's install: the build it just recorded wrote this APK.
    #[tokio::test]
    async fn run_app_records_the_build_whose_apk_it_installed() {
        let project = project();
        let data = TempDir::new().unwrap();
        let tools = TempDir::new().unwrap();
        let path = write_apk(
            project.path(),
            "release",
            "app-release.apk",
            "release",
            b"release",
        );
        let apks = hash_build_apks(&source(project.path(), an_hour_ago()));
        write_history(data.path(), &[record(9, apks, vec![mapping(1, "release")])]);
        let adb = fake_adb(tools.path(), "Pixel_7", "echo Success");

        let outcome = install_and_record_in(
            data.path(),
            &adb,
            None,
            "emulator-5554",
            &path.canonicalize().unwrap(),
            &DeviceState::new(),
        )
        .await
        .unwrap();

        assert!(outcome.output.contains("Success"));
        let entry = outcome.recorded.expect("recorded");
        assert_eq!(entry.build_id, Some(9));
        assert_eq!(entry.avd_name.as_deref(), Some("Pixel_7"));
        assert_eq!(entry.package, "com.example.release");
        assert_eq!(entry.version_code, Some(7));
        assert_eq!(entry.mappings, vec![mapping(1, "release")]);
        assert_eq!(load_installed_builds_from(data.path()), vec![entry]);
    }

    #[tokio::test]
    async fn an_apk_no_recorded_build_wrote_takes_its_package_from_its_metadata() {
        let project = project();
        let data = TempDir::new().unwrap();
        let tools = TempDir::new().unwrap();
        let path = write_apk(project.path(), "debug", "app-debug.apk", "debug", b"studio");
        let adb = fake_adb(tools.path(), "Pixel_7", "echo Success");

        let outcome = install_and_record_in(
            data.path(),
            &adb,
            None,
            "R5CT1234",
            &path.canonicalize().unwrap(),
            &DeviceState::new(),
        )
        .await
        .unwrap();

        let entry = outcome.recorded.expect("recorded");
        assert_eq!(entry.build_id, None);
        assert_eq!(entry.avd_name, None);
        assert_eq!(entry.package, "com.example.debug");
        assert_eq!(entry.version_code, Some(7));
    }

    #[tokio::test]
    async fn a_failed_install_records_nothing() {
        let project = project();
        let data = TempDir::new().unwrap();
        let tools = TempDir::new().unwrap();
        let path = write_apk(project.path(), "debug", "app-debug.apk", "debug", b"apk");
        let adb = fake_adb(
            tools.path(),
            "Pixel_7",
            "echo 'Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]'; exit 1",
        );

        let result = install_and_record_in(
            data.path(),
            &adb,
            None,
            "emulator-5554",
            &path.canonicalize().unwrap(),
            &DeviceState::new(),
        )
        .await;

        assert!(result.is_err());
        assert!(!data.path().join(INSTALLED_BUILDS_FILE).exists());
    }

    #[test]
    fn an_agent_is_told_what_was_recorded() {
        let entry = InstalledBuild {
            serial: "emulator-5554".into(),
            avd_name: Some("Pixel_7".into()),
            model: None,
            package: "com.example".into(),
            apk_sha256: "a".repeat(64),
            build_id: Some(12),
            version_code: Some(3),
            mappings: vec![mapping(1, "release")],
            installed_at: String::new(),
        };
        assert_eq!(
            describe_install(&entry),
            "Recorded com.example on Pixel_7 as build #12; R8 mapping kept (map1)."
        );
        let unmatched = InstalledBuild {
            avd_name: None,
            build_id: None,
            mappings: vec![],
            ..entry
        };
        assert_eq!(
            describe_install(&unmatched),
            "Recorded com.example on emulator-5554 as an APK no recorded build wrote."
        );
    }
}
