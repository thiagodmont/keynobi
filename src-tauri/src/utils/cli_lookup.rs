//! Find a command-line tool the way the user's terminal would.
//!
//! A GUI app on macOS does not inherit the `PATH` the user sets in
//! `.zprofile` or `.zshrc`, so a tool the terminal finds may be missing from
//! the app's own `PATH`. [`CliSearch`] tries the process `PATH`, then known
//! install locations, then asks a login shell.

use crate::utils::process::{output_with_timeout, LOGIN_SHELL_TIMEOUT};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

/// Where to look for one tool.
#[derive(Debug, Clone)]
pub struct CliSearch {
    /// The command name, such as `claude` or `android`.
    pub name: String,
    /// A `PATH`-style list of directories to search first.
    pub path_var: String,
    /// Files to try next, in order.
    pub candidates: Vec<PathBuf>,
    /// The shell asked with `-l -c 'command -v <name>'`, or `None` to skip it.
    pub login_shell: Option<PathBuf>,
    /// How long the login shell may take.
    pub shell_timeout: Duration,
}

impl CliSearch {
    /// Search the process `PATH`, then `extra` and the usual install
    /// locations (`~/.local/bin`, `/usr/local/bin`, `/opt/homebrew/bin`,
    /// `/usr/bin`), then the user's login shell (`$SHELL`, else `/bin/zsh`).
    pub fn system(name: &str, extra: Vec<PathBuf>) -> Self {
        let mut candidates = Vec::new();
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".local").join("bin").join(name));
        }
        candidates.extend(extra);
        for dir in ["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"] {
            candidates.push(Path::new(dir).join(name));
        }
        Self {
            name: name.to_string(),
            path_var: std::env::var("PATH").unwrap_or_default(),
            candidates,
            login_shell: Some(PathBuf::from(
                std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into()),
            )),
            shell_timeout: LOGIN_SHELL_TIMEOUT,
        }
    }

    /// The first match, as found (symlinks are not resolved).
    pub async fn find(&self) -> Option<PathBuf> {
        if let Some(path) = find_on_path(&self.name, &self.path_var) {
            return Some(path);
        }
        if let Some(path) = self.candidates.iter().find(|p| p.is_file()) {
            return Some(path.clone());
        }
        match &self.login_shell {
            Some(shell) => ask_login_shell(shell, &self.name, self.shell_timeout).await,
            None => None,
        }
    }
}

/// The first file named `name` in the directories of `path_var`.
pub fn find_on_path(name: &str, path_var: &str) -> Option<PathBuf> {
    path_var
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(name))
        .find(|p| p.is_file())
}

/// `command -v <name>` in a login shell, when it prints an absolute path to a
/// file. Profiles may print other lines first, so only the last one counts.
async fn ask_login_shell(shell: &Path, name: &str, timeout: Duration) -> Option<PathBuf> {
    // The name is pasted into a shell script, so only plain names are asked.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return None;
    }
    let out = output_with_timeout(
        tokio::process::Command::new(shell)
            .args(["-l", "-c", &format!("command -v {name}")])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
        timeout,
    )
    .await
    .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().rev().find(|l| !l.trim().is_empty())?.trim();
    let path = PathBuf::from(line);
    (path.is_absolute() && path.is_file()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::process::test_support::run_once;
    use std::os::unix::fs::PermissionsExt;

    fn script(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn search(name: &str, path_var: String) -> CliSearch {
        CliSearch {
            name: name.into(),
            path_var,
            candidates: Vec::new(),
            login_shell: None,
            shell_timeout: Duration::from_secs(30),
        }
    }

    #[tokio::test]
    async fn path_comes_before_known_locations() {
        let dir = tempfile::tempdir().unwrap();
        let on_path = dir.path().join("bin").join("tool");
        let known = dir.path().join("known").join("tool");
        script(&on_path, "true");
        script(&known, "true");

        let mut s = search(
            "tool",
            format!("/nonexistent:{}", on_path.parent().unwrap().display()),
        );
        s.candidates = vec![known.clone()];
        assert_eq!(s.find().await, Some(on_path));

        s.path_var = String::new();
        assert_eq!(s.find().await, Some(known));
    }

    #[tokio::test]
    async fn the_login_shell_is_asked_last_and_only_its_last_line_counts() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("elsewhere").join("tool");
        script(&tool, "true");
        let shell = dir.path().join("shell");
        script(
            &shell,
            &format!(
                "printf '%s\\n' \"$*\" > '{}'\necho 'Welcome banner'\necho '{}'",
                dir.path().join("args.txt").display(),
                tool.display()
            ),
        );
        run_once(&shell);

        let mut s = search("tool", String::new());
        s.login_shell = Some(shell);
        assert_eq!(s.find().await, Some(tool));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("args.txt")).unwrap(),
            "-l -c command -v tool\n"
        );
    }

    #[tokio::test]
    async fn a_shell_answer_that_is_not_a_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("shell");
        script(&shell, "echo 'alias tool=/nowhere/tool'");
        run_once(&shell);

        let mut s = search("tool", String::new());
        s.login_shell = Some(shell.clone());
        assert_eq!(s.find().await, None);

        s.name = "tool; touch pwned".into();
        assert_eq!(s.find().await, None);
    }

    #[tokio::test]
    async fn a_hung_login_shell_gives_up_at_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("shell");
        script(&shell, "[ \"$1\" = -l ] && exec sleep 30\ntrue");
        run_once(&shell);

        let mut s = search("tool", String::new());
        s.login_shell = Some(shell);
        s.shell_timeout = Duration::from_millis(300);
        let start = std::time::Instant::now();
        assert_eq!(s.find().await, None);
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
