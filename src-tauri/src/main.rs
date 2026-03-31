// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --mcp flag: run as a headless MCP stdio server without a GUI.
    // Usage: android-dev-companion --mcp [--project /path/to/project]
    if args.iter().any(|a| a == "--mcp") {
        let project_path = args.windows(2)
            .find(|w| w[0] == "--project")
            .map(|w| std::path::PathBuf::from(&w[1]));

        let rt = tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime for MCP server");
        rt.block_on(android_ide_lib::services::mcp_server::run_headless_mcp(project_path));
        return;
    }

    // Normal mode: launch the full Tauri GUI.
    android_ide_lib::run();
}
