pub use crate::services::mcp_server::start_mcp_server;

use crate::services::mcp_activity;
pub use crate::services::mcp_activity::{McpActivityEntry, McpServerStatus};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Status of the MCP integration with one AI client.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpClientSetupStatus {
    /// Whether the client CLI was found (via PATH, common install paths, or login shell).
    pub client_found: bool,
    /// Whether `keynobi` is already registered in this MCP client.
    pub is_configured: bool,
    /// The command that is currently registered (if any).
    pub configured_command: Option<String>,
    /// Full setup command the user can copy into a terminal.
    pub setup_command: String,
}

/// Status of the MCP integration with supported AI clients.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpSetupStatus {
    /// The real absolute path to this application's binary.
    pub exe_path: String,
    /// The binary path with `--mcp` flag.
    pub setup_command: String,
    /// Claude Code setup and configuration status.
    pub claude: McpClientSetupStatus,
    /// Codex setup and configuration status.
    pub codex: McpClientSetupStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct McpSetupCommands {
    server_command: String,
    claude_setup_command: String,
    codex_setup_command: String,
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
        .unwrap_or_else(|_| "keynobi".to_string());

    let commands = build_mcp_setup_commands(&exe_path);

    // ── 2. Find supported MCP client CLIs ────────────────────────────────────
    // GUI apps on macOS do not inherit the user's shell PATH, so we try
    // via a login shell if the simple PATH lookup fails.
    let claude_bin = find_client_binary("claude").await;
    let codex_bin = find_client_binary("codex").await;

    // ── 3. Check if already configured ───────────────────────────────────────
    let (claude_configured, claude_command) = if let Some(ref claude) = claude_bin {
        check_claude_mcp_configured(claude).await
    } else {
        (false, None)
    };

    let (codex_configured, codex_command) = if let Some(ref codex) = codex_bin {
        check_codex_mcp_configured(codex).await
    } else {
        (false, None)
    };

    Ok(McpSetupStatus {
        exe_path,
        setup_command: commands.server_command,
        claude: McpClientSetupStatus {
            client_found: claude_bin.is_some(),
            is_configured: claude_configured,
            configured_command: claude_command,
            setup_command: commands.claude_setup_command,
        },
        codex: McpClientSetupStatus {
            client_found: codex_bin.is_some(),
            is_configured: codex_configured,
            configured_command: codex_command,
            setup_command: commands.codex_setup_command,
        },
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_mcp_setup_commands(exe_path: &str) -> McpSetupCommands {
    let quoted_exe = double_quote_arg(exe_path);
    McpSetupCommands {
        server_command: format!("{exe_path} --mcp"),
        claude_setup_command: format!(
            "claude mcp add --transport stdio keynobi -- {quoted_exe} --mcp"
        ),
        codex_setup_command: format!("codex mcp add keynobi -- {quoted_exe} --mcp"),
    }
}

fn double_quote_arg(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn shell_quote_arg(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'))
    {
        return value.to_string();
    }

    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Find an MCP client binary, trying PATH, common install paths, then the login shell.
async fn find_client_binary(name: &str) -> Option<String> {
    // Fast path: the CLI is on the process PATH (works when launched from terminal).
    if is_executable_on_path(name) {
        return Some(name.to_string());
    }

    // Check common installation locations directly.
    for path in common_client_paths(name) {
        if path.is_file() {
            return Some(path.to_string_lossy().to_string());
        }
    }

    // Slow path: spawn a login shell to inherit the user's full environment.
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(4),
            tokio::process::Command::new(&shell)
                .args(["-l", "-c", &format!("command -v {name}")])
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

fn common_client_paths(name: &str) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local").join("bin").join(name));
        if name == "claude" {
            paths.push(home.join(".claude").join("local").join("claude"));
        }
        if name == "codex" {
            paths.push(home.join(".codex").join("bin").join("codex"));
            paths.push(home.join(".codex").join("local").join("codex"));
        }
    }
    paths.push(std::path::PathBuf::from(format!("/usr/local/bin/{name}")));
    paths.push(std::path::PathBuf::from(format!(
        "/opt/homebrew/bin/{name}"
    )));
    paths.push(std::path::PathBuf::from(format!("/usr/bin/{name}")));
    paths
}

/// Check whether `keynobi` is already registered in Claude Code.
/// Returns `(is_configured, configured_command_if_any)`.
async fn check_claude_mcp_configured(claude: &str) -> (bool, Option<String>) {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(claude)
            .args(["mcp", "get", "keynobi"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let cmd = extract_configured_command(&stdout);
            (true, cmd.or(Some(stdout)))
        }
        _ => (false, None),
    }
}

/// Check whether `keynobi` is already registered in Codex.
/// Returns `(is_configured, configured_command_if_any)`.
async fn check_codex_mcp_configured(codex: &str) -> (bool, Option<String>) {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(codex)
            .args(["mcp", "get", "keynobi", "--json"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let cmd = extract_configured_command(&stdout);
            (true, cmd.or(Some(stdout)))
        }
        _ => (false, None),
    }
}

fn extract_configured_command(stdout: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(command) = value.get("command").and_then(|v| v.as_str()) {
            let mut parts = vec![shell_quote_arg(command)];
            if let Some(args) = value.get("args").and_then(|v| v.as_array()) {
                parts.extend(
                    args.iter()
                        .filter_map(|arg| arg.as_str().map(shell_quote_arg)),
                );
            }
            return Some(parts.join(" "));
        }
    }

    stdout
        .lines()
        .find(|line| {
            line.contains("--mcp") || line.contains("keynobi") || line.contains("Command:")
        })
        .map(|line| {
            line.trim()
                .trim_start_matches("Command:")
                .trim()
                .to_string()
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_manual_setup_commands_for_claude_and_codex() {
        let commands = build_mcp_setup_commands("/Applications/Keynobi.app/Contents/MacOS/keynobi");

        assert_eq!(
            commands.server_command,
            "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp"
        );
        assert_eq!(
            commands.claude_setup_command,
            "claude mcp add --transport stdio keynobi -- \"/Applications/Keynobi.app/Contents/MacOS/keynobi\" --mcp"
        );
        assert_eq!(
            commands.codex_setup_command,
            "codex mcp add keynobi -- \"/Applications/Keynobi.app/Contents/MacOS/keynobi\" --mcp"
        );
    }

    #[test]
    fn mcp_setup_status_serializes_per_client_fields() {
        let status = McpSetupStatus {
            exe_path: "/mock/keynobi".into(),
            setup_command: "/mock/keynobi --mcp".into(),
            claude: McpClientSetupStatus {
                client_found: true,
                is_configured: true,
                configured_command: Some("/mock/keynobi --mcp".into()),
                setup_command:
                    "claude mcp add --transport stdio keynobi -- \"/mock/keynobi\" --mcp".into(),
            },
            codex: McpClientSetupStatus {
                client_found: false,
                is_configured: false,
                configured_command: None,
                setup_command: "codex mcp add keynobi -- \"/mock/keynobi\" --mcp".into(),
            },
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"claude\""));
        assert!(json.contains("\"codex\""));
        assert!(json.contains("\"clientFound\""));
    }
}

// ── Activity log commands ─────────────────────────────────────────────────────

/// Return the last `limit` MCP activity entries (default 200, max 2000).
#[tauri::command]
pub async fn get_mcp_activity(limit: Option<u32>) -> Result<Vec<McpActivityEntry>, String> {
    let n = limit.unwrap_or(200).min(2000) as usize;
    tokio::task::spawn_blocking(move || mcp_activity::read_activity(n))
        .await
        .map_err(|e| format!("Failed to read MCP activity log: {e}"))
}

/// Return the live status of the MCP server: whether a headless process is alive and its PID.
#[tauri::command]
pub async fn get_mcp_server_status() -> Result<McpServerStatus, String> {
    let status = tokio::task::spawn_blocking(|| McpServerStatus {
        alive: mcp_activity::is_mcp_server_alive(),
        pid: mcp_activity::read_pid_file(),
    })
    .await
    .map_err(|e| format!("Failed to check MCP server status: {e}"))?;
    Ok(status)
}

/// Clear the MCP activity log.
#[tauri::command]
pub async fn clear_mcp_activity() -> Result<(), String> {
    tokio::task::spawn_blocking(mcp_activity::clear_activity_log)
        .await
        .map_err(|e| format!("Failed to clear MCP activity log: {e}"))
}
