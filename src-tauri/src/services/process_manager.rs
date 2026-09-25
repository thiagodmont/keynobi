use crate::utils::line_reader::CappedLines;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;

static NEXT_PROCESS_ID: AtomicU32 = AtomicU32::new(1);

/// Most processes tracked at once.
const MAX_PROCESSES: usize = 10;

/// How long to keep reading output after the process has exited. A descendant
/// that inherited the pipes (for example a daemon the process started) can keep
/// them open indefinitely; without a bound the exit would never be reported.
const POST_EXIT_DRAIN: Duration = Duration::from_secs(2);

/// How long `cancel` waits after SIGTERM before sending SIGKILL. Gradle needs
/// SIGTERM to stop its build in the daemon cleanly.
pub const CANCEL_GRACE: Duration = Duration::from_secs(5);

/// The grace `shutdown_all` gets on app and MCP server exit.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// How long `shutdown_all` waits for exits to be reported after SIGKILL.
const FORCE_KILL_WAIT: Duration = Duration::from_secs(1);

/// Opaque handle uniquely identifying a managed process.
pub type ProcessId = u32;

/// Outcome of a line read from a spawned process's output streams.
#[derive(Debug, Clone)]
pub struct ProcessLine {
    pub pid: ProcessId,
    pub is_stderr: bool,
    pub text: String,
}

/// How a managed process ended.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessTermination {
    /// Process exited with this exit code.
    ExitCode(i32),
    /// Process was killed by a Unix signal (15=SIGTERM, 9=SIGKILL).
    Signal(i32),
    /// Process was stopped by `cancel()` or `shutdown_all()` while it ran.
    Cancelled,
}

/// What the task that owns a child has been asked to do. Requests only
/// escalate: `Run` → `Terminate` → `Kill`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StopRequest {
    Run,
    /// SIGTERM now, SIGKILL if it still runs after `grace`.
    Terminate {
        grace: Duration,
    },
    /// SIGKILL now, and stop reading output once it has exited.
    Kill,
}

impl StopRequest {
    fn rank(self) -> u8 {
        match self {
            StopRequest::Run => 0,
            StopRequest::Terminate { .. } => 1,
            StopRequest::Kill => 2,
        }
    }
}

fn request_stop(stop: &watch::Sender<StopRequest>, request: StopRequest) {
    stop.send_if_modified(|current| {
        if request.rank() > current.rank() {
            *current = request;
            true
        } else {
            false
        }
    });
}

/// Internal tracking record for a running process. The child itself is owned
/// by its reader task; everything else reaches it through `stop`.
pub(crate) struct ProcessRecord {
    /// Background task that owns the child, streams its output, and waits for its exit.
    _reader_task: JoinHandle<()>,
    /// Stop requests for the reader task. Closed once the task has finished,
    /// after `on_exit` has returned.
    stop: watch::Sender<StopRequest>,
}

/// Per-process callbacks dispatched from the reader task.
pub struct SpawnOptions {
    /// Called with each output line (stdout or stderr), on a tokio task.
    pub on_line: Box<dyn Fn(ProcessLine) + Send + Sync + 'static>,
    /// Called once when the process exits.
    pub on_exit: Box<dyn Fn(ProcessId, ProcessTermination) + Send + Sync + 'static>,
}

pub struct ProcessManagerInner {
    pub(crate) processes: HashMap<ProcessId, ProcessRecord>,
    /// Set by `shutdown_all`; no process is started afterwards.
    shut_down: bool,
}

impl Default for ProcessManagerInner {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManagerInner {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            shut_down: false,
        }
    }
}

/// What `ProcessManager::shutdown_all` did, by process.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ShutdownReport {
    /// Ended within the grace period.
    pub stopped: Vec<ProcessId>,
    /// Still running, or still draining output, after the grace period:
    /// SIGKILLed if running, and their remaining output dropped.
    pub killed: Vec<ProcessId>,
    /// Not reported as ended even after the kill; given up on.
    pub unresponsive: Vec<ProcessId>,
}

pub struct ProcessManager(pub Arc<Mutex<ProcessManagerInner>>);

impl ProcessManager {
    pub fn new() -> Self {
        ProcessManager(Arc::new(Mutex::new(ProcessManagerInner::new())))
    }

    /// Stop every process and refuse new ones: SIGTERM to all, SIGKILL to
    /// those still running after `grace`. Returns once every exit has been
    /// reported (`on_exit` returned), or after at most `grace` plus
    /// `FORCE_KILL_WAIT`.
    pub async fn shutdown_all(&self, grace: Duration) -> ShutdownReport {
        let running: Vec<(ProcessId, watch::Sender<StopRequest>)> = {
            let mut inner = self.0.lock().await;
            inner.shut_down = true;
            inner
                .processes
                .iter()
                .map(|(id, record)| (*id, record.stop.clone()))
                .collect()
        };
        let mut report = ShutdownReport::default();
        if running.is_empty() {
            return report;
        }
        for (_, stop) in &running {
            request_stop(stop, StopRequest::Terminate { grace });
        }
        let _ = tokio::time::timeout(grace, all_ended(&running)).await;

        let (stopped, left): (Vec<_>, Vec<_>) =
            running.into_iter().partition(|(_, stop)| stop.is_closed());
        report.stopped = stopped.into_iter().map(|(id, _)| id).collect();
        for (_, stop) in &left {
            request_stop(stop, StopRequest::Kill);
        }
        let _ = tokio::time::timeout(FORCE_KILL_WAIT, all_ended(&left)).await;
        for (id, stop) in left {
            if stop.is_closed() {
                report.killed.push(id);
            } else {
                report.unresponsive.push(id);
            }
        }

        if report.killed.is_empty() && report.unresponsive.is_empty() {
            tracing::info!("Stopped processes on shutdown: {:?}", report.stopped);
        } else {
            tracing::warn!(
                "Stopped processes on shutdown: {:?}; killed after {grace:?}: {:?}; \
                 unresponsive: {:?}",
                report.stopped,
                report.killed,
                report.unresponsive
            );
        }
        report
    }
}

async fn all_ended(processes: &[(ProcessId, watch::Sender<StopRequest>)]) {
    for (_, stop) in processes {
        stop.closed().await;
    }
}

impl Clone for ProcessManager {
    fn clone(&self) -> Self {
        ProcessManager(self.0.clone())
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Send `signal` to `child`'s process group (falling back to the child alone),
/// through the handle only. `id()` is `None` once the child has been reaped;
/// until then its PID, and the group named after it, cannot be reused, so an
/// unrelated process is never signalled. Returns whether a signal was sent.
fn signal_child(child: &Child, signal: libc::c_int) -> bool {
    let Some(pid) = child.id() else {
        return false;
    };
    let pid = pid as libc::pid_t;
    // SAFETY: plain syscalls on a PID we own and have not reaped.
    unsafe { libc::killpg(pid, signal) == 0 || libc::kill(pid, signal) == 0 }
}

/// Spawn a child process and stream its stdout/stderr line-by-line.
///
/// Returns a `ProcessId` that can be used to cancel the process.
/// The `options.on_line` callback is called from a dedicated tokio task for
/// every line produced by stdout or stderr. `options.on_exit` is called once
/// when the process terminates.
///
/// The child leads its own process group, so stop signals also reach helpers
/// it started in that group (a wrapper script's JVM), as Ctrl-C in a terminal
/// would. A Gradle daemon puts itself in a new session and is not signalled:
/// it is shared with other builds and IDEs.
///
/// # Errors
/// Returns an error string if the process fails to start.
pub async fn spawn(
    manager: &Arc<Mutex<ProcessManagerInner>>,
    cmd: &str,
    args: &[&str],
    cwd: PathBuf,
    env_extra: Vec<(String, String)>,
    options: SpawnOptions,
) -> Result<ProcessId, String> {
    let id = NEXT_PROCESS_ID.fetch_add(1, Ordering::SeqCst);

    let mut inner = manager.lock().await;
    if inner.shut_down {
        return Err("Keynobi is shutting down".into());
    }
    if inner.processes.len() >= MAX_PROCESSES {
        return Err(format!(
            "Maximum concurrent processes ({MAX_PROCESSES}) reached"
        ));
    }

    let mut command = Command::new(cmd);
    command
        .args(args)
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // Inherit base environment and add extras.
        .envs(env_extra)
        .process_group(0)
        // Only if the reader task is dropped unfinished (its runtime shut down).
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn '{cmd}': {e}"))?;

    let stdout = child.stdout.take().ok_or("No stdout")?;
    let stderr = child.stderr.take().ok_or("No stderr")?;

    let on_line = options.on_line;
    let on_exit = options.on_exit;
    let (stop_tx, mut stop_rx) = watch::channel(StopRequest::Run);

    // The reader task owns `child`: it alone waits for it and signals it, so
    // no signal can reach the PID after it has been reaped.
    let reader_task = {
        let manager_for_cleanup = manager.clone();

        tokio::spawn(async move {
            let mut stdout_lines = CappedLines::new(BufReader::new(stdout));
            let mut stderr_lines = CappedLines::new(BufReader::new(stderr));
            let mut stdout_done = false;
            let mut stderr_done = false;
            let mut stop_open = true;
            let mut exit_status = None;
            let mut drain_until = None;
            let mut kill_at = None;
            // Stopped by a request while it ran: reported as `Cancelled`.
            let mut cancelled = false;
            // Killed on request: its remaining output is not waited for.
            let mut forced = false;

            // Read both streams, wait for exit, and serve stop requests
            // concurrently. When one stream closes we keep reading the other so
            // no output is lost; once the process has exited, remaining output
            // is read for POST_EXIT_DRAIN.
            loop {
                if stdout_done && stderr_done && exit_status.is_some() {
                    break;
                }
                let now = tokio::time::Instant::now();
                tokio::select! {
                    result = stdout_lines.next_line(), if !stdout_done => {
                        match result {
                            Ok(Some(line)) => on_line(ProcessLine { pid: id, is_stderr: false, text: line }),
                            _ => stdout_done = true,
                        }
                    }
                    result = stderr_lines.next_line(), if !stderr_done => {
                        match result {
                            Ok(Some(line)) => on_line(ProcessLine { pid: id, is_stderr: true, text: line }),
                            _ => stderr_done = true,
                        }
                    }
                    status = child.wait(), if exit_status.is_none() => {
                        exit_status = Some(status);
                        if forced {
                            break;
                        }
                        drain_until = Some(tokio::time::Instant::now() + POST_EXIT_DRAIN);
                    }
                    changed = stop_rx.changed(), if stop_open => {
                        if changed.is_err() {
                            stop_open = false;
                            continue;
                        }
                        let request = *stop_rx.borrow_and_update();
                        match request {
                            StopRequest::Run => {}
                            StopRequest::Terminate { grace } => {
                                if exit_status.is_none() {
                                    cancelled = true;
                                    signal_child(&child, libc::SIGTERM);
                                    kill_at = Some(tokio::time::Instant::now() + grace);
                                }
                            }
                            StopRequest::Kill => {
                                if exit_status.is_some() {
                                    break;
                                }
                                cancelled = true;
                                forced = true;
                                signal_child(&child, libc::SIGKILL);
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(kill_at.unwrap_or(now)), if kill_at.is_some() && exit_status.is_none() => {
                        kill_at = None;
                        signal_child(&child, libc::SIGKILL);
                    }
                    _ = tokio::time::sleep_until(drain_until.unwrap_or(now)), if drain_until.is_some() => break,
                }
            }

            let exit_status = match exit_status {
                Some(status) => status,
                None => child.wait().await,
            };
            let termination = match exit_status {
                Ok(status) => {
                    if let Some(code) = status.code() {
                        ProcessTermination::ExitCode(code)
                    } else {
                        // Killed by a signal (Unix). Try to read the signal number.
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            ProcessTermination::Signal(status.signal().unwrap_or(0))
                        }
                        #[cfg(not(unix))]
                        ProcessTermination::Signal(0)
                    }
                }
                Err(_) => ProcessTermination::Signal(0),
            };

            // A stopped process is reported as cancelled, not by its raw
            // signal, so callers can tell a stop from an unexpected death.
            // One that had already exited when the request came keeps its
            // own status.
            let final_termination = if cancelled {
                ProcessTermination::Cancelled
            } else {
                termination
            };
            manager_for_cleanup.lock().await.processes.remove(&id);
            on_exit(id, final_termination);
        })
    };

    let record = ProcessRecord {
        _reader_task: reader_task,
        stop: stop_tx,
    };

    inner.processes.insert(id, record);

    Ok(id)
}

/// Stop the process: SIGTERM, then SIGKILL after `CANCEL_GRACE` if it still
/// runs. It stays tracked until its exit is reported.
///
/// No-op if the process ID is unknown (already exited or never created), or
/// if it has already exited.
pub async fn cancel(manager: &Mutex<ProcessManagerInner>, id: ProcessId) {
    cancel_with_grace(manager, id, CANCEL_GRACE).await;
}

async fn cancel_with_grace(manager: &Mutex<ProcessManagerInner>, id: ProcessId, grace: Duration) {
    let inner = manager.lock().await;
    if let Some(record) = inner.processes.get(&id) {
        request_stop(&record.stop, StopRequest::Terminate { grace });
    }
}

/// Remove a process record from the tracking map (called after natural exit).
pub async fn remove(manager: &Mutex<ProcessManagerInner>, id: ProcessId) {
    manager.lock().await.processes.remove(&id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    /// Bounds a wait that only fails on a hang. Spawning and reaping a child
    /// can take seconds on a loaded machine.
    const HANG_GUARD: Duration = Duration::from_secs(30);

    /// Poll `done` every 20 ms until it holds; false after [`HANG_GUARD`].
    async fn eventually(mut done: impl AsyncFnMut() -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + HANG_GUARD;
        while !done().await {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }

    /// Gradle can leave a descendant holding its stdout/stderr open after the
    /// wrapper exits. The exit used to be observed only after both pipes hit
    /// EOF, so the build never finished and the build slot stayed taken.
    #[tokio::test]
    async fn exit_is_reported_while_a_descendant_still_holds_the_pipes() {
        let manager = ProcessManager::new();
        let lines: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = StdMutex::new(Some(tx));

        spawn(
            &manager.0,
            "sh",
            &["-c", "sleep 20 & echo done; exit 3"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new({
                    let lines = lines.clone();
                    move |l| lines.lock().unwrap().push(l.text)
                }),
                on_exit: Box::new(move |_, termination| {
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(termination);
                    }
                }),
            },
        )
        .await
        .unwrap();

        let termination = tokio::time::timeout(std::time::Duration::from_secs(10), rx)
            .await
            .expect("exit must be reported without waiting for the descendant")
            .unwrap();
        assert_eq!(termination, ProcessTermination::ExitCode(3));
        assert_eq!(*lines.lock().unwrap(), vec!["done".to_string()]);
    }

    #[tokio::test]
    async fn spawn_and_collect_output() {
        let manager = ProcessManager::new();
        let lines: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(vec![]));
        let lines_clone = lines.clone();
        let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let exited_clone = exited.clone();

        let id = spawn(
            &manager.0,
            "echo",
            &["hello world"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(move |l| {
                    lines_clone.lock().unwrap().push(l.text);
                }),
                on_exit: Box::new(move |_, _termination| {
                    exited_clone.store(true, Ordering::SeqCst);
                }),
            },
        )
        .await
        .unwrap();

        let collected = eventually(async || {
            lines
                .lock()
                .unwrap()
                .iter()
                .any(|l| l.contains("hello world"))
        })
        .await;
        remove(&manager.0, id).await;

        assert!(collected, "got {:?}", lines.lock().unwrap());
    }

    /// A tool printing a multi-megabyte line must not be buffered whole, and a
    /// non-UTF-8 byte must not stop the stream from being drained.
    #[tokio::test]
    async fn overlong_and_invalid_utf8_lines_are_bounded_and_do_not_end_the_stream() {
        use crate::utils::line_reader::MAX_LINE_BYTES;

        let manager = ProcessManager::new();
        let lines: Arc<StdMutex<Vec<(bool, String)>>> = Arc::new(StdMutex::new(vec![]));
        let lines_clone = lines.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let done_tx = StdMutex::new(Some(done_tx));

        spawn(
            &manager.0,
            "sh",
            &[
                "-c",
                "head -c 1000000 /dev/zero | tr '\\0' a; printf '\\nbad \\377\\nnext\\n'; \
                 head -c 200000 /dev/zero | tr '\\0' b >&2; printf '\\nerr next\\n' >&2",
            ],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(move |l| {
                    lines_clone.lock().unwrap().push((l.is_stderr, l.text));
                }),
                on_exit: Box::new(move |_, _| {
                    if let Some(tx) = done_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                }),
            },
        )
        .await
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), done_rx)
            .await
            .expect("process must exit")
            .unwrap();

        let collected = lines.lock().unwrap();
        let stdout: Vec<&str> = collected
            .iter()
            .filter(|(err, _)| !err)
            .map(|(_, t)| t.as_str())
            .collect();
        let stderr: Vec<&str> = collected
            .iter()
            .filter(|(err, _)| *err)
            .map(|(_, t)| t.as_str())
            .collect();

        assert_eq!(stdout.len(), 3, "got {} stdout lines", stdout.len());
        assert!(stdout[0].starts_with(&"a".repeat(MAX_LINE_BYTES)));
        assert!(stdout[0].ends_with(&format!(
            "… [truncated {} bytes]",
            1_000_000 - MAX_LINE_BYTES
        )));
        assert_eq!(stdout[1], "bad \u{fffd}");
        assert_eq!(stdout[2], "next");

        assert_eq!(stderr.len(), 2, "got {} stderr lines", stderr.len());
        assert!(stderr[0].ends_with(&format!("… [truncated {} bytes]", 200_000 - MAX_LINE_BYTES)));
        assert_eq!(stderr[1], "err next");
    }

    #[tokio::test]
    async fn cancel_unknown_id_is_noop() {
        let manager = ProcessManager::new();
        // Should not panic.
        cancel(&manager.0, 99999).await;
    }

    #[test]
    fn process_termination_cancelled_flag_works() {
        // Verify that ExitCode, Signal, Cancelled are distinct
        let exit = ProcessTermination::ExitCode(0);
        let signal = ProcessTermination::Signal(15);
        let cancelled = ProcessTermination::Cancelled;
        assert!(matches!(exit, ProcessTermination::ExitCode(0)));
        assert!(matches!(signal, ProcessTermination::Signal(15)));
        assert!(matches!(cancelled, ProcessTermination::Cancelled));
        assert_ne!(exit, cancelled);
        assert_ne!(signal, cancelled);
    }

    /// A cancelled process stays tracked until its exit is reported, so a
    /// shutdown still sees one that has not died yet.
    #[tokio::test]
    async fn cancelled_process_stays_tracked_until_its_exit_is_reported() {
        let manager = ProcessManager::new();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = StdMutex::new(Some(tx));
        let id = spawn(
            &manager.0,
            "sleep",
            &["10"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(move |_, termination| {
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(termination);
                    }
                }),
            },
        )
        .await
        .unwrap();

        cancel(&manager.0, id).await;
        assert!(manager.0.lock().await.processes.contains_key(&id));

        let termination = tokio::time::timeout(HANG_GUARD, rx)
            .await
            .expect("SIGTERM must stop it")
            .unwrap();
        assert_eq!(termination, ProcessTermination::Cancelled);
        assert!(!manager.0.lock().await.processes.contains_key(&id));
    }

    #[tokio::test]
    async fn completed_processes_are_removed_from_tracking() {
        let manager = ProcessManager::new();
        let exited = Arc::new(std::sync::atomic::AtomicU32::new(0));

        for _ in 0..10 {
            let exited_clone = exited.clone();
            spawn(
                &manager.0,
                "echo",
                &["done"],
                std::env::temp_dir(),
                vec![],
                SpawnOptions {
                    on_line: Box::new(|_| {}),
                    on_exit: Box::new(move |_, _| {
                        exited_clone.fetch_add(1, Ordering::SeqCst);
                    }),
                },
            )
            .await
            .unwrap();
        }

        eventually(async || exited.load(Ordering::SeqCst) == 10).await;

        assert_eq!(exited.load(Ordering::SeqCst), 10);
        assert_eq!(manager.0.lock().await.processes.len(), 0);

        let id = spawn(
            &manager.0,
            "echo",
            &["after cleanup"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(|_, _| {}),
            },
        )
        .await
        .unwrap();
        cancel(&manager.0, id).await;
    }

    #[tokio::test]
    async fn completed_process_is_removed_before_on_exit_callback() {
        let manager = ProcessManager::new();
        {
            let mut inner = manager.0.lock().await;
            for id in 100_000..100_009 {
                inner.processes.insert(id, placeholder_record());
            }
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = std::sync::Mutex::new(Some(tx));
        let manager_for_callback = manager.clone();
        spawn(
            &manager.0,
            "echo",
            &["done"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(move |_, _| {
                    let len = manager_for_callback
                        .0
                        .try_lock()
                        .map(|inner| inner.processes.len())
                        .unwrap_or(usize::MAX);
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(len);
                    }
                }),
            },
        )
        .await
        .unwrap();

        let len_during_callback = tokio::time::timeout(HANG_GUARD, rx).await.unwrap().unwrap();

        assert_eq!(
            len_during_callback, 9,
            "completed process should be removed before on_exit runs"
        );
    }

    #[tokio::test]
    async fn completed_process_is_removed_even_if_on_exit_panics() {
        let manager = ProcessManager::new();
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));

        spawn(
            &manager.0,
            "echo",
            &["done"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(|_, _| {
                    panic!("on_exit panic should not leak process record");
                }),
            },
        )
        .await
        .unwrap();

        eventually(async || manager.0.lock().await.processes.is_empty()).await;
        std::panic::set_hook(previous_hook);

        assert_eq!(manager.0.lock().await.processes.len(), 0);
    }

    #[tokio::test]
    async fn capacity_is_checked_before_spawning_child() {
        let manager = ProcessManager::new();
        {
            let mut inner = manager.0.lock().await;
            for id in 1..=10 {
                inner.processes.insert(id, placeholder_record());
            }
        }

        let err = spawn(
            &manager.0,
            "definitely-not-a-real-keynobi-command",
            &[],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(|_, _| {}),
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Maximum concurrent processes (10) reached");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_spawns_cannot_exceed_capacity() {
        let manager = ProcessManager::new();
        {
            let mut inner = manager.0.lock().await;
            for id in 100_000..100_009 {
                inner.processes.insert(id, placeholder_record());
            }
        }

        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..50 {
            let manager = manager.clone();
            tasks.spawn(async move {
                spawn(
                    &manager.0,
                    "sleep",
                    &["2"],
                    std::env::temp_dir(),
                    vec![],
                    SpawnOptions {
                        on_line: Box::new(|_| {}),
                        on_exit: Box::new(|_, _| {}),
                    },
                )
                .await
            });
        }

        let mut spawned = Vec::new();
        while let Some(result) = tasks.join_next().await {
            if let Ok(Ok(id)) = result {
                spawned.push(id);
            }
        }

        assert_eq!(spawned.len(), 1, "only one slot should be available");
        assert!(
            manager.0.lock().await.processes.len() <= 10,
            "tracked processes must remain capped"
        );

        for id in spawned {
            cancel(&manager.0, id).await;
        }
    }

    // ── Phase 0 characterization: cancellation safety ────────────────────────

    /// `cancel()` must actually terminate a running process and report the
    /// termination as `Cancelled` (not as a raw signal) to `on_exit`.
    #[tokio::test]
    async fn cancel_terminates_running_process() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let manager = ProcessManager::new();
        let saw_cancelled = Arc::new(AtomicBool::new(false));
        let exited = Arc::new(AtomicBool::new(false));
        let saw_cancelled_cb = saw_cancelled.clone();
        let exited_cb = exited.clone();

        let id = spawn(
            &manager.0,
            "sleep",
            &["30"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(move |_, termination| {
                    if termination == ProcessTermination::Cancelled {
                        saw_cancelled_cb.store(true, Ordering::SeqCst);
                    }
                    exited_cb.store(true, Ordering::SeqCst);
                }),
            },
        )
        .await
        .unwrap();

        cancel(&manager.0, id).await;

        eventually(async || exited.load(Ordering::SeqCst)).await;

        assert!(
            exited.load(Ordering::SeqCst),
            "on_exit must fire after cancel terminates the process"
        );
        assert!(
            saw_cancelled.load(Ordering::SeqCst),
            "termination must be reported as Cancelled, not as a raw signal"
        );
    }

    // ── Stopping through the owned handle ────────────────────────────────────

    fn placeholder_record() -> ProcessRecord {
        ProcessRecord {
            _reader_task: tokio::spawn(async {}),
            stop: watch::channel(StopRequest::Run).0,
        }
    }

    /// A spawned shell script with its output and exits collected.
    struct Script {
        id: ProcessId,
        lines: tokio::sync::mpsc::UnboundedReceiver<String>,
        exits: Arc<StdMutex<Vec<ProcessTermination>>>,
    }

    impl Script {
        async fn spawn(manager: &ProcessManager, script: &str) -> Script {
            let (tx, lines) = tokio::sync::mpsc::unbounded_channel();
            let exits: Arc<StdMutex<Vec<ProcessTermination>>> = Arc::default();
            let id = spawn(
                &manager.0,
                "sh",
                &["-c", script],
                std::env::temp_dir(),
                vec![],
                SpawnOptions {
                    on_line: Box::new(move |l| {
                        let _ = tx.send(l.text);
                    }),
                    on_exit: Box::new({
                        let exits = exits.clone();
                        move |_, termination| exits.lock().unwrap().push(termination)
                    }),
                },
            )
            .await
            .unwrap();
            Script { id, lines, exits }
        }

        async fn line(&mut self) -> String {
            tokio::time::timeout(HANG_GUARD, self.lines.recv())
                .await
                .expect("the script must print its next line")
                .expect("output ended early")
        }

        /// The PID printed on the next line as `<label> <pid>`.
        async fn pid(&mut self, label: &str) -> libc::pid_t {
            let line = self.line().await;
            line.strip_prefix(label)
                .and_then(|pid| pid.trim().parse().ok())
                .unwrap_or_else(|| panic!("expected `{label} <pid>`, got {line:?}"))
        }

        async fn termination(&self, within: Duration) -> ProcessTermination {
            let deadline = tokio::time::Instant::now() + within;
            loop {
                if let Some(t) = self.exits.lock().unwrap().first() {
                    return t.clone();
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "exit not reported within {within:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }

        fn exit_count(&self) -> usize {
            self.exits.lock().unwrap().len()
        }
    }

    fn alive(pid: libc::pid_t) -> bool {
        // SAFETY: signal 0 only checks that the PID exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    async fn wait_until_gone(pid: libc::pid_t) -> bool {
        eventually(async || !alive(pid)).await
    }

    /// The only signal path goes through the child's handle, which gives no
    /// PID once the child has been reaped: that PID may already belong to an
    /// unrelated process.
    #[tokio::test]
    async fn signal_child_never_signals_a_reaped_child() {
        let mut live = Command::new("sleep").arg("10").spawn().unwrap();
        assert!(signal_child(&live, 0), "a running child is signalled");
        assert!(signal_child(&live, libc::SIGKILL));
        live.wait().await.unwrap();

        let mut exited = Command::new("true").spawn().unwrap();
        exited.wait().await.unwrap();
        assert!(!signal_child(&exited, 0));
        assert!(!signal_child(&exited, libc::SIGKILL));
    }

    /// A cancel that arrives after the process exited on its own (here while
    /// its output is still drained) must not act on it: its PID was already
    /// reaped. It keeps its own exit status.
    #[tokio::test]
    async fn cancel_after_the_process_exited_leaves_it_alone() {
        let manager = ProcessManager::new();
        // The descendant keeps the pipes open, so the record outlives the exit.
        let mut script = Script::spawn(&manager, "sleep 3 & echo pid $$; exit 0").await;
        let pid = script.pid("pid").await;
        assert!(
            wait_until_gone(pid).await,
            "the script must exit and be reaped"
        );
        assert!(manager.0.lock().await.processes.contains_key(&script.id));

        cancel(&manager.0, script.id).await;

        assert_eq!(
            script.termination(HANG_GUARD).await,
            ProcessTermination::ExitCode(0)
        );
        assert_eq!(script.exit_count(), 1);
    }

    #[tokio::test]
    async fn cancel_kills_a_process_that_ignores_sigterm_after_the_grace() {
        let manager = ProcessManager::new();
        let mut script = Script::spawn(
            &manager,
            "trap '' TERM; echo ready; while :; do sleep 1; done",
        )
        .await;
        assert_eq!(script.line().await, "ready");

        let grace = Duration::from_millis(300);
        let started = tokio::time::Instant::now();
        cancel_with_grace(&manager.0, script.id, grace).await;

        tokio::time::sleep(grace / 2).await;
        // Only meaningful while the grace lasts; a stalled test thread can
        // wake after it.
        if started.elapsed() < grace {
            assert_eq!(script.exit_count(), 0, "SIGTERM is ignored");
        }
        assert_eq!(
            script.termination(HANG_GUARD).await,
            ProcessTermination::Cancelled
        );
        assert!(started.elapsed() >= grace);
        assert_eq!(script.exit_count(), 1);
    }

    /// Shutdown stops everything within `grace + FORCE_KILL_WAIT`: a process
    /// that obeys SIGTERM, one that ignores it (already cancelled once), and
    /// one whose detached descendant keeps the output pipes open. Each exit is
    /// reported exactly once, before `shutdown_all` returns.
    #[tokio::test]
    async fn shutdown_all_stops_every_process_within_the_bound() {
        let manager = ProcessManager::new();
        let mut obeys = Script::spawn(&manager, "echo ready; exec sleep 30").await;
        let mut ignores = Script::spawn(
            &manager,
            "trap '' TERM; echo ready; while :; do sleep 1; done",
        )
        .await;
        let mut holds_pipes =
            Script::spawn(&manager, "set -m; sleep 30 & echo daemon $!; exec sleep 30").await;
        assert_eq!(obeys.line().await, "ready");
        assert_eq!(ignores.line().await, "ready");
        let daemon = holds_pipes.pid("daemon").await;
        cancel_with_grace(&manager.0, ignores.id, Duration::from_secs(60)).await;

        let grace = Duration::from_millis(500);
        let started = tokio::time::Instant::now();
        let report = manager.shutdown_all(grace).await;
        let elapsed = started.elapsed();

        assert_eq!(report.stopped, vec![obeys.id]);
        let mut killed = report.killed.clone();
        killed.sort_unstable();
        let mut expected = vec![ignores.id, holds_pipes.id];
        expected.sort_unstable();
        assert_eq!(killed, expected);
        assert!(report.unresponsive.is_empty());
        // The bound is grace + FORCE_KILL_WAIT; the slack only absorbs a
        // test thread that wakes late, and stays far below the 60 s grace
        // and 30 s sleeps an unbounded wait would take.
        assert!(
            elapsed < grace + FORCE_KILL_WAIT + Duration::from_secs(5),
            "shutdown took {elapsed:?}"
        );
        assert!(manager.0.lock().await.processes.is_empty());

        tokio::time::sleep(Duration::from_millis(200)).await;
        for script in [&obeys, &ignores, &holds_pipes] {
            assert_eq!(script.exit_count(), 1, "on_exit must fire exactly once");
            assert_eq!(
                script.termination(Duration::ZERO).await,
                ProcessTermination::Cancelled
            );
        }

        assert!(alive(daemon));
        // SAFETY: test cleanup of the descendant this test started.
        unsafe { libc::kill(daemon, libc::SIGKILL) };
    }

    /// Stop signals go to the process group, reaching a helper the process
    /// started in it (a wrapper script's JVM), but never a daemon that left
    /// the group (the shared Gradle daemon puts itself in a new session).
    #[tokio::test]
    async fn stop_reaches_the_process_group_but_not_a_detached_daemon() {
        let manager = ProcessManager::new();
        let mut script = Script::spawn(
            &manager,
            "trap '' TERM
             set -m; sleep 30 & echo daemon $!; set +m
             sleep 30 & echo helper $!
             wait",
        )
        .await;
        let daemon = script.pid("daemon").await;
        let helper = script.pid("helper").await;

        let report = manager.shutdown_all(Duration::from_millis(300)).await;
        assert_eq!(report.killed, vec![script.id]);

        assert!(wait_until_gone(helper).await, "the helper must be killed");
        assert!(alive(daemon), "the detached daemon must survive");
        // SAFETY: test cleanup of the descendant this test started.
        unsafe { libc::kill(daemon, libc::SIGKILL) };
    }

    #[tokio::test]
    async fn no_process_starts_after_shutdown() {
        let manager = ProcessManager::new();
        assert_eq!(
            manager.shutdown_all(Duration::from_millis(100)).await,
            ShutdownReport::default()
        );
        let err = spawn(
            &manager.0,
            "echo",
            &["late"],
            std::env::temp_dir(),
            vec![],
            SpawnOptions {
                on_line: Box::new(|_| {}),
                on_exit: Box::new(|_, _| {}),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err, "Keynobi is shutting down");
    }
}
