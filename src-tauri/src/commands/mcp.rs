use crate::models::error::AppError;
use crate::services::agent_skill;
pub use crate::services::agent_skill::AgentSkillStatus;
use crate::services::app_location;
use crate::services::mcp_activity;
pub use crate::services::mcp_activity::McpActivityEntry;
pub use crate::services::mcp_sessions::McpServerStatus;
use crate::services::mcp_sessions::{self, McpSessionRegistry};
use crate::utils::cli_lookup::CliSearch;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

/// Registration checks run here, so that a client's registrations for a
/// particular folder (Claude Code's `local` and `project` scopes) are not
/// mistaken for one that works everywhere.
const REGISTRATION_CHECK_DIR: &str = "/";
const REGISTRATION_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Status of the MCP integration with one AI client.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpClientSetupStatus {
    /// Whether the client CLI was found (via PATH, common install paths, or login shell).
    pub client_found: bool,
    /// Whether `keynobi` is registered in this MCP client for every folder.
    pub is_configured: bool,
    /// The command that is currently registered (if any).
    pub configured_command: Option<String>,
    /// Scope of the registration found (`user`, `local`, `project`, …), when
    /// the client reports one. A `local` or `project` registration only works
    /// in one folder, so it does not count as configured.
    pub configured_scope: Option<String>,
    /// Full setup command the user can copy into a terminal, or `None` when
    /// the app's location must not be registered (see `location_problem`).
    pub setup_command: Option<String>,
}

/// Status of the MCP integration with supported AI clients.
#[derive(Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct McpSetupStatus {
    /// The real absolute path to this application's binary.
    pub exe_path: String,
    /// The binary path with `--mcp` flag, or `None` when `location_problem` is set.
    pub setup_command: Option<String>,
    /// Why the app's path must not be registered (it runs from a disk image
    /// or a temporary App Translocation copy), phrased for the user.
    pub location_problem: Option<String>,
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

/// What an MCP client reports about its `keynobi` registration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Registration {
    configured: bool,
    command: Option<String>,
    scope: Option<String>,
}

/// Query everything needed to set up or verify the MCP integration.
///
/// Returns the real binary path, whether Claude Code CLI is installed,
/// and whether the MCP server is already registered.
#[tauri::command]
pub async fn get_mcp_setup_status() -> Result<McpSetupStatus, String> {
    // ── 1. Resolve the real binary path ──────────────────────────────────────
    let exe = std::env::current_exe().ok();
    let exe_path = exe
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "keynobi".to_string());
    let location_problem = exe
        .as_deref()
        .and_then(app_location::temporary_location_reason);

    // ── 2. Find supported MCP client CLIs ────────────────────────────────────
    // GUI apps on macOS do not inherit the user's shell PATH, so we try
    // via a login shell if the simple PATH lookup fails.
    let claude_bin = find_client_binary("claude").await;
    let codex_bin = find_client_binary("codex").await;

    // ── 3. Check if already configured ───────────────────────────────────────
    Ok(setup_status(
        exe_path,
        location_problem,
        claude_bin.as_deref(),
        codex_bin.as_deref(),
    )
    .await)
}

async fn setup_status(
    exe_path: String,
    location_problem: Option<String>,
    claude_bin: Option<&str>,
    codex_bin: Option<&str>,
) -> McpSetupStatus {
    let commands = match location_problem {
        None => Some(build_mcp_setup_commands(&exe_path)),
        Some(_) => None,
    };
    let claude = match claude_bin {
        Some(claude) => check_claude_mcp_configured(claude).await,
        None => Registration::default(),
    };
    let codex = match codex_bin {
        Some(codex) => check_codex_mcp_configured(codex).await,
        None => Registration::default(),
    };

    McpSetupStatus {
        exe_path,
        setup_command: commands.as_ref().map(|c| c.server_command.clone()),
        location_problem,
        claude: McpClientSetupStatus {
            client_found: claude_bin.is_some(),
            is_configured: claude.configured,
            configured_command: claude.command,
            configured_scope: claude.scope,
            setup_command: commands.as_ref().map(|c| c.claude_setup_command.clone()),
        },
        codex: McpClientSetupStatus {
            client_found: codex_bin.is_some(),
            is_configured: codex.configured,
            configured_command: codex.command,
            configured_scope: codex.scope,
            setup_command: commands.map(|c| c.codex_setup_command),
        },
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Claude Code is registered at user scope so Keynobi works in every folder.
/// Codex has no scopes: `codex mcp add` always writes the user's own config.
fn build_mcp_setup_commands(exe_path: &str) -> McpSetupCommands {
    let quoted_exe = single_quote_arg(exe_path);
    McpSetupCommands {
        server_command: format!("{quoted_exe} --mcp"),
        claude_setup_command: format!(
            "claude mcp add --scope user --transport stdio keynobi -- {quoted_exe} --mcp"
        ),
        codex_setup_command: format!("codex mcp add keynobi -- {quoted_exe} --mcp"),
    }
}

fn single_quote_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_quote_arg(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'))
    {
        return value.to_string();
    }

    single_quote_arg(value)
}

/// Find an MCP client binary, trying PATH, common install paths, then the login shell.
async fn find_client_binary(name: &str) -> Option<String> {
    let mut extra = Vec::new();
    if let Some(home) = dirs::home_dir() {
        if name == "claude" {
            extra.push(home.join(".claude").join("local").join("claude"));
        }
        if name == "codex" {
            extra.push(home.join(".codex").join("bin").join("codex"));
            extra.push(home.join(".codex").join("local").join("codex"));
        }
    }
    CliSearch::system(name, extra)
        .find()
        .await
        .map(|path| path.to_string_lossy().to_string())
}

/// Run a client's registration query from [`REGISTRATION_CHECK_DIR`];
/// `Some(stdout)` when it exits successfully (the server is registered).
async fn query_registration(client: &str, args: &[&str]) -> Option<String> {
    let result = tokio::time::timeout(
        REGISTRATION_CHECK_TIMEOUT,
        tokio::process::Command::new(client)
            .args(args)
            .current_dir(REGISTRATION_CHECK_DIR)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) if out.status.success() => {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        _ => None,
    }
}

/// Check whether `keynobi` is registered in Claude Code for every folder.
async fn check_claude_mcp_configured(claude: &str) -> Registration {
    match query_registration(claude, &["mcp", "get", "keynobi"]).await {
        Some(stdout) => parse_claude_mcp_get(&stdout),
        None => Registration::default(),
    }
}

/// Read `claude mcp get` output: a `Scope:` line naming the config the entry
/// came from, and `Command:`/`Args:` lines for stdio servers.
fn parse_claude_mcp_get(stdout: &str) -> Registration {
    let field = |name: &str| {
        stdout.lines().find_map(|line| {
            line.trim()
                .strip_prefix(name)
                .and_then(|rest| rest.strip_prefix(':'))
                .map(|value| value.trim().to_string())
        })
    };
    let scope = field("Scope").and_then(|s| {
        s.split_whitespace()
            .next()
            .map(|word| word.to_ascii_lowercase())
    });
    let command = field("Command").map(|cmd| {
        let mut parts = vec![shell_quote_arg(&cmd)];
        if let Some(args) = field("Args").filter(|a| !a.is_empty()) {
            parts.push(args);
        }
        parts.join(" ")
    });
    let folder_only = matches!(scope.as_deref(), Some("local" | "project"));
    Registration {
        configured: !folder_only,
        command: command.or_else(|| Some(stdout.to_string())),
        scope,
    }
}

/// Check whether `keynobi` is registered in Codex.
async fn check_codex_mcp_configured(codex: &str) -> Registration {
    match query_registration(codex, &["mcp", "get", "keynobi", "--json"]).await {
        Some(stdout) => Registration {
            configured: true,
            command: extract_configured_command(&stdout).or(Some(stdout)),
            scope: None,
        },
        None => Registration::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::cli_lookup::find_on_path;
    use std::path::Path;

    const INSTALLED_EXE: &str = "/Applications/Keynobi.app/Contents/MacOS/keynobi";

    #[test]
    fn builds_manual_setup_commands_for_claude_and_codex() {
        let commands = build_mcp_setup_commands(INSTALLED_EXE);

        assert_eq!(
            commands.server_command,
            "'/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp"
        );
        assert_eq!(
            commands.claude_setup_command,
            "claude mcp add --scope user --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp"
        );
        assert_eq!(
            commands.codex_setup_command,
            "codex mcp add keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp"
        );
    }

    #[test]
    fn setup_commands_shell_quote_expansion_characters() {
        let commands = build_mcp_setup_commands("/tmp/Key $HOME `touch bad` 'App'/keynobi");

        assert_eq!(
            commands.server_command,
            "'/tmp/Key $HOME `touch bad` '\"'\"'App'\"'\"'/keynobi' --mcp"
        );
        assert_eq!(
            commands.claude_setup_command,
            "claude mcp add --scope user --transport stdio keynobi -- '/tmp/Key $HOME `touch bad` '\"'\"'App'\"'\"'/keynobi' --mcp"
        );
        assert_eq!(
            commands.codex_setup_command,
            "codex mcp add keynobi -- '/tmp/Key $HOME `touch bad` '\"'\"'App'\"'\"'/keynobi' --mcp"
        );
    }

    #[test]
    fn configured_command_shell_quotes_expansion_characters() {
        let stdout = r#"{
            "command": "/tmp/Key $HOME `touch bad` 'App'/keynobi",
            "args": ["--mcp", "--project", "/tmp/project $(bad)"]
        }"#;

        assert_eq!(
            extract_configured_command(stdout),
            Some(
                "'/tmp/Key $HOME `touch bad` '\"'\"'App'\"'\"'/keynobi' --mcp --project '/tmp/project $(bad)'"
                    .to_string()
            )
        );
    }

    fn claude_get_output(scope_line: &str) -> String {
        format!(
            "keynobi:\n  Scope: {scope_line}\n  Status: ✓ Connected\n  Type: stdio\n  \
             Command: /Applications/Keynobi.app/Contents/MacOS/keynobi\n  Args: --mcp\n\n\
             To remove this server, run: claude mcp remove \"keynobi\" -s user\n"
        )
    }

    #[test]
    fn a_user_scope_claude_registration_is_configured() {
        let found = parse_claude_mcp_get(&claude_get_output(
            "User config (available in all your projects)",
        ));
        assert_eq!(
            found,
            Registration {
                configured: true,
                command: Some(format!("{INSTALLED_EXE} --mcp")),
                scope: Some("user".into()),
            }
        );
    }

    #[test]
    fn a_folder_scope_claude_registration_is_not_configured() {
        for (line, scope) in [
            ("Local config (private to you in this project)", "local"),
            ("Project config (shared via .mcp.json)", "project"),
        ] {
            let found = parse_claude_mcp_get(&claude_get_output(line));
            assert!(!found.configured, "{scope}");
            assert_eq!(found.scope.as_deref(), Some(scope));
            assert!(found.command.is_some());
        }
    }

    #[test]
    fn claude_output_without_a_scope_line_counts_as_configured() {
        let found = parse_claude_mcp_get("keynobi:\n  Command: keynobi\n  Args: --mcp");
        assert!(found.configured);
        assert_eq!(found.scope, None);
        assert_eq!(found.command.as_deref(), Some("keynobi --mcp"));
    }

    /// A fake client CLI in a temp dir: records its arguments and working
    /// directory, prints `stdout`, and exits with `status`.
    fn fake_client(dir: &Path, name: &str, stdout: &str, status: i32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(dir.join("stdout.txt"), stdout).unwrap();
        let script = dir.join(name);
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nd=\"$(dirname \"$0\")\"\npwd -P > \"$d/cwd.txt\"\n\
                 printf '%s\\n' \"$@\" > \"$d/args.txt\"\ncat \"$d/stdout.txt\"\nexit {status}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Registration checks give the client a few seconds.
        crate::utils::process::test_support::run_once(&script);
        let _ = std::fs::remove_file(dir.join("cwd.txt"));
        let _ = std::fs::remove_file(dir.join("args.txt"));
        script
    }

    fn recorded(dir: &Path, file: &str) -> String {
        std::fs::read_to_string(dir.join(file)).unwrap_or_default()
    }

    #[tokio::test]
    async fn claude_detection_asks_outside_any_project_and_needs_user_scope() {
        let dir = tempfile::tempdir().unwrap();
        fake_client(
            dir.path(),
            "claude",
            &claude_get_output("User config (available in all your projects)"),
            0,
        );
        let path_var = format!("/nonexistent-dir:{}", dir.path().display());
        let claude = find_on_path("claude", &path_var).expect("fake claude is on PATH");

        let status = setup_status(
            INSTALLED_EXE.into(),
            None,
            Some(&claude.to_string_lossy()),
            None,
        )
        .await;

        assert_eq!(recorded(dir.path(), "args.txt"), "mcp\nget\nkeynobi\n");
        assert_eq!(
            recorded(dir.path(), "cwd.txt").trim(),
            REGISTRATION_CHECK_DIR
        );
        assert!(status.claude.client_found);
        assert!(status.claude.is_configured);
        assert_eq!(status.claude.configured_scope.as_deref(), Some("user"));
        assert_eq!(
            status.claude.setup_command.as_deref(),
            Some("claude mcp add --scope user --transport stdio keynobi -- '/Applications/Keynobi.app/Contents/MacOS/keynobi' --mcp")
        );
        assert!(!status.codex.client_found);

        std::fs::write(
            dir.path().join("stdout.txt"),
            claude_get_output("Local config (private to you in this project)"),
        )
        .unwrap();
        let status = setup_status(
            INSTALLED_EXE.into(),
            None,
            Some(&claude.to_string_lossy()),
            None,
        )
        .await;
        assert!(!status.claude.is_configured, "a local registration counted");
        assert_eq!(status.claude.configured_scope.as_deref(), Some("local"));
    }

    #[tokio::test]
    async fn an_unregistered_client_is_not_configured() {
        let dir = tempfile::tempdir().unwrap();
        let codex = fake_client(dir.path(), "codex", "No MCP server named 'keynobi'", 1);

        let status = setup_status(
            INSTALLED_EXE.into(),
            None,
            None,
            Some(&codex.to_string_lossy()),
        )
        .await;

        assert_eq!(
            recorded(dir.path(), "args.txt"),
            "mcp\nget\nkeynobi\n--json\n"
        );
        assert_eq!(
            recorded(dir.path(), "cwd.txt").trim(),
            REGISTRATION_CHECK_DIR
        );
        assert!(status.codex.client_found);
        assert!(!status.codex.is_configured);
        assert_eq!(status.codex.configured_command, None);
    }

    #[tokio::test]
    async fn a_temporary_location_offers_no_setup_commands() {
        let exe = "/Volumes/Keynobi/Keynobi.app/Contents/MacOS/keynobi";
        let problem = app_location::temporary_location_reason(Path::new(exe));
        assert!(problem.is_some());

        let status = setup_status(exe.into(), problem.clone(), None, None).await;

        assert_eq!(status.location_problem, problem);
        assert_eq!(status.setup_command, None);
        assert_eq!(status.claude.setup_command, None);
        assert_eq!(status.codex.setup_command, None);
        assert_eq!(status.exe_path, exe);
    }

    #[test]
    fn mcp_setup_status_serializes_per_client_fields() {
        let status = McpSetupStatus {
            exe_path: "/mock/keynobi".into(),
            setup_command: Some("/mock/keynobi --mcp".into()),
            location_problem: None,
            claude: McpClientSetupStatus {
                client_found: true,
                is_configured: true,
                configured_command: Some("/mock/keynobi --mcp".into()),
                configured_scope: Some("user".into()),
                setup_command: Some(
                    "claude mcp add --scope user --transport stdio keynobi -- \"/mock/keynobi\" --mcp"
                        .into(),
                ),
            },
            codex: McpClientSetupStatus {
                client_found: false,
                is_configured: false,
                configured_command: None,
                configured_scope: None,
                setup_command: Some("codex mcp add keynobi -- \"/mock/keynobi\" --mcp".into()),
            },
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"claude\""));
        assert!(json.contains("\"codex\""));
        assert!(json.contains("\"clientFound\""));
        assert!(json.contains("\"locationProblem\":null"));
        assert!(json.contains("\"configuredScope\":\"user\""));
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

/// Return the live MCP sessions: clients attached to this app, and standalone
/// `keynobi --mcp` servers running with their own state.
#[tauri::command]
pub async fn get_mcp_server_status(
    registry: tauri::State<'_, McpSessionRegistry>,
) -> Result<McpServerStatus, AppError> {
    let standalone = tokio::task::spawn_blocking(mcp_sessions::list_standalone_servers)
        .await
        .map_err(|e| AppError::McpError(format!("Failed to list MCP servers: {e}")))?;
    Ok(McpServerStatus {
        listening: registry.is_listening(),
        app_version: mcp_sessions::APP_VERSION.to_string(),
        attached: registry.sessions(),
        standalone,
    })
}

// ── Agent skill ───────────────────────────────────────────────────────────────

fn home_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| AppError::NotFound("Home folder not found".into()))
}

/// The Keynobi agent skill and whether it is installed for Claude Code.
/// Reads only.
#[tauri::command]
pub async fn get_agent_skill_status() -> Result<AgentSkillStatus, AppError> {
    let home = home_dir()?;
    tokio::task::spawn_blocking(move || agent_skill::status(&home))
        .await
        .map_err(|e| AppError::Other(format!("Failed to read the agent skill: {e}")))
}

/// Install the Keynobi agent skill for Claude Code. An existing different
/// `SKILL.md` is replaced only when `replace` is true.
#[tauri::command]
pub async fn install_agent_skill(replace: bool) -> Result<AgentSkillStatus, AppError> {
    let home = home_dir()?;
    tokio::task::spawn_blocking(move || agent_skill::install(&home, replace))
        .await
        .map_err(|e| AppError::Other(format!("Failed to install the agent skill: {e}")))?
}

/// Clear the MCP activity log.
#[tauri::command]
pub async fn clear_mcp_activity() -> Result<(), String> {
    tokio::task::spawn_blocking(mcp_activity::clear_activity_log)
        .await
        .map_err(|e| format!("Failed to clear MCP activity log: {e}"))
}
