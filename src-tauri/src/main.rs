// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // `--mcp`: serve MCP on stdio, attached to the running app when possible.
    // Usage: keynobi --mcp [--project /path/to/project] [--attach-only] [--toolsets core,ui]
    if args.iter().any(|a| a == "--mcp") {
        let project_path = args
            .windows(2)
            .find(|w| w[0] == "--project")
            .map(|w| std::path::PathBuf::from(&w[1]));
        let attach_only = args.iter().any(|a| a == "--attach-only");
        let toolsets = match keynobi_lib::services::mcp_toolsets::Toolsets::from_args(&args) {
            Ok(toolsets) => toolsets,
            Err(e) => {
                eprintln!("keynobi: {e}");
                std::process::exit(2);
            }
        };

        let rt =
            tokio::runtime::Runtime::new().expect("failed to create tokio runtime for MCP server");
        let code = rt.block_on(keynobi_lib::services::mcp_server::run_mcp(
            project_path,
            attach_only,
            toolsets,
        ));
        // Exit without waiting for the runtime: a relay may still be blocked
        // reading stdin, which cannot be cancelled.
        std::process::exit(code);
    }

    // Normal mode: launch the full Tauri GUI.
    keynobi_lib::run();
}
