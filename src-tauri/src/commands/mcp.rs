pub use crate::services::mcp_server::start_mcp_server;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Status of the MCP integration with Claude Code.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpSetupStatus {
    /// The real absolute path to this application's binary.
    /// Use this to build the `claude mcp add` command.
    pub exe_path: String,
    /// The full `--command` string ready to paste into a terminal.
    pub setup_command: String,
    /// Whether the `claude` CLI was found (via PATH or login shell).
    pub claude_found: bool,
    /// Whether `android-companion` is already registered in Claude Code.
    pub is_configured: bool,
    /// The command that is currently registered (if any).
    pub configured_command: Option<String>,
}

/// Query everything needed to set up or verify the MCP integration.
///
/// Returns the real binary path, whether Claude Code CLI is installed,
/// and whether the MCP server is already registered.
#[tauri::command]
pub async fn get_mcp_setup_status() -> Result<McpSetupStatus, String> {
    // ── 1. Resolve the real binary path ──────────────────────────────────────
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "android-dev-companion".to_string());

    let setup_command = format!("{} --mcp", exe_path);

    // ── 2. Find the `claude` binary ──────────────────────────────────────────
    // GUI apps on macOS do not inherit the user's shell PATH, so we try
    // via a login shell if the simple PATH lookup fails.
    let claude_bin = find_claude_binary().await;

    // ── 3. Check if already configured ───────────────────────────────────────
    let (is_configured, configured_command) = if let Some(ref claude) = claude_bin {
        check_mcp_configured(claude).await
    } else {
        (false, None)
    };

    Ok(McpSetupStatus {
        exe_path,
        setup_command,
        claude_found: claude_bin.is_some(),
        is_configured,
        configured_command,
    })
}

/// Run `claude mcp add android-companion --command "<exe_path> --mcp"`.
///
/// Returns a human-readable success/error message.
#[tauri::command]
pub async fn configure_mcp_in_claude() -> Result<String, String> {
    let exe_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| format!("Could not determine app path: {e}"))?;

    let claude = find_claude_binary()
        .await
        .ok_or_else(|| "Claude Code CLI not found. Install Claude Code from https://claude.ai/download".to_string())?;

    let command_arg = format!("{} --mcp", exe_path);

    // Remove any existing entry first so re-configuring works cleanly.
    let _ = tokio::process::Command::new(&claude)
        .args(["mcp", "remove", "android-companion"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new(&claude)
            .args(["mcp", "add", "android-companion", "--command", &command_arg])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| "Timed out waiting for claude mcp add".to_string())?
    .map_err(|e| format!("Failed to run claude: {e}"))?;

    if output.status.success() {
        Ok(format!(
            "MCP server registered successfully!\nCommand: claude mcp add android-companion --command \"{}\"",
            command_arg
        ))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        Err(format!("claude mcp add failed: {}", detail))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find the `claude` binary, trying PATH first, then the login shell.
/// Returns the path if found, `None` if Claude Code CLI is not installed.
async fn find_claude_binary() -> Option<String> {
    // Fast path: `claude` is on the process PATH (works when launched from terminal).
    if is_executable_on_path("claude") {
        return Some("claude".to_string());
    }

    // Check common installation locations directly.
    let common_paths = [
        // npm global installs / claude code locations
        dirs::home_dir().map(|h| h.join(".claude").join("local").join("claude")),
        dirs::home_dir().map(|h| h.join(".nvm").join("versions")),
        // /usr/local/bin, /opt/homebrew/bin — often present even without shell
        Some(std::path::PathBuf::from("/usr/local/bin/claude")),
        Some(std::path::PathBuf::from("/opt/homebrew/bin/claude")),
        Some(std::path::PathBuf::from("/usr/bin/claude")),
    ];
    for path_opt in common_paths.into_iter().flatten() {
        if path_opt.is_file() {
            return Some(path_opt.to_string_lossy().to_string());
        }
    }

    // Slow path: spawn a login shell to inherit the user's full environment.
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            tokio::process::Command::new(&shell)
                .args(["-l", "-c", "which claude"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output(),
        )
        .await;

        if let Ok(Ok(out)) = result {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).is_file() {
                return Some(path);
            }
        }
    }

    None
}

/// Check whether `android-companion` is already registered in Claude Code.
/// Returns `(is_configured, configured_command_if_any)`.
async fn check_mcp_configured(claude: &str) -> (bool, Option<String>) {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(claude)
            .args(["mcp", "get", "android-companion"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Extract the command from the output if possible
            let cmd = stdout.lines()
                .find(|l| l.contains("--mcp") || l.contains("android-dev-companion") || l.contains("Command:"))
                .map(|l| l.trim().trim_start_matches("Command:").trim().to_string());
            (true, cmd.or(Some(stdout)))
        }
        _ => (false, None),
    }
}

fn is_executable_on_path(name: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| {
            let p = std::path::Path::new(dir).join(name);
            p.is_file()
        })
}
