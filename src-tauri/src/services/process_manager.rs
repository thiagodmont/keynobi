use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

static NEXT_PROCESS_ID: AtomicU32 = AtomicU32::new(1);

/// Opaque handle uniquely identifying a managed process.
pub type ProcessId = u32;

/// Outcome of a line read from a spawned process's output streams.
#[derive(Debug, Clone)]
pub struct ProcessLine {
    pub pid: ProcessId,
    pub is_stderr: bool,
    pub text: String,
}

/// Internal tracking record for a running process.
struct ProcessRecord {
    /// OS-level PID for sending signals. Captured before child is moved into reader task.
    os_pid: Option<u32>,
    /// Background task that streams output lines and waits for process exit.
    _reader_task: JoinHandle<()>,
}

/// Per-process callbacks dispatched from the reader task.
pub struct SpawnOptions {
    /// Called with each output line (stdout or stderr), on a tokio task.
    pub on_line: Box<dyn Fn(ProcessLine) + Send + Sync + 'static>,
    /// Called once when the process exits.
    pub on_exit: Box<dyn Fn(ProcessId, Option<i32>) + Send + Sync + 'static>,
}

pub struct ProcessManagerInner {
    processes: HashMap<ProcessId, ProcessRecord>,
}

impl ProcessManagerInner {
    pub fn new() -> Self {
        Self { processes: HashMap::new() }
    }
}

pub struct ProcessManager(pub Mutex<ProcessManagerInner>);

impl ProcessManager {
    pub fn new() -> Self {
        ProcessManager(Mutex::new(ProcessManagerInner::new()))
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn a child process and stream its stdout/stderr line-by-line.
///
/// Returns a `ProcessId` that can be used to cancel the process.
/// The `options.on_line` callback is called from a dedicated tokio task for
/// every line produced by stdout or stderr. `options.on_exit` is called once
/// when the process terminates.
///
/// # Errors
/// Returns an error string if the process fails to start.
pub async fn spawn(
    manager: &Mutex<ProcessManagerInner>,
    cmd: &str,
    args: &[&str],
    cwd: PathBuf,
    env_extra: Vec<(String, String)>,
    options: SpawnOptions,
) -> Result<ProcessId, String> {
    let id = NEXT_PROCESS_ID.fetch_add(1, Ordering::SeqCst);

    let mut command = Command::new(cmd);
    command
        .args(args)
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // Inherit base environment and add extras.
        .envs(env_extra);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn '{cmd}': {e}"))?;

    let stdout = child.stdout.take().ok_or("No stdout")?;
    let stderr = child.stderr.take().ok_or("No stderr")?;
    // Capture OS PID before we move child into the reader task.
    let os_pid = child.id();

    let on_line = std::sync::Arc::new(options.on_line);
    let on_exit = std::sync::Arc::new(options.on_exit);

    // Spawn the reader task that merges stdout and stderr.
    // We move `child` into this task so we can call child.wait() after both
    // streams are fully drained — capturing the real exit code.
    let reader_task = {
        let on_line = on_line.clone();
        let on_exit = on_exit.clone();

        tokio::spawn(async move {
            let stdout_reader = BufReader::new(stdout);
            let stderr_reader = BufReader::new(stderr);

            let mut stdout_lines = stdout_reader.lines();
            let mut stderr_lines = stderr_reader.lines();
            let mut stdout_done = false;
            let mut stderr_done = false;

            // Read both streams concurrently until both are fully drained.
            // When one stream closes we keep reading the other so no output is lost.
            loop {
                if stdout_done && stderr_done {
                    break;
                }
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
                }
            }

            // Both streams are exhausted — wait for the process to exit and
            // capture its exit code so on_exit can report success/failure correctly.
            let exit_code = child.wait().await.ok().and_then(|s| s.code());
            on_exit(id, exit_code);
        })
    };

    let record = ProcessRecord {
        os_pid,
        _reader_task: reader_task,
    };

    let mut inner = manager.lock().await;
    // Enforce max 10 concurrent processes (bounded collection rule).
    if inner.processes.len() >= 10 {
        return Err("Maximum concurrent processes (10) reached".into());
    }
    inner.processes.insert(id, record);

    Ok(id)
}

/// Send SIGTERM to the process, then SIGKILL after 5 seconds if still running.
///
/// No-op if the process ID is unknown (already exited or never created).
pub async fn cancel(manager: &Mutex<ProcessManagerInner>, id: ProcessId) {
    let mut inner = manager.lock().await;
    if let Some(record) = inner.processes.remove(&id) {
        if let Some(os_pid) = record.os_pid {
            // Try graceful termination first.
            #[cfg(unix)]
            unsafe { libc::kill(os_pid as libc::pid_t, libc::SIGTERM) };

            // Spawn a background task to force-kill if it doesn't die in 5s.
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                #[cfg(unix)]
                unsafe { libc::kill(os_pid as libc::pid_t, libc::SIGKILL) };
            });
        }
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
                on_exit: Box::new(move |_, _| {
                    exited_clone.store(true, Ordering::SeqCst);
                }),
            },
        )
        .await
        .unwrap();

        // Give the reader task a moment to drain.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        remove(&manager.0, id).await;

        let collected = lines.lock().unwrap();
        assert!(collected.iter().any(|l| l.contains("hello world")));
    }

    #[tokio::test]
    async fn cancel_unknown_id_is_noop() {
        let manager = ProcessManager::new();
        // Should not panic.
        cancel(&manager.0, 99999).await;
    }
}
