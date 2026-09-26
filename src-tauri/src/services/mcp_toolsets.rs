//! Toolsets: groups of MCP tools a client can choose to see, so it can keep
//! its tool list short (`keynobi --mcp --toolsets core,ui`). Every tool
//! belongs to exactly one toolset; without `--toolsets` all are served.
use std::collections::BTreeSet;

/// A group of MCP tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Toolset {
    /// Build, logcat, crashes, project, health, and reading devices and apps.
    Core,
    /// Reading the UI hierarchy and driving the UI.
    Ui,
    /// Installing, launching, stopping, and changing apps and devices.
    DeviceAdmin,
}

const CORE_TOOLS: &[&str] = &[
    "run_gradle_task",
    "get_build_status",
    "get_build_errors",
    "get_build_log",
    "cancel_build",
    "list_build_variants",
    "set_active_variant",
    "find_apk_path",
    "run_tests",
    "get_build_config",
    "start_logcat",
    "stop_logcat",
    "clear_logcat",
    "get_logcat_entries",
    "get_logcat_stats",
    "get_crash_logs",
    "get_crash_stack_trace",
    "get_project_info",
    "run_health_check",
    "list_devices",
    "get_device_info",
    "dump_app_info",
    "get_memory_info",
    "get_app_runtime_state",
    "get_exit_reasons",
    "list_avds",
];

const UI_TOOLS: &[&str] = &[
    "get_ui_hierarchy",
    "list_clickable_elements",
    "find_ui_elements",
    "find_ui_parent",
    "compare_ui_state",
    "wait_for_element",
    "ui_wait_for_idle",
    "ui_assert_element",
    "ui_tap",
    "ui_tap_element",
    "ui_type_text",
    "ui_fill_input",
    "ui_type_text_unicode",
    "clear_focused_input",
    "hide_soft_keyboard",
    "send_ui_key",
    "ui_swipe",
    "ui_scroll_until_element",
    "screenshot",
];

const DEVICE_ADMIN_TOOLS: &[&str] = &[
    "install_apk",
    "launch_app",
    "stop_app",
    "restart_app",
    "grant_runtime_permission",
    "revoke_runtime_permission",
    "set_network_state",
    "set_device_orientation",
    "open_deep_link",
    "open_app_settings",
    "launch_avd",
    "stop_avd",
];

impl Toolset {
    pub const ALL: [Toolset; 3] = [Toolset::Core, Toolset::Ui, Toolset::DeviceAdmin];

    /// The name `--toolsets` takes.
    pub fn name(self) -> &'static str {
        match self {
            Toolset::Core => "core",
            Toolset::Ui => "ui",
            Toolset::DeviceAdmin => "device-admin",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    /// The tools in this toolset.
    pub fn tools(self) -> &'static [&'static str] {
        match self {
            Toolset::Core => CORE_TOOLS,
            Toolset::Ui => UI_TOOLS,
            Toolset::DeviceAdmin => DEVICE_ADMIN_TOOLS,
        }
    }

    /// The toolset `tool` belongs to; `None` for a name that is not a tool.
    pub fn of(tool: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.tools().contains(&tool))
    }
}

fn all_names() -> String {
    Toolset::ALL.map(Toolset::name).join(", ")
}

/// The toolsets a session serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolsets(BTreeSet<Toolset>);

impl Default for Toolsets {
    fn default() -> Self {
        Self::all()
    }
}

impl Toolsets {
    pub fn all() -> Self {
        Self(Toolset::ALL.into_iter().collect())
    }

    /// Parse a comma-separated list such as `core,ui`.
    pub fn parse(list: &str) -> Result<Self, String> {
        let names: Vec<String> = list.split(',').map(|n| n.trim().to_string()).collect();
        Self::from_names(&names)
    }

    /// The toolsets named in `names`. An unknown or empty name is an error.
    pub fn from_names(names: &[String]) -> Result<Self, String> {
        let mut sets = BTreeSet::new();
        for name in names {
            let set = Toolset::from_name(name).ok_or_else(|| {
                if name.is_empty() {
                    format!(
                        "--toolsets needs toolset names separated by commas: {}",
                        all_names()
                    )
                } else {
                    format!(
                        "unknown toolset \"{name}\" in --toolsets; the toolsets are {}",
                        all_names()
                    )
                }
            })?;
            sets.insert(set);
        }
        if sets.is_empty() {
            return Err(format!("--toolsets needs at least one of {}", all_names()));
        }
        Ok(Self(sets))
    }

    /// The `--toolsets` value in `args` (`--toolsets a,b` or `--toolsets=a,b`),
    /// or every toolset when it is absent.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut found = None;
        for (i, arg) in args.iter().enumerate() {
            if arg == "--toolsets" {
                let value = args
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| {
                        format!(
                            "--toolsets needs a value, for example --toolsets core,ui ({})",
                            all_names()
                        )
                    })?;
                found = Some(value.as_str());
            } else if let Some(value) = arg.strip_prefix("--toolsets=") {
                found = Some(value);
            }
        }
        found.map_or_else(|| Ok(Self::all()), Self::parse)
    }

    pub fn is_all(&self) -> bool {
        self.0.len() == Toolset::ALL.len()
    }

    pub fn contains(&self, set: Toolset) -> bool {
        self.0.contains(&set)
    }

    /// The enabled toolsets' names, in a fixed order.
    pub fn names(&self) -> Vec<String> {
        self.0.iter().map(|t| t.name().to_string()).collect()
    }

    /// The tools of the toolsets not enabled.
    pub fn hidden_tools(&self) -> impl Iterator<Item = &'static str> + '_ {
        Toolset::ALL
            .into_iter()
            .filter(|t| !self.contains(*t))
            .flat_map(|t| t.tools().iter().copied())
    }

    /// Why a call to `tool` is refused, when its toolset is not enabled.
    pub fn refusal(&self, tool: &str) -> Option<String> {
        let set = Toolset::of(tool).filter(|t| !self.contains(*t))?;
        Some(format!(
            "{tool} is in the \"{}\" toolset, which this Keynobi MCP server does not serve \
             (it was started with --toolsets {}). To use it, add {} to --toolsets in the AI \
             client's Keynobi MCP server configuration and restart the server.",
            set.name(),
            self.names().join(","),
            set.name()
        ))
    }

    /// A sentence for the server's instructions; `None` when every toolset is served.
    pub fn describe(&self) -> Option<String> {
        if self.is_all() {
            return None;
        }
        let hidden: Vec<&str> = Toolset::ALL
            .into_iter()
            .filter(|t| !self.contains(*t))
            .map(Toolset::name)
            .collect();
        Some(format!(
            "Toolsets: {} only; the {} tools are not available in this session.",
            self.names().join(", "),
            hidden.join(" and ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn toolsets_parse_from_a_comma_separated_list() {
        let sets = Toolsets::parse(" ui ,core,ui").unwrap();
        assert_eq!(sets.names(), ["core", "ui"]);
        assert!(!sets.is_all());
        assert!(Toolsets::parse("core,ui,device-admin").unwrap().is_all());
    }

    #[test]
    fn unknown_or_empty_toolset_names_are_errors_that_list_the_toolsets() {
        let err = Toolsets::parse("core,devices").unwrap_err();
        assert!(err.contains("unknown toolset \"devices\""), "{err}");
        assert!(err.contains("core, ui, device-admin"), "{err}");
        for empty in ["", "core,", " , "] {
            let err = Toolsets::parse(empty).unwrap_err();
            assert!(err.contains("core, ui, device-admin"), "{empty:?}: {err}");
        }
    }

    #[test]
    fn toolsets_come_from_the_command_line() {
        let all = Toolsets::from_args(&args(&["keynobi", "--mcp"])).unwrap();
        assert!(all.is_all());
        let spaced =
            Toolsets::from_args(&args(&["keynobi", "--mcp", "--toolsets", "core"])).unwrap();
        assert_eq!(spaced.names(), ["core"]);
        let joined =
            Toolsets::from_args(&args(&["keynobi", "--toolsets=ui,core", "--mcp"])).unwrap();
        assert_eq!(joined.names(), ["core", "ui"]);
        for missing in [
            &["keynobi", "--mcp", "--toolsets"][..],
            &["keynobi", "--toolsets", "--mcp"],
        ] {
            let err = Toolsets::from_args(&args(missing)).unwrap_err();
            assert!(err.contains("--toolsets needs a value"), "{err}");
        }
        assert!(Toolsets::from_args(&args(&["keynobi", "--mcp", "--toolsets", "ux"])).is_err());
    }

    #[test]
    fn a_hidden_tool_is_refused_naming_its_toolset() {
        let core = Toolsets::parse("core").unwrap();
        assert_eq!(core.refusal("list_devices"), None);
        assert_eq!(core.refusal("not_a_tool"), None);
        let refusal = core.refusal("ui_tap").unwrap();
        assert!(
            refusal.starts_with("ui_tap is in the \"ui\" toolset")
                && refusal.contains("--toolsets core)")
                && refusal.contains("add ui to --toolsets"),
            "{refusal}"
        );
        assert!(core.hidden_tools().any(|t| t == "install_apk"));
        assert!(!core.hidden_tools().any(|t| t == "run_gradle_task"));
        assert_eq!(Toolsets::all().hidden_tools().count(), 0);
        assert_eq!(Toolsets::all().describe(), None);
        assert_eq!(
            core.describe().as_deref(),
            Some("Toolsets: core only; the ui and device-admin tools are not available in this session.")
        );
    }
}
