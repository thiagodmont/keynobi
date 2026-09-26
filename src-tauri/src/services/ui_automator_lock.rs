//! One UI Automator client per device at a time.
//!
//! A device accepts a single UiAutomation registration. A second
//! `uiautomator dump`, or a dump during an instrumentation test run, fails on
//! the device with "UiAutomationService ... already registered". Every UI
//! Automator call therefore takes the device's lock here first, and fails fast
//! while an instrumentation run that Keynobi started is using the device.
//!
//! Both registries are process-wide, so GUI commands and MCP tools running in
//! the same process share them. A separate `keynobi --mcp` process has its own.
//! Across processes, a call also holds an advisory lock on
//! `ui-automator-locks/<hash of the serial>.lock` in the data directory for
//! its duration, so the app and standalone servers take turns on a device.
//! The lock dies with its process, so a crashed holder never blocks a device.

use crate::services::build_lock::{self, LockError};
use crate::services::settings_manager;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, MutexGuard, Weak};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::Instant;

/// Most devices that can have a UI Automator call running or waiting at once.
/// Entries exist only while a call holds or waits for a device's lock.
pub const MAX_LOCKED_SERIALS: usize = 64;

/// Folder under the data directory holding one lock file per device serial.
pub const UI_AUTOMATOR_LOCKS_DIR: &str = "ui-automator-locks";

/// How often a call retries a device another process holds.
const CROSS_PROCESS_POLL: Duration = Duration::from_millis(50);

/// The device output that means another UiAutomation client is registered.
const ALREADY_REGISTERED: &str = "already registered";

static DEVICE_LOCKS: LazyLock<StdMutex<HashMap<String, Weak<AsyncMutex<()>>>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

static INSTRUMENTATION: LazyLock<InstrumentationRegistry> =
    LazyLock::new(InstrumentationRegistry::default);

fn lock_ignoring_poison<T>(m: &StdMutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Held for the whole UI Automator call; releases the device when dropped.
pub struct DeviceUiLease {
    // Declared first so the cross-process lock is released before the next
    // caller in this process gets the device.
    _file: Option<File>,
    _guard: OwnedMutexGuard<()>,
}

fn device_lock(serial: &str) -> Result<Arc<AsyncMutex<()>>, String> {
    let mut locks = lock_ignoring_poison(&DEVICE_LOCKS);
    if let Some(lock) = locks.get(serial).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    locks.retain(|_, lock| lock.strong_count() > 0);
    if locks.len() >= MAX_LOCKED_SERIALS {
        return Err(format!(
            "UI Automator is busy on {MAX_LOCKED_SERIALS} devices; try again when a call finishes"
        ));
    }
    let lock = Arc::new(AsyncMutex::new(()));
    locks.insert(serial.to_owned(), Arc::downgrade(&lock));
    Ok(lock)
}

/// The cross-process lock file for `serial` under `data_dir`.
///
/// Keyed by serial, the name adb addresses the device by: two processes
/// capturing through one serial reach the same device.
pub fn device_lock_path(data_dir: &Path, serial: &str) -> PathBuf {
    data_dir
        .join(UI_AUTOMATOR_LOCKS_DIR)
        .join(build_lock::lock_file_name(serial))
}

/// Wait for `serial`'s UI Automator lock until `deadline`.
///
/// Fails at once while a Keynobi instrumentation run is using the device, and
/// checks again after waiting in case a run started meanwhile. The returned
/// lease is a tokio mutex guard, deliberately held across the awaits of the
/// whole call: it is the async lock that serializes the device. It also holds
/// the device's lock file, which serializes it against other processes.
pub async fn acquire(
    serial: &str,
    deadline: Instant,
    deadline_label: &str,
) -> Result<DeviceUiLease, String> {
    acquire_in(
        &settings_manager::data_dir(),
        serial,
        deadline,
        deadline_label,
    )
    .await
}

async fn acquire_in(
    data_dir: &Path,
    serial: &str,
    deadline: Instant,
    deadline_label: &str,
) -> Result<DeviceUiLease, String> {
    INSTRUMENTATION.ensure_idle(serial)?;
    let lock = device_lock(serial)?;
    let guard = tokio::time::timeout_at(deadline, lock.lock_owned())
        .await
        .map_err(|_| {
            format!(
                "UI Automator on {serial} is busy with another request; gave up after the \
                 {deadline_label} total deadline"
            )
        })?;
    let file = lock_device_file(
        &device_lock_path(data_dir, serial),
        serial,
        deadline,
        deadline_label,
    )
    .await?;
    INSTRUMENTATION.ensure_idle(serial)?;
    Ok(DeviceUiLease {
        _file: file,
        _guard: guard,
    })
}

/// Take the device's lock file, retrying while another process holds it,
/// until `deadline`. `Ok(None)` when the file cannot be used at all (for
/// example an unwritable data directory): the call then runs without it.
async fn lock_device_file(
    path: &Path,
    serial: &str,
    deadline: Instant,
    deadline_label: &str,
) -> Result<Option<File>, String> {
    loop {
        match build_lock::try_lock_file(path) {
            Ok(file) => return Ok(Some(file)),
            Err(LockError::Io(e)) => {
                tracing::warn!("UI Automator on {serial} runs without the cross-process lock: {e}");
                return Ok(None);
            }
            Err(LockError::Held { pid }) => {
                let now = Instant::now();
                if now >= deadline {
                    let holder = pid.map_or_else(
                        || "another Keynobi process".to_string(),
                        |pid| format!("another Keynobi process (pid {pid})"),
                    );
                    return Err(format!(
                        "UI Automator on {serial} is busy with a request from {holder}; gave up \
                         after the {deadline_label} total deadline"
                    ));
                }
                tokio::time::sleep_until((now + CROSS_PROCESS_POLL).min(deadline)).await;
            }
        }
    }
}

/// True when device output reports another UiAutomation client.
pub fn reports_already_registered(output: &[u8]) -> bool {
    String::from_utf8_lossy(output).contains(ALREADY_REGISTERED)
}

/// The error for a device whose UiAutomation is held by a client outside Keynobi.
pub fn foreign_client_busy(serial: &str) -> String {
    format!(
        "Device {serial} is busy: another UI Automator client is registered (for example a test \
         run from Android Studio or another tool). Try again when it finishes."
    )
}

fn instrumentation_busy(serial: &str) -> String {
    format!(
        "Device {serial} is busy: instrumentation running. A connected test run started by \
         Keynobi is using UI Automator; try again when the tests finish."
    )
}

/// Whether a Gradle task runs instrumentation tests on connected devices
/// (`connectedAndroidTest`, `:app:connectedDebugAndroidTest`, `connectedCheck`).
pub fn is_instrumentation_task(task: &str) -> bool {
    task.rsplit(':')
        .next()
        .is_some_and(|name| name.starts_with("connected"))
}

/// Registers an instrumentation run for as long as the value lives.
pub struct InstrumentationRun {
    id: u64,
    registry: &'static InstrumentationRegistry,
}

impl Drop for InstrumentationRun {
    fn drop(&mut self) {
        lock_ignoring_poison(&self.registry.runs).remove(&self.id);
    }
}

/// Mark the devices `task` tests on as busy until the returned value drops.
///
/// Connected tests run on every connected device unless `ANDROID_SERIAL`
/// (from `env`, else the inherited environment) names one. Returns `None` for
/// tasks that are not instrumentation runs.
pub fn begin_instrumentation_for_task(
    task: &str,
    env: &[(String, String)],
) -> Option<InstrumentationRun> {
    if !is_instrumentation_task(task) {
        return None;
    }
    let serial = env
        .iter()
        .find(|(k, _)| k == "ANDROID_SERIAL")
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var("ANDROID_SERIAL").ok())
        .filter(|s| !s.trim().is_empty());
    Some(INSTRUMENTATION.begin(serial))
}

/// Running instrumentation, by run id: `Some(serial)` for one device, `None`
/// for all. Builds are one at a time per process, so this holds at most one
/// entry per build that has not finished yet.
#[derive(Default)]
pub struct InstrumentationRegistry {
    runs: StdMutex<HashMap<u64, Option<String>>>,
    next_id: AtomicU64,
}

impl InstrumentationRegistry {
    fn begin(&'static self, serial: Option<String>) -> InstrumentationRun {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        lock_ignoring_poison(&self.runs).insert(id, serial);
        InstrumentationRun { id, registry: self }
    }

    fn ensure_idle(&self, serial: &str) -> Result<(), String> {
        let busy = lock_ignoring_poison(&self.runs)
            .values()
            .any(|scope| scope.as_deref().is_none_or(|s| s == serial));
        if busy {
            Err(instrumentation_busy(serial))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// Mark one device as running instrumentation (tests share the process,
    /// so they never mark every device on the global registry).
    pub fn begin_instrumentation_on(serial: &str) -> InstrumentationRun {
        INSTRUMENTATION.begin(Some(serial.to_owned()))
    }

    pub fn instrumentation_active_on(serial: &str) -> bool {
        INSTRUMENTATION.ensure_idle(serial).is_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaked_registry() -> &'static InstrumentationRegistry {
        Box::leak(Box::default())
    }

    #[test]
    fn connected_test_tasks_are_instrumentation() {
        for task in [
            "connectedAndroidTest",
            "connectedDebugAndroidTest",
            ":app:connectedDebugAndroidTest",
            "connectedCheck",
        ] {
            assert!(is_instrumentation_task(task), "{task}");
        }
        for task in ["testDebug", "assembleDebug", ":app:lint", "app:deviceCheck"] {
            assert!(!is_instrumentation_task(task), "{task}");
        }
    }

    #[test]
    fn a_run_without_a_serial_makes_every_device_busy_until_it_ends() {
        let registry = leaked_registry();
        let run = registry.begin(None);
        let err = registry.ensure_idle("emulator-5554").unwrap_err();
        assert!(err.contains("busy: instrumentation running"), "{err}");
        assert!(registry.ensure_idle("R58M123ABC").is_err());
        drop(run);
        assert!(registry.ensure_idle("emulator-5554").is_ok());
    }

    #[test]
    fn a_run_on_one_serial_leaves_other_devices_free() {
        let registry = leaked_registry();
        let _run = registry.begin(Some("emulator-5554".into()));
        assert!(registry.ensure_idle("emulator-5554").is_err());
        assert!(registry.ensure_idle("emulator-5556").is_ok());
    }

    #[test]
    fn android_serial_in_the_gradle_env_scopes_the_run() {
        let serial = "lock-test-android-serial";
        let env = vec![("ANDROID_SERIAL".to_string(), serial.to_string())];
        assert!(begin_instrumentation_for_task("assembleDebug", &env).is_none());
        let run = begin_instrumentation_for_task(":app:connectedDebugAndroidTest", &env)
            .expect("connected tests register a run");
        assert!(test_support::instrumentation_active_on(serial));
        assert!(!test_support::instrumentation_active_on(
            "lock-test-other-device"
        ));
        drop(run);
        assert!(!test_support::instrumentation_active_on(serial));
    }

    #[test]
    fn already_registered_output_is_recognised() {
        assert!(reports_already_registered(
            b"java.lang.IllegalStateException: UiAutomationService \
              android.accessibilityservice.IAccessibilityServiceClient$Stub$Proxy@1 already registered!"
        ));
        assert!(!reports_already_registered(b"ERROR: null root node"));
    }

    #[tokio::test]
    async fn released_serials_are_forgotten() {
        let serial = "lock-test-forgotten";
        let lease = acquire(serial, Instant::now() + Duration::from_secs(1), "1 s")
            .await
            .unwrap();
        assert!(lock_ignoring_poison(&DEVICE_LOCKS)
            .get(serial)
            .is_some_and(|w| w.strong_count() > 0));
        drop(lease);
        // The next insert prunes it; the entry no longer keeps a lock alive.
        assert!(lock_ignoring_poison(&DEVICE_LOCKS)
            .get(serial)
            .is_none_or(|w| w.strong_count() == 0));
    }

    #[tokio::test]
    async fn waiting_for_a_held_device_gives_up_at_the_deadline() {
        let serial = "lock-test-deadline";
        let _held = acquire(serial, Instant::now() + Duration::from_secs(1), "1 s")
            .await
            .unwrap();
        let start = std::time::Instant::now();
        let err = acquire(
            serial,
            Instant::now() + Duration::from_millis(200),
            "200 ms",
        )
        .await
        .err()
        .expect("the device is held");
        assert!(
            err.contains("gave up after the 200 ms total deadline"),
            "{err}"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    const CHILD_DIR: &str = "KEYNOBI_UI_LOCK_TEST_DIR";
    const CHILD_SERIAL: &str = "KEYNOBI_UI_LOCK_TEST_SERIAL";
    const WAIT_BOUND: Duration = Duration::from_secs(30);

    /// Run in a child process by the tests below: holds a device's lock
    /// until told to release it.
    #[tokio::test]
    #[ignore = "helper the cross-process tests run in a child process"]
    async fn child_holds_a_device_lock() {
        let (Ok(dir), Ok(serial)) = (std::env::var(CHILD_DIR), std::env::var(CHILD_SERIAL)) else {
            return;
        };
        let dir = PathBuf::from(dir);
        let _lease = acquire_in(&dir, &serial, Instant::now() + WAIT_BOUND, "30 s")
            .await
            .expect("the device is free");
        std::fs::write(dir.join("held"), "").unwrap();
        let deadline = std::time::Instant::now() + WAIT_BOUND;
        while !dir.join("release").exists() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Another test process holding `serial`'s lock in `dir`.
    fn spawn_holder(dir: &Path, serial: &str) -> std::process::Child {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "services::ui_automator_lock::tests::child_holds_a_device_lock",
                "--ignored",
                "--test-threads=1",
            ])
            .env(CHILD_DIR, dir)
            .env(CHILD_SERIAL, serial)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn the holding process");
        let deadline = std::time::Instant::now() + WAIT_BOUND;
        while !dir.join("held").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the other process never took the lock"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        child
    }

    #[tokio::test]
    async fn a_device_another_process_holds_waits_for_it_and_then_is_free() {
        let dir = tempfile::tempdir().unwrap();
        let serial = "lock-test-cross-process";
        let mut child = spawn_holder(dir.path(), serial);

        let start = std::time::Instant::now();
        let err = acquire_in(
            dir.path(),
            serial,
            Instant::now() + Duration::from_millis(300),
            "300 ms",
        )
        .await
        .err()
        .expect("the other process holds the device");
        assert!(
            err.contains(&format!("another Keynobi process (pid {})", child.id()))
                && err.contains("gave up after the 300 ms total deadline"),
            "{err}"
        );
        assert!(start.elapsed() >= Duration::from_millis(300));

        let owned = dir.path().to_path_buf();
        let waiting = tokio::spawn(async move {
            acquire_in(&owned, serial, Instant::now() + WAIT_BOUND, "30 s")
                .await
                .map(drop)
        });
        std::fs::write(dir.path().join("release"), "").unwrap();
        waiting
            .await
            .unwrap()
            .expect("free once the other process releases it");
        child.wait().unwrap();
    }

    #[tokio::test]
    async fn a_process_that_dies_holding_a_device_does_not_block_it() {
        let dir = tempfile::tempdir().unwrap();
        let serial = "lock-test-crashed-holder";
        let mut child = spawn_holder(dir.path(), serial);
        child.kill().unwrap();
        child.wait().unwrap();

        acquire_in(
            dir.path(),
            serial,
            Instant::now() + Duration::from_secs(2),
            "2 s",
        )
        .await
        .map(drop)
        .expect("the lock died with its process");
    }

    #[tokio::test]
    async fn the_device_lock_file_is_released_on_drop_and_on_panic() {
        let dir = tempfile::tempdir().unwrap();
        let serial = "lock-test-file-release";
        let path = device_lock_path(dir.path(), serial);
        let deadline = || Instant::now() + Duration::from_secs(2);

        let lease = acquire_in(dir.path(), serial, deadline(), "2 s")
            .await
            .unwrap();
        assert_eq!(
            build_lock::try_lock_file(&path).err(),
            Some(LockError::Held {
                pid: Some(std::process::id())
            })
        );
        drop(lease);
        drop(build_lock::test_support::lock_once_free(&path).expect("released on drop"));

        let owned = dir.path().to_path_buf();
        let panicked = tokio::spawn(async move {
            let _lease = acquire_in(&owned, serial, deadline(), "2 s").await.unwrap();
            panic!("the capture failed");
        })
        .await;
        assert!(panicked.unwrap_err().is_panic());
        drop(build_lock::test_support::lock_once_free(&path).expect("released on panic"));
    }

    #[tokio::test]
    async fn a_device_locked_elsewhere_is_busy_after_the_deadline_and_others_are_free() {
        let dir = tempfile::tempdir().unwrap();
        // Another open of the lock file conflicts like another process's.
        let _elsewhere =
            build_lock::try_lock_file(&device_lock_path(dir.path(), "lock-test-held-a")).unwrap();

        let start = std::time::Instant::now();
        let err = acquire_in(
            dir.path(),
            "lock-test-held-a",
            Instant::now() + Duration::from_millis(200),
            "200 ms",
        )
        .await
        .err()
        .expect("the device is held elsewhere");
        let waited = start.elapsed();
        assert!(
            err.starts_with("UI Automator on lock-test-held-a is busy with a request from another Keynobi process")
                && err.contains("gave up after the 200 ms total deadline"),
            "{err}"
        );
        assert!(
            waited >= Duration::from_millis(200) && waited < Duration::from_secs(2),
            "{waited:?}"
        );

        let start = std::time::Instant::now();
        acquire_in(
            dir.path(),
            "lock-test-held-b",
            Instant::now() + Duration::from_secs(5),
            "5 s",
        )
        .await
        .map(drop)
        .expect("another device is free");
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
