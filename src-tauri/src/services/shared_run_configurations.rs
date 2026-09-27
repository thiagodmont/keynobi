//! Run configurations shared with a project: `<gradle root>/.keynobi/
//! run-configurations.json`, written only when the user shares one.
//!
//! The file is untrusted input: anyone who can commit to the repository
//! writes it. It is read inside the project only, capped, and parsed
//! strictly, and each configuration is checked like a local one. Its schema
//! holds only portable fields, so it cannot carry a serial, a path, an
//! environment variable, a Gradle flag, or a command.

use crate::models::error::AppError;
use crate::models::run_configuration::{
    RunConfiguration, RunLaunch, SharedRunConfigurationProblem, SharedRunConfigurationsFile,
};
use crate::services::{gradle_modules, run_configurations, settings_manager};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The directory of the shared file, relative to the Gradle root.
pub const SHARED_DIR: &str = ".keynobi";

/// The shared file, relative to the Gradle root.
pub const SHARED_FILE: &str = ".keynobi/run-configurations.json";

/// Largest shared file read or written.
pub const MAX_SHARED_FILE_BYTES: u64 = 64 * 1024;

/// The file format this version reads and writes.
pub const SCHEMA_VERSION: u64 = 1;

/// The file, as read before its configurations are checked one by one.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileContents {
    #[allow(dead_code)]
    schema_version: u64,
    #[serde(default)]
    configurations: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WrittenFile<'a> {
    schema_version: u64,
    configurations: &'a [SharedConfiguration],
}

/// One configuration as the file holds it: the portable fields only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SharedConfiguration {
    name: String,
    module: String,
    variant: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task: Option<String>,
    #[serde(default, skip_serializing_if = "SharedLaunch::is_default")]
    launch: SharedLaunch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logcat_filter: Option<String>,
}

/// Unit variants are written `Default {}` so that unknown fields are refused
/// for them too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum SharedLaunch {
    Default {},
    Activity { name: String },
    DeepLink { uri: String },
    None {},
}

impl Default for SharedLaunch {
    fn default() -> Self {
        SharedLaunch::Default {}
    }
}

impl SharedLaunch {
    fn is_default(&self) -> bool {
        *self == SharedLaunch::Default {}
    }
}

impl From<SharedConfiguration> for RunConfiguration {
    fn from(c: SharedConfiguration) -> Self {
        RunConfiguration {
            name: c.name,
            module: c.module,
            variant: c.variant,
            task: c.task,
            launch: match c.launch {
                SharedLaunch::Default {} => RunLaunch::Default,
                SharedLaunch::Activity { name } => RunLaunch::Activity { name },
                SharedLaunch::DeepLink { uri } => RunLaunch::DeepLink { uri },
                SharedLaunch::None {} => RunLaunch::None,
            },
            logcat_filter: c.logcat_filter,
        }
    }
}

impl From<&RunConfiguration> for SharedConfiguration {
    fn from(c: &RunConfiguration) -> Self {
        SharedConfiguration {
            name: c.name.clone(),
            module: c.module.clone(),
            variant: c.variant.clone(),
            task: c.task.clone(),
            launch: match &c.launch {
                RunLaunch::Default => SharedLaunch::Default {},
                RunLaunch::Activity { name } => SharedLaunch::Activity { name: name.clone() },
                RunLaunch::DeepLink { uri } => SharedLaunch::DeepLink { uri: uri.clone() },
                RunLaunch::None => SharedLaunch::None {},
            },
            logcat_filter: c.logcat_filter.clone(),
        }
    }
}

// ── Reading ───────────────────────────────────────────────────────────────────

/// What the project's shared file offers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedRead {
    /// Its valid configurations, in file order.
    pub configurations: Vec<RunConfiguration>,
    /// The file as read; `None` when the project has no shared file.
    pub file: Option<SharedRunConfigurationsFile>,
}

impl SharedRead {
    /// SHA-256 of the file's bytes, when it was read.
    pub fn sha256(&self) -> Option<&str> {
        self.file.as_ref().and_then(|f| f.sha256.as_deref())
    }

    /// The valid configuration named `name`.
    pub fn get(&self, name: &str) -> Option<&RunConfiguration> {
        self.configurations.iter().find(|c| c.name == name)
    }

    /// Refuse a rewrite that would drop what this read could not use: an
    /// unreadable file, or a configuration in it that is not offered.
    pub fn check_writable(&self) -> Result<(), AppError> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let fix_first = |why: String| {
            AppError::InvalidInput(format!(
                "The project's shared run configurations ({SHARED_FILE}) cannot be changed \
                 because {why} Fix or remove the file first."
            ))
        };
        if let Some(error) = &file.error {
            return Err(fix_first(format!("the file cannot be used: {error}")));
        }
        if let Some(problem) = file.problems.first() {
            let which = problem
                .name
                .as_deref()
                .map(|n| format!("'{n}'"))
                .unwrap_or_else(|| "a configuration".into());
            return Err(fix_first(format!(
                "{which} in it is not valid: {}",
                problem.message
            )));
        }
        Ok(())
    }
}

fn file_status(sha256: Option<String>, error: Option<String>) -> SharedRunConfigurationsFile {
    SharedRunConfigurationsFile {
        path: SHARED_FILE.to_string(),
        sha256,
        error,
        problems: Vec::new(),
    }
}

fn unusable(sha256: Option<String>, error: String) -> SharedRead {
    SharedRead {
        configurations: Vec::new(),
        file: Some(file_status(sha256, Some(error))),
    }
}

/// Read and check the shared file of the project at `gradle_root`. Never
/// fails: a file that cannot be used, and each configuration in it that is
/// not valid, are reported in [`SharedRead::file`]. Needs no trust: it only
/// parses.
pub fn read(gradle_root: &Path) -> SharedRead {
    let path = match crate::utils::path::resolve_project_file(gradle_root, SHARED_FILE) {
        Ok(path) => path,
        Err(AppError::PermissionDenied(_)) => {
            return unusable(
                None,
                "It resolves outside the project (through a symbolic link), so it is not read."
                    .into(),
            )
        }
        Err(_) if gradle_root.join(SHARED_FILE).symlink_metadata().is_err() => {
            return SharedRead::default()
        }
        Err(_) => return unusable(None, "It is not a regular file.".into()),
    };
    let bytes = match read_capped(&path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return unusable(
                None,
                format!(
                    "It is larger than {} KiB, so it is not read.",
                    MAX_SHARED_FILE_BYTES / 1024
                ),
            )
        }
        Err(e) => return unusable(None, format!("It cannot be read: {e}")),
    };
    let sha256 = sha256_hex(&bytes);
    let contents = match parse(&bytes) {
        Ok(contents) => contents,
        Err(error) => return unusable(Some(sha256), error),
    };

    let modules: Vec<String> = gradle_modules::application_modules(gradle_root)
        .into_iter()
        .map(|m| m.path)
        .collect();
    let mut file = file_status(Some(sha256), None);
    let mut configurations: Vec<RunConfiguration> = Vec::new();
    for (index, value) in contents.configurations.into_iter().enumerate() {
        let name = value
            .get("name")
            .and_then(|n| n.as_str())
            .map(str::to_string);
        let mut problem = |message: String| {
            file.problems.push(SharedRunConfigurationProblem {
                name: name.clone(),
                message,
            })
        };
        if index >= run_configurations::MAX_RUN_CONFIGURATIONS {
            problem(format!(
                "The file has more than {} configurations; the rest are ignored.",
                run_configurations::MAX_RUN_CONFIGURATIONS
            ));
            continue;
        }
        let config: RunConfiguration = match serde_json::from_value::<SharedConfiguration>(value) {
            Ok(config) => config.into(),
            Err(e) => {
                problem(format!("It is not a valid run configuration: {e}."));
                continue;
            }
        };
        match run_configurations::validate_run_configuration(&config, &modules, &configurations) {
            Ok(()) => configurations.push(config),
            Err(e) => problem(e),
        }
    }
    SharedRead {
        configurations,
        file: Some(file),
    }
}

/// The file's bytes, or `None` when it is larger than `MAX_SHARED_FILE_BYTES`.
fn read_capped(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_SHARED_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= MAX_SHARED_FILE_BYTES).then_some(bytes))
}

fn parse(bytes: &[u8]) -> Result<FileContents, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("It is not valid JSON: {e}."))?;
    match value.get("schemaVersion").map(|v| v.as_u64()) {
        Some(Some(SCHEMA_VERSION)) => {}
        Some(Some(other)) => {
            return Err(format!(
                "It has schemaVersion {other}, and this Keynobi reads version \
                 {SCHEMA_VERSION}. Update Keynobi to use it."
            ))
        }
        _ => {
            return Err(format!(
                "It has no schemaVersion (expected {SCHEMA_VERSION})."
            ))
        }
    }
    serde_json::from_value(value)
        .map_err(|e| format!("It is not a valid run configuration file: {e}."))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ── Writing ───────────────────────────────────────────────────────────────────

/// Replace the project's shared file with `configurations`, atomically, or
/// remove it when there are none. `current` is the file as read before the
/// change: a file that could not be fully read is never rewritten.
pub fn write(
    gradle_root: &Path,
    current: &SharedRead,
    configurations: &[RunConfiguration],
) -> Result<(), AppError> {
    current.check_writable()?;
    if configurations.len() > run_configurations::MAX_RUN_CONFIGURATIONS {
        return Err(AppError::InvalidInput(format!(
            "A project can share at most {} run configurations.",
            run_configurations::MAX_RUN_CONFIGURATIONS
        )));
    }
    let root = gradle_root
        .canonicalize()
        .map_err(|e| AppError::io(gradle_root.display(), e))?;
    if configurations.is_empty() {
        return remove(&root);
    }
    let shared: Vec<SharedConfiguration> = configurations.iter().map(Into::into).collect();
    let mut bytes = serde_json::to_vec_pretty(&WrittenFile {
        schema_version: SCHEMA_VERSION,
        configurations: &shared,
    })
    .map_err(|e| AppError::Other(e.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_SHARED_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "The shared run configurations would be larger than {} KiB.",
            MAX_SHARED_FILE_BYTES / 1024
        )));
    }
    let dir = shared_dir(&root, true)?;
    write_atomically(&dir.join(file_name()), &bytes).map_err(|e| AppError::io(SHARED_FILE, e))
}

fn file_name() -> &'static str {
    SHARED_FILE
        .rsplit('/')
        .next()
        .unwrap_or("run-configurations.json")
}

/// The canonical `.keynobi` directory inside `root`, created when `create`.
/// One that resolves outside the project is refused.
fn shared_dir(root: &Path, create: bool) -> Result<PathBuf, AppError> {
    let dir = root.join(SHARED_DIR);
    if create {
        std::fs::create_dir_all(&dir).map_err(|e| AppError::io(SHARED_DIR, e))?;
    }
    let canonical = crate::utils::path::validate_within_root(root, SHARED_DIR)?;
    if !canonical.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "{SHARED_DIR} in the project is not a directory."
        )));
    }
    Ok(canonical)
}

/// Remove the shared file, and its directory when that is then empty.
fn remove(root: &Path) -> Result<(), AppError> {
    let dir = match shared_dir(root, false) {
        Ok(dir) => dir,
        Err(AppError::NotFound(_)) => return Ok(()),
        Err(e) => return Err(e),
    };
    match std::fs::remove_file(dir.join(file_name())) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(AppError::io(SHARED_FILE, e)),
    }
    // Only an empty directory is removed.
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

/// Write `bytes` to a new temporary file beside `path` and rename it over
/// `path`, keeping the permissions of the file it replaces.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let permissions = std::fs::metadata(path).ok().map(|m| m.permissions());
    let tmp = settings_manager::unique_tmp_path(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| match permissions {
            Some(permissions) => std::fs::set_permissions(&tmp, permissions),
            None => Ok(()),
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// ── Approval ──────────────────────────────────────────────────────────────────

/// What a shared configuration would run that needs the user's approval: a
/// task outside `assemble*`, and a deep link it opens (only when it
/// launches). Empty when nothing does.
pub fn needs_approval(config: &RunConfiguration, task: &str, launches: bool) -> Vec<String> {
    let mut reasons = Vec::new();
    let name = task.rsplit(':').next().unwrap_or(task);
    if !name.starts_with("assemble") {
        reasons.push(format!("builds {task}, which is not an assemble task"));
    }
    if let (true, RunLaunch::DeepLink { uri }) = (launches, &config.launch) {
        reasons.push(format!("opens the deep link {uri}"));
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const APP: &str = "plugins { id(\"com.android.application\") }\n";

    fn write_file(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A project whose application modules are `:app` and `:wear`.
    fn project() -> TempDir {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "settings.gradle.kts",
            "include(\":app\", \":wear\")\n",
        );
        write_file(dir.path(), "app/build.gradle.kts", APP);
        write_file(dir.path(), "wear/build.gradle.kts", APP);
        dir
    }

    fn shared(root: &Path, json: &str) {
        write_file(root, SHARED_FILE, json);
    }

    fn config(name: &str, module: &str) -> RunConfiguration {
        RunConfiguration {
            name: name.into(),
            module: module.into(),
            variant: "debug".into(),
            task: None,
            launch: RunLaunch::Default,
            logcat_filter: None,
        }
    }

    fn error_of(read: &SharedRead) -> String {
        read.file
            .as_ref()
            .unwrap()
            .error
            .clone()
            .unwrap_or_default()
    }

    fn problems_of(read: &SharedRead) -> Vec<(Option<String>, String)> {
        read.file
            .as_ref()
            .unwrap()
            .problems
            .iter()
            .map(|p| (p.name.clone(), p.message.clone()))
            .collect()
    }

    #[test]
    fn a_project_without_the_file_shares_nothing() {
        let p = project();
        assert_eq!(read(p.path()), SharedRead::default());
    }

    #[test]
    fn a_valid_file_is_read_with_its_hash() {
        let p = project();
        let text = r#"{
  "schemaVersion": 1,
  "configurations": [
    { "name": "Phone", "module": ":app", "variant": "debug" },
    {
      "name": "Watch",
      "module": ":wear",
      "variant": "release",
      "task": ":wear:bundleRelease",
      "launch": { "kind": "deepLink", "uri": "myapp://home" },
      "logcatFilter": "level:warn"
    }
  ]
}"#;
        shared(p.path(), text);

        let read = read(p.path());

        assert_eq!(
            read.configurations,
            vec![
                config("Phone", ":app"),
                RunConfiguration {
                    variant: "release".into(),
                    task: Some(":wear:bundleRelease".into()),
                    launch: RunLaunch::DeepLink {
                        uri: "myapp://home".into()
                    },
                    logcat_filter: Some("level:warn".into()),
                    ..config("Watch", ":wear")
                }
            ]
        );
        let file = read.file.unwrap();
        assert_eq!(file.path, ".keynobi/run-configurations.json");
        assert_eq!(
            file.sha256.as_deref(),
            Some(sha256_hex(text.as_bytes()).as_str())
        );
        assert_eq!(file.sha256.as_ref().map(String::len), Some(64));
        assert_eq!(file.error, None);
        assert!(file.problems.is_empty());
    }

    #[test]
    fn a_file_over_the_cap_is_not_read() {
        let p = project();
        let padding = " ".repeat(MAX_SHARED_FILE_BYTES as usize);
        shared(
            p.path(),
            &format!("{{\"schemaVersion\": 1, \"configurations\": []}}{padding}"),
        );

        let read = read(p.path());

        assert!(read.configurations.is_empty());
        assert!(error_of(&read).contains("larger than 64 KiB"), "{read:?}");
        assert_eq!(read.sha256(), None);
    }

    #[test]
    fn unknown_fields_are_refused_at_every_level() {
        let p = project();
        shared(
            p.path(),
            r#"{"schemaVersion": 1, "configurations": [], "env": {"TOKEN": "x"}}"#,
        );
        assert!(error_of(&read(p.path())).contains("unknown field `env`"));

        shared(
            p.path(),
            r#"{"schemaVersion": 1, "configurations": [
                {"name": "Serial", "module": ":app", "variant": "debug", "serial": "emulator-5554"},
                {"name": "Flags", "module": ":app", "variant": "debug",
                 "launch": {"kind": "activity", "name": ".Main", "args": "--ez x true"}},
                {"name": "Unit", "module": ":app", "variant": "debug",
                 "launch": {"kind": "default", "command": "rm -rf /"}},
                {"name": "Ok", "module": ":app", "variant": "debug"}
            ]}"#,
        );
        let read = read(p.path());
        assert_eq!(read.configurations, vec![config("Ok", ":app")]);
        let problems = problems_of(&read);
        let names: Vec<Option<&str>> = problems.iter().map(|(n, _)| n.as_deref()).collect();
        assert_eq!(names, [Some("Serial"), Some("Flags"), Some("Unit")]);
        assert!(
            problems[0].1.contains("unknown field `serial`"),
            "{problems:?}"
        );
        assert!(
            problems[1].1.contains("unknown field `args`"),
            "{problems:?}"
        );
        assert!(
            problems[2].1.contains("unknown field `command`"),
            "{problems:?}"
        );
    }

    #[test]
    fn the_schema_version_must_be_the_one_this_version_reads() {
        let p = project();
        for (text, expected) in [
            (r#"{"configurations": []}"#, "no schemaVersion"),
            (
                r#"{"schemaVersion": "1", "configurations": []}"#,
                "no schemaVersion",
            ),
            (
                r#"{"schemaVersion": 2, "configurations": [], "profiles": []}"#,
                "schemaVersion 2, and this Keynobi reads version 1",
            ),
            ("[1, 2]", "no schemaVersion"),
            ("{ not json", "not valid JSON"),
        ] {
            shared(p.path(), text);
            let read = read(p.path());
            assert!(read.configurations.is_empty());
            assert!(error_of(&read).contains(expected), "{text}: {read:?}");
        }
    }

    #[test]
    fn each_configuration_is_validated_like_a_local_one() {
        let p = project();
        shared(
            p.path(),
            r#"{"schemaVersion": 1, "configurations": [
                {"name": "Library", "module": ":lib", "variant": "debug"},
                {"name": "Other module", "module": ":app", "variant": "debug", "task": ":wear:assembleDebug"},
                {"name": "Flag", "module": ":app", "variant": "debug", "task": "--init-script=x"},
                {"name": "Activity", "module": ":app", "variant": "debug",
                 "launch": {"kind": "activity", "name": ".Main; reboot"}},
                {"name": "Link", "module": ":app", "variant": "debug",
                 "launch": {"kind": "deepLink", "uri": "no scheme"}},
                {"name": "Phone", "module": ":app", "variant": "debug"},
                {"name": "phone", "module": ":wear", "variant": "debug"},
                {"module": ":app", "variant": "debug"}
            ]}"#,
        );

        let read = read(p.path());

        assert_eq!(read.configurations, vec![config("Phone", ":app")]);
        let problems = problems_of(&read);
        let expected = [
            (Some("Library"), "':lib' is not an application module"),
            (Some("Other module"), "is not a task of :app"),
            (Some("Flag"), "Invalid Gradle task"),
            (Some("Activity"), "activity"),
            (Some("Link"), "Invalid deep link"),
            (
                Some("phone"),
                "A run configuration named 'Phone' already exists.",
            ),
            (None, "missing field `name`"),
        ];
        assert_eq!(problems.len(), expected.len(), "{problems:?}");
        for ((name, message), (want_name, want)) in problems.iter().zip(expected) {
            assert_eq!(name.as_deref(), want_name, "{problems:?}");
            assert!(message.contains(want), "{name:?}: {message}");
        }
    }

    #[test]
    fn at_most_the_configuration_cap_is_read() {
        let p = project();
        let configurations: Vec<String> = (0..run_configurations::MAX_RUN_CONFIGURATIONS + 1)
            .map(|i| format!(r#"{{"name": "C{i}", "module": ":app", "variant": "debug"}}"#))
            .collect();
        shared(
            p.path(),
            &format!(
                r#"{{"schemaVersion": 1, "configurations": [{}]}}"#,
                configurations.join(",")
            ),
        );

        let read = read(p.path());

        assert_eq!(
            read.configurations.len(),
            run_configurations::MAX_RUN_CONFIGURATIONS
        );
        let problems = problems_of(&read);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].1.contains("more than 32 configurations"));
    }

    #[test]
    fn a_file_linked_outside_the_project_is_not_read() {
        let p = project();
        let outside = TempDir::new().unwrap();
        write_file(
            outside.path(),
            "run-configurations.json",
            r#"{"schemaVersion": 1, "configurations": [{"name": "X", "module": ":app", "variant": "debug"}]}"#,
        );
        std::os::unix::fs::symlink(outside.path(), p.path().join(SHARED_DIR)).unwrap();

        let read = read(p.path());

        assert!(read.configurations.is_empty());
        assert!(error_of(&read).contains("outside the project"), "{read:?}");
        // Nor written through the link.
        let err = write(p.path(), &SharedRead::default(), &[config("X", ":app")]).unwrap_err();
        assert!(matches!(err, AppError::PermissionDenied(_)), "{err:?}");
    }

    #[test]
    fn a_directory_in_place_of_the_file_is_reported() {
        let p = project();
        std::fs::create_dir_all(p.path().join(SHARED_FILE)).unwrap();
        assert!(error_of(&read(p.path())).contains("not a regular file"));
    }

    #[test]
    fn a_written_file_holds_only_portable_fields_and_reads_back() {
        let p = project();
        let configurations = vec![
            config("Phone", ":app"),
            RunConfiguration {
                task: Some(":wear:bundleDebug".into()),
                launch: RunLaunch::Activity {
                    name: ".Main".into(),
                },
                logcat_filter: Some("level:warn".into()),
                ..config("Watch", ":wear")
            },
        ];

        write(p.path(), &read(p.path()), &configurations).unwrap();

        let text = std::fs::read_to_string(p.path().join(SHARED_FILE)).unwrap();
        assert_eq!(
            text,
            r#"{
  "schemaVersion": 1,
  "configurations": [
    {
      "name": "Phone",
      "module": ":app",
      "variant": "debug"
    },
    {
      "name": "Watch",
      "module": ":wear",
      "variant": "debug",
      "task": ":wear:bundleDebug",
      "launch": {
        "kind": "activity",
        "name": ".Main"
      },
      "logcatFilter": "level:warn"
    }
  ]
}
"#
        );
        assert_eq!(read(p.path()).configurations, configurations);
        // No temporary file is left beside it.
        let entries: Vec<_> = std::fs::read_dir(p.path().join(SHARED_DIR))
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(entries, ["run-configurations.json"]);

        // The last one removed takes the file and its empty directory.
        write(p.path(), &read(p.path()), &[]).unwrap();
        assert!(!p.path().join(SHARED_DIR).exists());
    }

    #[test]
    fn a_file_that_could_not_be_fully_read_is_never_rewritten() {
        let p = project();
        let text = r#"{"schemaVersion": 1, "configurations": [
            {"name": "Future", "module": ":app", "variant": "debug", "profile": "x"}
        ]}"#;
        shared(p.path(), text);

        let err = write(p.path(), &read(p.path()), &[config("Phone", ":app")]).unwrap_err();

        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert!(
            err.to_string().contains("'Future' in it is not valid"),
            "{err}"
        );
        assert_eq!(
            std::fs::read_to_string(p.path().join(SHARED_FILE)).unwrap(),
            text
        );
    }

    #[test]
    fn approval_is_needed_for_a_task_outside_assemble_or_a_deep_link_it_opens() {
        let plain = config("Phone", ":app");
        assert!(needs_approval(&plain, ":app:assembleDebug", true).is_empty());
        assert!(needs_approval(&plain, "assembleFreeDebug", true).is_empty());
        assert_eq!(
            needs_approval(&plain, ":app:publishRelease", false),
            ["builds :app:publishRelease, which is not an assemble task"]
        );
        let link = RunConfiguration {
            launch: RunLaunch::DeepLink {
                uri: "myapp://pay".into(),
            },
            ..plain
        };
        assert_eq!(
            needs_approval(&link, ":app:assembleDebug", true),
            ["opens the deep link myapp://pay"]
        );
        // A build does not open it.
        assert!(needs_approval(&link, ":app:assembleDebug", false).is_empty());
    }
}
