//! Run configurations: what Run builds and launches in a project.
//!
//! Stored locally in the project's registry entry (`ProjectEntry`) and
//! written through `mutate_settings`, so saves from several processes merge
//! under the data lock. The settings UI's snapshot never writes them (the
//! backend owns `recent_projects`).
//!
//! A project's first read creates them: one configuration for its application
//! module (**Default**) or one per application module, from the last variant
//! and device. The old fields stay and follow the active configuration, so an
//! older Keynobi still finds its variant.
//!
//! Configurations the user shares live in the project's shared file instead
//! (`shared_run_configurations`); their local state stays here, by name.

use crate::models::error::AppError;
use crate::models::run_configuration::{
    LocalRunState, ProjectRunConfigurations, RunConfiguration, RunLaunch,
    SharedRunConfigurationProblem, TargetPreference,
};
use crate::models::settings::{AppSettings, ProjectEntry};
use crate::services::shared_run_configurations::{self, SharedRead};
use crate::services::{gradle_modules, settings_manager, variant_manager};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Most run configurations per project.
pub const MAX_RUN_CONFIGURATIONS: usize = 32;

/// Longest configuration name, in characters.
pub const MAX_RUN_CONFIGURATION_NAME_CHARS: usize = 64;

/// Longest logcat filter, in bytes.
pub const MAX_LOGCAT_FILTER_BYTES: usize = 1024;

/// Longest variant name, in characters.
const MAX_VARIANT_CHARS: usize = 128;

/// Largest build file read to find a module's declared variants.
const MAX_BUILD_FILE_BYTES: u64 = 1024 * 1024;

/// The configuration created for a project with one application module.
pub const DEFAULT_RUN_CONFIGURATION: &str = "Default";

/// Variant used when neither the project's last variant nor its build file
/// names one; AGP always defines it.
const FALLBACK_VARIANT: &str = "debug";

// ── Migration ─────────────────────────────────────────────────────────────────

/// What creating a project's first configurations needs from its files, read
/// before the data lock.
#[derive(Debug, Clone, Default)]
pub struct MigrationSeed {
    modules: Vec<ModuleSeed>,
    /// Names the project's shared file offers; migration creates no local
    /// configuration that would hide one.
    shared_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct ModuleSeed {
    /// Gradle path.
    path: String,
    /// Variants the module's build file declares; empty when it declares none.
    declared_variants: Vec<String>,
    /// The default among them.
    default_variant: Option<String>,
}

/// Read the project's application modules (at most
/// `MAX_RUN_CONFIGURATIONS`) and the variants their build files declare.
pub fn migration_seed(gradle_root: &Path) -> MigrationSeed {
    let modules = gradle_modules::application_modules(gradle_root)
        .into_iter()
        .take(MAX_RUN_CONFIGURATIONS)
        .map(|module| {
            let (declared_variants, default_variant) = declared_variants(gradle_root, &module);
            ModuleSeed {
                path: module.path,
                declared_variants,
                default_variant,
            }
        })
        .collect();
    MigrationSeed {
        modules,
        shared_names: Vec::new(),
    }
}

/// Variants the module's build file (else the root project's) declares, and
/// the default among them.
pub(crate) fn declared_variants(
    gradle_root: &Path,
    module: &gradle_modules::GradleModule,
) -> (Vec<String>, Option<String>) {
    let mut files: Vec<String> = Vec::new();
    if module.path != ":" {
        let dir = module.relative_dir(gradle_root);
        files.push(format!("{dir}/build.gradle.kts"));
        files.push(format!("{dir}/build.gradle"));
    }
    files.push("build.gradle.kts".into());
    files.push("build.gradle".into());
    for relative in files {
        let Some((path, content)) = read_build_file(gradle_root, &relative) else {
            continue;
        };
        if let Some(list) = variant_manager::parse_variants_from_gradle(&path, &content) {
            if !list.variants.is_empty() {
                let default =
                    variant_manager::infer_default_variant_name(gradle_root, &list.variants);
                return (list.variants.into_iter().map(|v| v.name).collect(), default);
            }
        }
    }
    (Vec::new(), None)
}

/// A build file inside the project, when it is a regular file of at most
/// `MAX_BUILD_FILE_BYTES`.
fn read_build_file(gradle_root: &Path, relative: &str) -> Option<(PathBuf, String)> {
    let path = crate::utils::path::resolve_project_file(gradle_root, relative).ok()?;
    if std::fs::metadata(&path).ok()?.len() > MAX_BUILD_FILE_BYTES {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    Some((path, content))
}

/// Create `entry`'s first configurations from `seed`, once: one named
/// **Default** for a single application module, else one per module (named
/// after it) with none active. The variant is the entry's last variant when
/// the module declares it (or declares none), else the module's default. Each
/// targets the last used device. A name the project's shared file offers is
/// left to the shared configuration (still active when it would be). Returns
/// whether it changed `entry`; an entry that has configurations (even none)
/// or a project without an application module is left alone.
pub fn migrate(entry: &mut ProjectEntry, seed: &MigrationSeed) -> bool {
    if entry.run_configurations.is_some() || seed.modules.is_empty() {
        return false;
    }
    let names: Vec<(String, bool)> = migrated_names(&seed.modules)
        .into_iter()
        .map(|name| {
            match seed
                .shared_names
                .iter()
                .find(|shared| shared.eq_ignore_ascii_case(&name))
            {
                Some(shared) => (shared.clone(), true),
                None => (name, false),
            }
        })
        .collect();
    entry.active_run_configuration = match names.as_slice() {
        [(only, _)] => Some(only.clone()),
        _ => None,
    };
    let mut configurations = Vec::new();
    let mut local = BTreeMap::new();
    for (module, (name, shared)) in seed.modules.iter().zip(names) {
        let variant = match entry.last_build_variant.as_deref() {
            Some(last)
                if validate_variant(last).is_ok()
                    && (module.declared_variants.is_empty()
                        || module.declared_variants.iter().any(|v| v == last)) =>
            {
                last.to_string()
            }
            _ => module
                .default_variant
                .clone()
                .unwrap_or_else(|| FALLBACK_VARIANT.to_string()),
        };
        if !shared {
            configurations.push(RunConfiguration {
                name: name.clone(),
                module: module.path.clone(),
                variant,
                task: None,
                launch: RunLaunch::Default,
                logcat_filter: None,
            });
        }
        local.insert(
            name,
            LocalRunState {
                target: TargetPreference::LastUsed,
                last_device: entry.last_device.clone(),
                approved_project_file_sha256: None,
            },
        );
    }
    entry.run_configurations = Some(configurations);
    entry.run_local = local;
    true
}

/// **Default** for one module; otherwise each module's last path segment,
/// or its whole path when two modules share a last segment.
fn migrated_names(modules: &[ModuleSeed]) -> Vec<String> {
    if let [_] = modules {
        return vec![DEFAULT_RUN_CONFIGURATION.to_string()];
    }
    let last = |path: &str| path.rsplit(':').next().unwrap_or(path).to_string();
    modules
        .iter()
        .map(|m| {
            let short = last(&m.path);
            let shared = modules.iter().filter(|o| last(&o.path) == short).count() > 1;
            let name = if short.is_empty() || shared {
                m.path.trim_start_matches(':').to_string()
            } else {
                short
            };
            let name = if name.is_empty() {
                DEFAULT_RUN_CONFIGURATION.to_string()
            } else {
                name
            };
            name.chars()
                .take(MAX_RUN_CONFIGURATION_NAME_CHARS)
                .collect()
        })
        .collect()
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Check `config` before it is saved: the name (unique among `existing`,
/// ignoring case, apart from the configuration it replaces), the module (one
/// of `application_modules`), the variant, the task (a valid Gradle task in
/// that module), the launch activity or deep link, and the logcat filter.
pub fn validate_run_configuration(
    config: &RunConfiguration,
    application_modules: &[String],
    existing: &[RunConfiguration],
) -> Result<(), String> {
    validate_name(&config.name)?;
    if let Some(other) = existing
        .iter()
        .find(|c| c.name != config.name && c.name.eq_ignore_ascii_case(&config.name))
    {
        return Err(format!(
            "A run configuration named '{}' already exists.",
            other.name
        ));
    }
    if !application_modules.contains(&config.module) {
        return Err(format!(
            "'{}' is not an application module of this project. Application modules: {}.",
            config.module,
            if application_modules.is_empty() {
                "none found".to_string()
            } else {
                application_modules.join(", ")
            }
        ));
    }
    validate_variant(&config.variant)?;
    if let Some(task) = &config.task {
        validate_task(task, &config.module)?;
    }
    match &config.launch {
        RunLaunch::Activity { name } => crate::utils::validation::validate_activity_name(name)?,
        RunLaunch::DeepLink { uri } => crate::services::ui_automation::validate_deep_link_uri(uri)
            .map_err(|e| format!("Invalid deep link: {e}"))?,
        RunLaunch::Default | RunLaunch::None => {}
    }
    if let Some(filter) = &config.logcat_filter {
        if filter.len() > MAX_LOGCAT_FILTER_BYTES {
            return Err(format!(
                "The logcat filter is too long (max {MAX_LOGCAT_FILTER_BYTES} bytes)."
            ));
        }
        if filter.chars().any(char::is_control) {
            return Err("The logcat filter must be one line without control characters.".into());
        }
    }
    Ok(())
}

/// [`validate_run_configuration`] against the application modules of the
/// project at `gradle_root`.
pub fn validate_in_project(
    config: &RunConfiguration,
    gradle_root: &Path,
    existing: &[RunConfiguration],
) -> Result<(), String> {
    let modules: Vec<String> = gradle_modules::application_modules(gradle_root)
        .into_iter()
        .map(|m| m.path)
        .collect();
    validate_run_configuration(config, &modules, existing)
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("A run configuration needs a name.".into());
    }
    if name.trim() != name {
        return Err("A run configuration name cannot start or end with spaces.".into());
    }
    if name.chars().count() > MAX_RUN_CONFIGURATION_NAME_CHARS {
        return Err(format!(
            "The run configuration name is too long (max {MAX_RUN_CONFIGURATION_NAME_CHARS} characters)."
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("A run configuration name cannot contain control characters.".into());
    }
    Ok(())
}

/// An AGP variant name: an ASCII letter, then ASCII letters and digits.
pub(crate) fn validate_variant(variant: &str) -> Result<(), String> {
    let mut chars = variant.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric())
        && variant.chars().count() <= MAX_VARIANT_CHARS;
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Invalid variant '{variant}': a variant name is letters and digits, starting with a \
             letter (for example debug or freeRelease)."
        ))
    }
}

/// A valid Gradle task of `module`: `:mobile:bundleRelease`, or for the root
/// project `bundleRelease` (or `:bundleRelease`).
pub(crate) fn validate_task(task: &str, module: &str) -> Result<(), String> {
    crate::utils::validation::validate_gradle_task(task)?;
    let name = if module == ":" {
        Some(task.strip_prefix(':').unwrap_or(task))
    } else {
        task.strip_prefix(module)
            .and_then(|rest| rest.strip_prefix(':'))
    };
    match name {
        Some(name) if !name.is_empty() && !name.contains(':') => Ok(()),
        _ => Err(format!(
            "The task '{task}' is not a task of {module}: name it {}.",
            if module == ":" {
                "without a module path (for example assembleDebug)".to_string()
            } else {
                format!("in the module (for example {module}:assembleDebug)")
            }
        )),
    }
}

// ── Storage ───────────────────────────────────────────────────────────────────

pub(crate) fn settings_path() -> PathBuf {
    settings_manager::data_dir().join("settings.json")
}

fn is_project(entry: &ProjectEntry, project_root: &str) -> bool {
    entry.path == project_root || entry.gradle_root.as_deref() == Some(project_root)
}

fn find<'a>(settings: &'a AppSettings, project_root: &str) -> Option<&'a ProjectEntry> {
    settings
        .recent_projects
        .iter()
        .find(|e| is_project(e, project_root))
}

fn not_in_registry(project_root: &str) -> AppError {
    AppError::NotFound(format!(
        "{project_root} is not in the project registry. Open it in Keynobi first."
    ))
}

fn gradle_root_of(entry: &ProjectEntry) -> PathBuf {
    PathBuf::from(entry.gradle_root.as_deref().unwrap_or(&entry.path))
}

fn configurations_of(entry: &ProjectEntry) -> ProjectRunConfigurations {
    ProjectRunConfigurations {
        configurations: entry.run_configurations.clone().unwrap_or_default(),
        active: entry.active_run_configuration.clone(),
        local: entry.run_local.clone(),
        shared: Vec::new(),
        shared_file: None,
    }
}

/// Point the old fields at the active configuration, so an older Keynobi and
/// MCP `set_active_variant` readers see the same variant.
fn sync_old_fields(entry: &mut ProjectEntry) {
    let active = entry.active_run_configuration.as_deref();
    if let Some(config) = entry
        .run_configurations
        .iter()
        .flatten()
        .find(|c| Some(c.name.as_str()) == active)
    {
        entry.last_build_variant = Some(config.variant.clone());
    }
}

/// The variant picker changed the project's variant: the active
/// configuration follows it. Called under the data lock with the entry that
/// `last_build_variant` is written to.
pub fn follow_variant(entry: &mut ProjectEntry, variant: &str) {
    if validate_variant(variant).is_err() {
        return;
    }
    let active = entry.active_run_configuration.clone();
    if let Some(config) = entry
        .run_configurations
        .iter_mut()
        .flatten()
        .find(|c| Some(&c.name) == active.as_ref())
    {
        config.variant = variant.to_string();
    }
}

/// Run `edit` on the project's entry under the data lock, after creating its
/// first configurations from `seed` when it has none, and save. Nothing is
/// saved when `edit` fails.
fn edit_at<R>(
    path: &Path,
    project_root: &str,
    seed: &MigrationSeed,
    edit: impl FnOnce(&mut ProjectEntry, &Path) -> Result<R, AppError>,
) -> Result<R, AppError> {
    let mut rejected: Option<AppError> = None;
    let saved = settings_manager::mutate_settings_at_path_with_result(path, |settings| {
        let Some(entry) = settings
            .recent_projects
            .iter_mut()
            .find(|e| is_project(e, project_root))
        else {
            rejected = Some(not_in_registry(project_root));
            return Err(String::new());
        };
        migrate(entry, seed);
        let gradle_root = gradle_root_of(entry);
        match edit(entry, &gradle_root) {
            Ok(value) => {
                sync_old_fields(entry);
                Ok(value)
            }
            Err(e) => {
                rejected = Some(e);
                Err(String::new())
            }
        }
    });
    saved.map_err(|message| rejected.unwrap_or(AppError::SettingsError(message)))
}

/// The project's run configurations, created on the first read (see
/// [`migrate`]): the local ones, then those shared with the project (see
/// [`with_shared`]).
pub fn list(project_root: &str) -> Result<ProjectRunConfigurations, AppError> {
    list_at(&settings_path(), project_root)
}

pub fn list_at(path: &Path, project_root: &str) -> Result<ProjectRunConfigurations, AppError> {
    let Prepared {
        entry,
        shared,
        seed,
        migrated,
    } = prepare(path, project_root)?;
    let listed = if migrated {
        edit_at(path, project_root, &seed, |entry, _| {
            Ok(configurations_of(entry))
        })?
    } else {
        configurations_of(&entry)
    };
    Ok(with_shared(listed, &shared))
}

/// Save `config`, replacing the configuration of the same name or adding it
/// (at most `MAX_RUN_CONFIGURATIONS`), after [`validate_in_project`]. With
/// `shared`, it is moved into the project's shared file (`Some(true)`) or out
/// of it into the local ones (`Some(false)`); without, it stays where it is.
pub fn save(
    project_root: &str,
    config: RunConfiguration,
    shared: Option<bool>,
) -> Result<ProjectRunConfigurations, AppError> {
    save_at(&settings_path(), project_root, config, shared)
}

pub fn save_at(
    path: &Path,
    project_root: &str,
    config: RunConfiguration,
    shared: Option<bool>,
) -> Result<ProjectRunConfigurations, AppError> {
    let Prepared {
        entry,
        shared: file,
        seed,
        ..
    } = prepare(path, project_root)?;
    let gradle_root = gradle_root_of(&entry);
    let locals = entry.run_configurations.clone().unwrap_or_default();
    let is_shared = visible_shared(&locals, &file).any(|c| c.name == config.name);
    let to_shared = shared.unwrap_or(is_shared);
    if is_shared && !to_shared {
        // Unsharing rewrites the file after the local save: check it first.
        file.check_writable()?;
    }
    // Checked here too, so a refused save writes neither file.
    let others: Vec<RunConfiguration> = locals
        .iter()
        .chain(visible_shared(&locals, &file))
        .filter(|c| c.name != config.name)
        .cloned()
        .collect();
    validate_in_project(&config, &gradle_root, &others).map_err(AppError::InvalidInput)?;

    let saved = if to_shared {
        let mut list = file.configurations.clone();
        match list
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(&config.name))
        {
            Some(index) => list[index] = config.clone(),
            None => list.push(config.clone()),
        }
        shared_run_configurations::write(&gradle_root, &file, &list)?;
        // The shared one replaces a local one of the same name; its local
        // state stays.
        edit_at(path, project_root, &seed, |entry, _| {
            if let Some(configurations) = entry.run_configurations.as_mut() {
                configurations.retain(|c| c.name != config.name);
            }
            entry.run_local.entry(config.name.clone()).or_default();
            Ok(configurations_of(entry))
        })?
    } else {
        let shared_names: Vec<RunConfiguration> = visible_shared(&locals, &file)
            .filter(|c| c.name != config.name)
            .cloned()
            .collect();
        let saved = edit_at(path, project_root, &seed, |entry, gradle_root| {
            let configurations = entry.run_configurations.get_or_insert_with(Vec::new);
            let others: Vec<RunConfiguration> = configurations
                .iter()
                .chain(&shared_names)
                .cloned()
                .collect();
            validate_in_project(&config, gradle_root, &others).map_err(AppError::InvalidInput)?;
            match configurations.iter().position(|c| c.name == config.name) {
                Some(index) => configurations[index] = config.clone(),
                None if configurations.len() >= MAX_RUN_CONFIGURATIONS => {
                    return Err(AppError::InvalidInput(format!(
                        "A project can have at most {MAX_RUN_CONFIGURATIONS} run configurations. \
                         Delete one first."
                    )));
                }
                None => configurations.push(config.clone()),
            }
            entry.run_local.entry(config.name.clone()).or_default();
            Ok(configurations_of(entry))
        })?;
        // Saved locally first, so a failed rewrite leaves it in both places
        // (the local one is used), never in neither.
        if is_shared {
            let rest: Vec<RunConfiguration> = file
                .configurations
                .iter()
                .filter(|c| c.name != config.name)
                .cloned()
                .collect();
            shared_run_configurations::write(&gradle_root, &file, &rest)?;
        }
        saved
    };
    Ok(with_shared(
        saved,
        &shared_run_configurations::read(&gradle_root),
    ))
}

/// Delete the configuration named `name`, from the project's shared file
/// when it is shared. No configuration is active afterwards when it was the
/// active one.
pub fn delete(project_root: &str, name: &str) -> Result<ProjectRunConfigurations, AppError> {
    delete_at(&settings_path(), project_root, name)
}

pub fn delete_at(
    path: &Path,
    project_root: &str,
    name: &str,
) -> Result<ProjectRunConfigurations, AppError> {
    let Prepared {
        entry,
        shared: file,
        seed,
        ..
    } = prepare(path, project_root)?;
    let gradle_root = gradle_root_of(&entry);
    let locals = entry.run_configurations.clone().unwrap_or_default();
    let is_shared = visible_shared(&locals, &file).any(|c| c.name == name);
    if is_shared {
        let rest: Vec<RunConfiguration> = file
            .configurations
            .iter()
            .filter(|c| c.name != name)
            .cloned()
            .collect();
        shared_run_configurations::write(&gradle_root, &file, &rest)?;
    }
    let deleted = edit_at(path, project_root, &seed, |entry, _| {
        let configurations = entry.run_configurations.get_or_insert_with(Vec::new);
        let before = configurations.len();
        configurations.retain(|c| c.name != name);
        if configurations.len() == before && !is_shared {
            return Err(no_such_configuration(name));
        }
        entry.run_local.remove(name);
        if entry.active_run_configuration.as_deref() == Some(name) {
            entry.active_run_configuration = None;
        }
        Ok(configurations_of(entry))
    })?;
    Ok(with_shared(
        deleted,
        &if is_shared {
            shared_run_configurations::read(&gradle_root)
        } else {
            file
        },
    ))
}

/// Make the configuration named `name` the active one. The project's last
/// variant follows it.
pub fn set_active(project_root: &str, name: &str) -> Result<ProjectRunConfigurations, AppError> {
    set_active_at(&settings_path(), project_root, name)
}

pub fn set_active_at(
    path: &Path,
    project_root: &str,
    name: &str,
) -> Result<ProjectRunConfigurations, AppError> {
    edit_known(path, project_root, name, |entry| {
        entry.active_run_configuration = Some(name.to_string());
    })
}

/// Remember `serial` as the device the configuration named `name` last ran
/// on (`LocalRunState.last_device`, which a `lastUsed` target prefers).
pub fn record_last_device(
    project_root: &str,
    name: &str,
    serial: &str,
) -> Result<ProjectRunConfigurations, AppError> {
    record_last_device_at(&settings_path(), project_root, name, serial)
}

pub fn record_last_device_at(
    path: &Path,
    project_root: &str,
    name: &str,
    serial: &str,
) -> Result<ProjectRunConfigurations, AppError> {
    crate::utils::validation::validate_device_serial(serial).map_err(AppError::InvalidInput)?;
    edit_known(path, project_root, name, |entry| {
        entry
            .run_local
            .entry(name.to_string())
            .or_default()
            .last_device = Some(serial.to_string());
    })
}

/// Set which device the configuration named `name` runs on. A serial and an
/// AVD name are validated; the device need not be connected now.
pub fn set_target(
    project_root: &str,
    name: &str,
    target: TargetPreference,
) -> Result<ProjectRunConfigurations, AppError> {
    set_target_at(&settings_path(), project_root, name, target)
}

pub fn set_target_at(
    path: &Path,
    project_root: &str,
    name: &str,
    target: TargetPreference,
) -> Result<ProjectRunConfigurations, AppError> {
    match &target {
        TargetPreference::Serial { serial } => {
            crate::utils::validation::validate_device_serial(serial)
                .map_err(AppError::InvalidInput)?
        }
        TargetPreference::Avd { name } => {
            crate::services::adb_manager::validate_avd_name(name).map_err(AppError::InvalidInput)?
        }
        TargetPreference::Ask | TargetPreference::LastUsed => {}
    }
    edit_known(path, project_root, name, |entry| {
        entry.run_local.entry(name.to_string()).or_default().target = target;
    })
}

/// Approve running the shared configuration named `name` as the project's
/// shared file is now: `sha256` is the file's hash the user reviewed. Refused
/// when the file changed since.
pub fn approve_shared(
    project_root: &str,
    name: &str,
    sha256: &str,
) -> Result<ProjectRunConfigurations, AppError> {
    approve_shared_at(&settings_path(), project_root, name, sha256)
}

pub fn approve_shared_at(
    path: &Path,
    project_root: &str,
    name: &str,
    sha256: &str,
) -> Result<ProjectRunConfigurations, AppError> {
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::InvalidInput(
            "Invalid SHA-256: expected 64 hexadecimal characters.".into(),
        ));
    }
    let Prepared {
        entry,
        shared: file,
        seed,
        ..
    } = prepare(path, project_root)?;
    let locals = entry.run_configurations.clone().unwrap_or_default();
    if !visible_shared(&locals, &file).any(|c| c.name == name) {
        return Err(AppError::NotFound(format!(
            "There is no shared run configuration named '{name}'."
        )));
    }
    if file.sha256() != Some(sha256) {
        return Err(AppError::InvalidInput(format!(
            "The project's shared run configurations ({}) changed since you reviewed them. \
             Review them again.",
            shared_run_configurations::SHARED_FILE
        )));
    }
    let approved = edit_at(path, project_root, &seed, |entry, _| {
        entry
            .run_local
            .entry(name.to_string())
            .or_default()
            .approved_project_file_sha256 = Some(sha256.to_string());
        Ok(configurations_of(entry))
    })?;
    Ok(with_shared(approved, &file))
}

fn no_such_configuration(name: &str) -> AppError {
    AppError::NotFound(format!("There is no run configuration named '{name}'."))
}

/// A project's registry entry and shared file, read before any edit takes
/// the data lock.
struct Prepared {
    /// The entry, with its first configurations created in memory when it
    /// has none (saved by the next edit, which is given `seed`).
    entry: ProjectEntry,
    shared: SharedRead,
    seed: MigrationSeed,
    /// Whether the entry had no configurations.
    migrated: bool,
}

/// Read the project's entry and shared file (see [`migrate`]).
fn prepare(path: &Path, project_root: &str) -> Result<Prepared, AppError> {
    let settings = settings_manager::load_settings_at_path(path);
    let mut entry = find(&settings, project_root)
        .ok_or_else(|| not_in_registry(project_root))?
        .clone();
    let gradle_root = gradle_root_of(&entry);
    let shared = shared_run_configurations::read(&gradle_root);
    if entry.run_configurations.is_some() {
        return Ok(Prepared {
            entry,
            shared,
            seed: MigrationSeed::default(),
            migrated: false,
        });
    }
    // The project's files are read before the data lock.
    let seed = MigrationSeed {
        shared_names: shared
            .configurations
            .iter()
            .map(|c| c.name.clone())
            .collect(),
        ..migration_seed(&gradle_root)
    };
    migrate(&mut entry, &seed);
    Ok(Prepared {
        entry,
        shared,
        seed,
        migrated: true,
    })
}

/// Run `edit` on the entry when `name` is one of its configurations, local
/// or shared.
fn edit_known(
    path: &Path,
    project_root: &str,
    name: &str,
    edit: impl FnOnce(&mut ProjectEntry),
) -> Result<ProjectRunConfigurations, AppError> {
    let Prepared { shared, seed, .. } = prepare(path, project_root)?;
    let edited = edit_at(path, project_root, &seed, |entry, _| {
        let locals = entry.run_configurations.clone().unwrap_or_default();
        if !locals.iter().any(|c| c.name == name)
            && !visible_shared(&locals, &shared).any(|c| c.name == name)
        {
            return Err(no_such_configuration(name));
        }
        edit(entry);
        Ok(configurations_of(entry))
    })?;
    Ok(with_shared(edited, &shared))
}

/// The shared configurations no local one hides: a local configuration wins
/// over a shared one of the same name (ignoring case).
fn visible_shared<'a>(
    locals: &'a [RunConfiguration],
    shared: &'a SharedRead,
) -> impl Iterator<Item = &'a RunConfiguration> {
    shared.configurations.iter().filter(move |shared| {
        !locals
            .iter()
            .any(|local| local.name.eq_ignore_ascii_case(&shared.name))
    })
}

/// `project`'s local configurations followed by the shared ones no local one
/// hides. A hidden one is reported in the shared file's problems.
fn with_shared(
    mut project: ProjectRunConfigurations,
    shared: &SharedRead,
) -> ProjectRunConfigurations {
    let mut file = shared.file.clone();
    for config in &shared.configurations {
        if let Some(local) = project
            .configurations
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&config.name))
        {
            if let Some(file) = file.as_mut() {
                file.problems.push(SharedRunConfigurationProblem {
                    name: Some(config.name.clone()),
                    message: format!(
                        "Your local configuration '{}' has the same name and is used instead. \
                         Rename or delete it to use the project's.",
                        local.name
                    ),
                });
            }
            continue;
        }
        project.shared.push(config.name.clone());
        project.configurations.push(config.clone());
    }
    project.shared_file = file;
    project
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use tempfile::TempDir;

    const APP: &str = "plugins { id(\"com.android.application\") }\n";

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A project with application modules `modules` (Gradle paths).
    fn project(modules: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        let includes: Vec<String> = modules.iter().map(|m| format!("\"{m}\"")).collect();
        write(
            dir.path(),
            "settings.gradle.kts",
            &format!("include({})\n", includes.join(", ")),
        );
        for module in modules {
            let rel = module.trim_start_matches(':').replace(':', "/");
            write(dir.path(), &format!("{rel}/build.gradle.kts"), APP);
        }
        dir
    }

    fn entry(root: &Path) -> ProjectEntry {
        ProjectEntry {
            id: "p".into(),
            path: root.to_string_lossy().into_owned(),
            name: "p".into(),
            gradle_root: Some(root.to_string_lossy().into_owned()),
            trusted: Some(true),
            ..Default::default()
        }
    }

    /// A settings file whose registry holds `entries`.
    fn settings_with(entries: Vec<ProjectEntry>) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        settings_manager::mutate_settings_at_path(&path, |s| s.recent_projects = entries).unwrap();
        (dir, path)
    }

    fn root_of(project: &TempDir) -> String {
        project.path().to_string_lossy().into_owned()
    }

    fn stored(path: &Path, project: &TempDir) -> ProjectEntry {
        settings_manager::load_settings_at_path(path)
            .recent_projects
            .into_iter()
            .find(|e| e.path == root_of(project))
            .unwrap()
    }

    fn config(name: &str, module: &str) -> RunConfiguration {
        RunConfiguration {
            name: name.into(),
            module: module.into(),
            variant: "debug".into(),
            task: None,
            launch: RunLaunch::Default,
            logcat_filter: None,
        }
    }

    // ── Migration ────────────────────────────────────────────────────────────

    #[test]
    fn a_single_module_project_gets_a_default_configuration() {
        let project = project(&[":mobile"]);
        let mut old = entry(project.path());
        old.last_build_variant = Some("release".into());
        old.last_device = Some("emulator-5554".into());
        let (_dir, path) = settings_with(vec![old]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert_eq!(
            listed.configurations,
            vec![RunConfiguration {
                variant: "release".into(),
                ..config(DEFAULT_RUN_CONFIGURATION, ":mobile")
            }]
        );
        assert_eq!(listed.active.as_deref(), Some(DEFAULT_RUN_CONFIGURATION));
        assert_eq!(
            listed.local[DEFAULT_RUN_CONFIGURATION],
            LocalRunState {
                target: TargetPreference::LastUsed,
                last_device: Some("emulator-5554".into()),
                approved_project_file_sha256: None,
            }
        );
        // Saved, with the old fields kept for an older Keynobi.
        let saved = stored(&path, &project);
        assert_eq!(saved.run_configurations, Some(listed.configurations));
        assert_eq!(saved.last_build_variant.as_deref(), Some("release"));
        assert_eq!(saved.last_device.as_deref(), Some("emulator-5554"));
    }

    #[test]
    fn without_a_last_variant_the_modules_default_variant_is_used() {
        let project = project(&[":app"]);
        write(
            project.path(),
            "app/build.gradle.kts",
            "plugins { id(\"com.android.application\") }\nandroid {\n    buildTypes {\n        staging {\n        }\n    }\n}\n",
        );
        let (_dir, path) = settings_with(vec![entry(project.path())]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert_eq!(listed.configurations[0].variant, "debug");
    }

    #[test]
    fn a_last_variant_the_module_does_not_declare_is_not_used() {
        let project = project(&[":app"]);
        write(
            project.path(),
            "app/build.gradle.kts",
            "plugins { id(\"com.android.application\") }\nandroid {\n    buildTypes {\n        staging {\n        }\n    }\n}\n",
        );
        let mut old = entry(project.path());
        old.last_build_variant = Some("paidDebug".into());
        let (_dir, path) = settings_with(vec![old]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert_eq!(listed.configurations[0].variant, "debug");
    }

    #[test]
    fn several_modules_get_one_configuration_each_and_none_active() {
        let project = project(&[":mobile", ":wear", ":apps:tv", ":legacy:tv"]);
        let mut old = entry(project.path());
        old.last_build_variant = Some("debug".into());
        let (_dir, path) = settings_with(vec![old]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        let names: Vec<(&str, &str)> = listed
            .configurations
            .iter()
            .map(|c| (c.name.as_str(), c.module.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("apps:tv", ":apps:tv"),
                ("legacy:tv", ":legacy:tv"),
                ("mobile", ":mobile"),
                ("wear", ":wear"),
            ]
        );
        assert_eq!(listed.active, None, "nothing runs until one is chosen");
        assert_eq!(listed.local.len(), 4);
    }

    #[test]
    fn an_entry_from_an_older_version_loads_and_is_migrated_once() {
        let project = project(&[":app"]);
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let root = serde_json::to_string(&root_of(&project)).unwrap();
        std::fs::write(
            &path,
            format!(
                r#"{{ "recentProjects": [{{ "id": "p", "path": {root}, "name": "p",
                    "lastBuildVariant": "debug", "lastDevice": null, "trusted": true }}] }}"#
            ),
        )
        .unwrap();

        let old = stored(&path, &project);
        assert_eq!(old.run_configurations, None);
        assert!(old.run_local.is_empty());
        assert_eq!(old.active_run_configuration, None);
        let json = serde_json::to_value(&old).unwrap();
        assert!(json.get("runConfigurations").is_none(), "{json}");

        list_at(&path, &root_of(&project)).unwrap();
        delete_at(&path, &root_of(&project), DEFAULT_RUN_CONFIGURATION).unwrap();

        // Deleting every configuration does not bring the Default back.
        let listed = list_at(&path, &root_of(&project)).unwrap();
        assert!(listed.configurations.is_empty());
        assert_eq!(stored(&path, &project).run_configurations, Some(Vec::new()));
        let saved = save_at(&path, &root_of(&project), config("Mine", ":app"), None).unwrap();
        assert_eq!(saved.configurations, vec![config("Mine", ":app")]);
    }

    #[test]
    fn migration_leaves_an_entry_that_has_configurations_even_none() {
        let project = project(&[":app"]);
        let seed = migration_seed(project.path());
        let mut emptied = ProjectEntry {
            run_configurations: Some(Vec::new()),
            ..entry(project.path())
        };

        assert!(!migrate(&mut emptied, &seed));
        assert_eq!(emptied.run_configurations, Some(Vec::new()));

        let mut unread = entry(project.path());
        assert!(migrate(&mut unread, &seed));
        assert!(!migrate(&mut unread, &seed), "only once");
    }

    #[test]
    fn a_project_without_an_application_module_is_not_migrated() {
        let project = TempDir::new().unwrap();
        write(project.path(), "settings.gradle.kts", "include(\":lib\")\n");
        write(
            project.path(),
            "lib/build.gradle.kts",
            "plugins { id(\"com.android.library\") }\n",
        );
        let (_dir, path) = settings_with(vec![entry(project.path())]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert!(listed.configurations.is_empty());
        assert_eq!(stored(&path, &project).run_configurations, None);
    }

    #[test]
    fn migration_is_capped() {
        let modules: Vec<String> = (0..MAX_RUN_CONFIGURATIONS + 3)
            .map(|i| format!(":m{i:02}"))
            .collect();
        let refs: Vec<&str> = modules.iter().map(String::as_str).collect();
        let project = project(&refs);
        let (_dir, path) = settings_with(vec![entry(project.path())]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert_eq!(listed.configurations.len(), MAX_RUN_CONFIGURATIONS);
    }

    #[test]
    fn an_unknown_project_is_not_found() {
        let (_dir, path) = settings_with(vec![]);

        assert!(matches!(
            list_at(&path, "/nowhere"),
            Err(AppError::NotFound(_))
        ));
    }

    // ── Validation ───────────────────────────────────────────────────────────

    fn modules() -> Vec<String> {
        vec![":app".into(), ":".into()]
    }

    fn rejects(config: RunConfiguration, needle: &str) {
        let existing = vec![config_named("Existing")];
        let err = validate_run_configuration(&config, &modules(), &existing).unwrap_err();
        assert!(err.contains(needle), "{needle:?} not in {err:?}");
    }

    fn config_named(name: &str) -> RunConfiguration {
        config(name, ":app")
    }

    #[test]
    fn a_valid_configuration_passes() {
        let full = RunConfiguration {
            task: Some(":app:bundleRelease".into()),
            launch: RunLaunch::DeepLink {
                uri: "myapp://home".into(),
            },
            logcat_filter: Some("package:mine level:warn".into()),
            ..config_named("Release")
        };
        validate_run_configuration(&full, &modules(), &[config_named("Existing")]).unwrap();
        // Saving again under its own name replaces it.
        validate_run_configuration(&full, &modules(), std::slice::from_ref(&full)).unwrap();
        // The root project's tasks have no module path.
        let root = RunConfiguration {
            task: Some("assembleDebug".into()),
            ..config("Root", ":")
        };
        validate_run_configuration(&root, &modules(), &[]).unwrap();
    }

    #[test]
    fn a_bad_name_is_rejected() {
        rejects(config_named(""), "needs a name");
        rejects(config_named(" Padded"), "spaces");
        rejects(
            config_named(&"n".repeat(MAX_RUN_CONFIGURATION_NAME_CHARS + 1)),
            "too long",
        );
        rejects(config_named("Line\nbreak"), "control characters");
        rejects(config_named("existing"), "already exists");
    }

    #[test]
    fn a_module_that_is_not_an_application_is_rejected() {
        rejects(config("Lib", ":lib"), "not an application module");
    }

    #[test]
    fn a_bad_variant_is_rejected() {
        for variant in ["", "free-debug", "1debug", "debug;rm"] {
            rejects(
                RunConfiguration {
                    variant: variant.into(),
                    ..config_named("V")
                },
                "Invalid variant",
            );
        }
    }

    #[test]
    fn a_bad_task_is_rejected() {
        let with_task = |task: &str| RunConfiguration {
            task: Some(task.into()),
            ..config_named("T")
        };
        rejects(with_task("--init-script"), "Gradle options");
        rejects(with_task(":app:assemble; rm -rf /"), "only alphanumeric");
        rejects(with_task(":wear:assembleDebug"), "not a task of :app");
        rejects(with_task("assembleDebug"), "not a task of :app");
        rejects(with_task(":app:"), "not a task of :app");
        rejects(with_task(":app:sub:assembleDebug"), "not a task of :app");
        rejects(
            RunConfiguration {
                task: Some(":app:assembleDebug".into()),
                ..config("Root", ":")
            },
            "not a task of :",
        );
    }

    #[test]
    fn a_bad_launch_is_rejected() {
        rejects(
            RunConfiguration {
                launch: RunLaunch::Activity {
                    name: "com.example/.Main; reboot".into(),
                },
                ..config_named("A")
            },
            "Invalid activity name",
        );
        rejects(
            RunConfiguration {
                launch: RunLaunch::DeepLink {
                    uri: "no scheme".into(),
                },
                ..config_named("D")
            },
            "Invalid deep link",
        );
    }

    #[test]
    fn a_bad_logcat_filter_is_rejected() {
        rejects(
            RunConfiguration {
                logcat_filter: Some("x".repeat(MAX_LOGCAT_FILTER_BYTES + 1)),
                ..config_named("F")
            },
            "too long",
        );
        rejects(
            RunConfiguration {
                logcat_filter: Some("tag:a\ntag:b".into()),
                ..config_named("F")
            },
            "one line",
        );
    }

    // ── Commands ─────────────────────────────────────────────────────────────

    #[test]
    fn save_validates_against_the_projects_modules_and_saves_nothing_when_invalid() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);

        let err = save_at(&path, &root, config("Wear", ":wear"), None).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert_eq!(stored(&path, &project).run_configurations, None);

        let saved = save_at(&path, &root, config("Second", ":app"), None).unwrap();
        let names: Vec<&str> = saved
            .configurations
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, vec![DEFAULT_RUN_CONFIGURATION, "Second"]);
        assert_eq!(saved.local["Second"], LocalRunState::default());
    }

    #[test]
    fn saving_the_same_name_replaces_it() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);

        let replaced = RunConfiguration {
            variant: "release".into(),
            ..config(DEFAULT_RUN_CONFIGURATION, ":app")
        };
        let saved = save_at(&path, &root, replaced.clone(), None).unwrap();

        assert_eq!(saved.configurations, vec![replaced]);
        // The active configuration's variant is the project's last variant.
        assert_eq!(
            stored(&path, &project).last_build_variant.as_deref(),
            Some("release")
        );
    }

    #[test]
    fn configurations_are_capped() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        for i in 1..MAX_RUN_CONFIGURATIONS {
            save_at(&path, &root, config(&format!("C{i}"), ":app"), None).unwrap();
        }
        assert_eq!(
            list_at(&path, &root).unwrap().configurations.len(),
            MAX_RUN_CONFIGURATIONS
        );

        let err = save_at(&path, &root, config("One more", ":app"), None).unwrap_err();

        assert!(err.to_string().contains("at most"), "{err}");
        // Replacing one still works at the cap.
        save_at(&path, &root, config("C1", ":app"), None).unwrap();
    }

    #[test]
    fn delete_and_set_active_keep_the_old_fields_in_step() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        save_at(
            &path,
            &root,
            RunConfiguration {
                variant: "staging".into(),
                ..config("Staging", ":app")
            },
            None,
        )
        .unwrap();

        let active = set_active_at(&path, &root, "Staging").unwrap();
        assert_eq!(active.active.as_deref(), Some("Staging"));
        assert_eq!(
            stored(&path, &project).last_build_variant.as_deref(),
            Some("staging")
        );

        assert!(matches!(
            set_active_at(&path, &root, "Missing"),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            delete_at(&path, &root, "Missing"),
            Err(AppError::NotFound(_))
        ));

        let after = delete_at(&path, &root, "Staging").unwrap();
        assert_eq!(after.active, None);
        assert!(!after.local.contains_key("Staging"));
        // The last variant stays for an older Keynobi.
        assert_eq!(
            stored(&path, &project).last_build_variant.as_deref(),
            Some("staging")
        );
    }

    #[test]
    fn the_variant_picker_edits_the_active_configuration() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        list_at(&path, &root).unwrap();

        settings_manager::set_active_variant_for_project_path(&path, &root, "release").unwrap();

        let listed = list_at(&path, &root).unwrap();
        assert_eq!(listed.configurations[0].variant, "release");
        assert_eq!(
            stored(&path, &project).last_build_variant.as_deref(),
            Some("release")
        );
    }

    #[test]
    fn a_run_records_the_device_it_ran_on() {
        let project = project(&[":app"]);
        let mut old = entry(project.path());
        old.last_device = Some("emulator-5554".into());
        let (_dir, path) = settings_with(vec![old]);
        let root = root_of(&project);

        let recorded =
            record_last_device_at(&path, &root, DEFAULT_RUN_CONFIGURATION, "28151FDH2000Q4")
                .unwrap();

        let local = &recorded.local[DEFAULT_RUN_CONFIGURATION];
        assert_eq!(local.last_device.as_deref(), Some("28151FDH2000Q4"));
        assert_eq!(local.target, TargetPreference::LastUsed);
        // The project's own device selection is left alone.
        assert_eq!(
            stored(&path, &project).last_device.as_deref(),
            Some("emulator-5554")
        );
        assert!(matches!(
            record_last_device_at(&path, &root, "Missing", "emulator-5554"),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            record_last_device_at(&path, &root, DEFAULT_RUN_CONFIGURATION, "-s; reboot"),
            Err(AppError::InvalidInput(_))
        ));
        assert_eq!(
            list_at(&path, &root).unwrap().local[DEFAULT_RUN_CONFIGURATION]
                .last_device
                .as_deref(),
            Some("28151FDH2000Q4")
        );
    }

    #[test]
    fn the_target_is_set_per_configuration_and_validated() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        let avd = TargetPreference::Avd {
            name: "Pixel_7".into(),
        };

        let saved = set_target_at(&path, &root, DEFAULT_RUN_CONFIGURATION, avd.clone()).unwrap();

        assert_eq!(saved.local[DEFAULT_RUN_CONFIGURATION].target, avd);
        assert_eq!(
            list_at(&path, &root).unwrap().local[DEFAULT_RUN_CONFIGURATION].target,
            avd
        );
        for bad in [
            TargetPreference::Serial {
                serial: "x; reboot".into(),
            },
            TargetPreference::Avd {
                name: "-wipe-data".into(),
            },
        ] {
            assert!(matches!(
                set_target_at(&path, &root, DEFAULT_RUN_CONFIGURATION, bad),
                Err(AppError::InvalidInput(_))
            ));
        }
        assert!(matches!(
            set_target_at(&path, &root, "Missing", TargetPreference::Ask),
            Err(AppError::NotFound(_))
        ));
        // A refused target leaves the saved one.
        assert_eq!(
            list_at(&path, &root).unwrap().local[DEFAULT_RUN_CONFIGURATION].target,
            avd
        );
    }

    #[test]
    fn a_settings_ui_snapshot_does_not_clobber_configurations() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        // The settings UI loaded its snapshot before any configuration existed.
        let mut snapshot = settings_manager::load_settings_at_path(&path);
        save_at(&path, &root, config("Mine", ":app"), None).unwrap();

        snapshot.onboarding_completed = true;
        snapshot.recent_projects[0].run_configurations = Some(Vec::new());
        settings_manager::save_settings_snapshot_at_path(&path, &snapshot).unwrap();

        let saved = stored(&path, &project);
        let names: Vec<String> = saved
            .run_configurations
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec![DEFAULT_RUN_CONFIGURATION, "Mine"]);
        assert!(settings_manager::load_settings_at_path(&path).onboarding_completed);
    }

    #[test]
    fn saves_from_two_processes_are_both_kept() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        list_at(&path, &root).unwrap();

        // Another process is in the middle of its own save under the data lock.
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (other_path, other_root) = (path.clone(), root.clone());
        let other = std::thread::spawn(move || {
            settings_manager::mutate_settings_at_path(&other_path, |s| {
                let entry = s
                    .recent_projects
                    .iter_mut()
                    .find(|e| e.path == other_root)
                    .unwrap();
                entry
                    .run_configurations
                    .get_or_insert_with(Vec::new)
                    .push(config("From the other process", ":app"));
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
            .unwrap();
        });
        entered_rx.recv().unwrap();

        let (done_tx, done_rx) = mpsc::channel();
        let (this_path, this_root) = (path.clone(), root.clone());
        let this = std::thread::spawn(move || {
            let saved = save_at(
                &this_path,
                &this_root,
                config("From this one", ":app"),
                None,
            );
            done_tx.send(()).unwrap();
            saved.unwrap();
        });
        assert!(
            done_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "the save must wait for the lock"
        );
        release_tx.send(()).unwrap();
        other.join().unwrap();
        this.join().unwrap();

        let names: Vec<String> = list_at(&path, &root)
            .unwrap()
            .configurations
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            names,
            vec![
                DEFAULT_RUN_CONFIGURATION,
                "From the other process",
                "From this one"
            ]
        );
    }

    // ── Shared with the project ──────────────────────────────────────────────

    use crate::services::shared_run_configurations::SHARED_FILE;

    fn write_shared(project: &TempDir, configurations: &str) {
        write(
            project.path(),
            SHARED_FILE,
            &format!(r#"{{"schemaVersion": 1, "configurations": [{configurations}]}}"#),
        );
    }

    fn shared_text(project: &TempDir) -> Option<String> {
        std::fs::read_to_string(project.path().join(SHARED_FILE)).ok()
    }

    const PHONE: &str = r#"{"name": "Phone", "module": ":app", "variant": "release"}"#;

    #[test]
    fn shared_configurations_follow_the_local_ones_and_a_local_one_of_the_same_name_wins() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        list_at(&path, &root_of(&project)).unwrap();
        write_shared(
            &project,
            &format!(r#"{PHONE}, {{"name": "default", "module": ":app", "variant": "release"}}"#),
        );

        let listed = list_at(&path, &root_of(&project)).unwrap();

        let names: Vec<&str> = listed
            .configurations
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(names, [DEFAULT_RUN_CONFIGURATION, "Phone"]);
        assert_eq!(listed.shared, ["Phone"]);
        let file = listed.shared_file.unwrap();
        assert_eq!(file.path, SHARED_FILE);
        assert!(file.sha256.is_some());
        assert_eq!(file.problems.len(), 1, "{:?}", file.problems);
        assert_eq!(file.problems[0].name.as_deref(), Some("default"));
        assert!(file.problems[0]
            .message
            .contains("Your local configuration 'Default' has the same name and is used instead"));
    }

    #[test]
    fn migration_leaves_a_shared_name_to_the_shared_configuration() {
        let project = project(&[":app"]);
        write_shared(
            &project,
            r#"{"name": "Default", "module": ":app", "variant": "release"}"#,
        );
        let mut fresh = entry(project.path());
        fresh.last_device = Some("emulator-5554".into());
        let (_dir, path) = settings_with(vec![fresh]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert_eq!(
            listed.configurations,
            vec![RunConfiguration {
                variant: "release".into(),
                ..config(DEFAULT_RUN_CONFIGURATION, ":app")
            }]
        );
        assert_eq!(listed.shared, [DEFAULT_RUN_CONFIGURATION]);
        assert_eq!(listed.active.as_deref(), Some(DEFAULT_RUN_CONFIGURATION));
        // Its local state is kept by name.
        assert_eq!(
            listed.local[DEFAULT_RUN_CONFIGURATION]
                .last_device
                .as_deref(),
            Some("emulator-5554")
        );
        assert_eq!(stored(&path, &project).run_configurations, Some(Vec::new()));
    }

    #[test]
    fn shared_configurations_are_read_in_safe_mode() {
        let project = project(&[":app"]);
        write_shared(&project, PHONE);
        let mut untrusted = entry(project.path());
        untrusted.trusted = Some(false);
        let (_dir, path) = settings_with(vec![untrusted]);

        let listed = list_at(&path, &root_of(&project)).unwrap();

        assert_eq!(listed.shared, ["Phone"]);
    }

    #[test]
    fn sharing_moves_a_configuration_into_the_file_and_unsharing_moves_it_back() {
        let project = project(&[":app"]);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        let phone = RunConfiguration {
            launch: RunLaunch::DeepLink {
                uri: "myapp://home".into(),
            },
            ..config("Phone", ":app")
        };
        save_at(&path, &root, phone.clone(), None).unwrap();
        set_target_at(
            &path,
            &root,
            "Phone",
            TargetPreference::Serial {
                serial: "emulator-5554".into(),
            },
        )
        .unwrap();

        let shared = save_at(&path, &root, phone.clone(), Some(true)).unwrap();

        assert_eq!(shared.shared, ["Phone"]);
        assert!(shared.configurations.contains(&phone));
        let text = shared_text(&project).unwrap();
        assert!(text.contains("\"name\": \"Phone\""), "{text}");
        assert!(!text.contains("emulator-5554"), "{text}");
        let local: Vec<String> = stored(&path, &project)
            .run_configurations
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(local, [DEFAULT_RUN_CONFIGURATION]);
        // Its target stays local.
        assert_eq!(
            shared.local["Phone"].target,
            TargetPreference::Serial {
                serial: "emulator-5554".into()
            }
        );

        // Saved again without saying, it stays shared.
        let edited = RunConfiguration {
            variant: "release".into(),
            ..phone.clone()
        };
        let listed = save_at(&path, &root, edited.clone(), None).unwrap();
        assert_eq!(listed.shared, ["Phone"]);
        assert!(shared_text(&project).unwrap().contains("\"release\""));

        let unshared = save_at(&path, &root, edited.clone(), Some(false)).unwrap();

        assert!(unshared.shared.is_empty());
        assert_eq!(unshared.shared_file, None);
        assert!(!project.path().join(".keynobi").exists());
        assert!(stored(&path, &project)
            .run_configurations
            .unwrap()
            .contains(&edited));
        assert_eq!(
            unshared.local["Phone"].target,
            TargetPreference::Serial {
                serial: "emulator-5554".into()
            }
        );
    }

    #[test]
    fn shared_configurations_can_be_chosen_targeted_recorded_and_deleted() {
        let project = project(&[":app"]);
        write_shared(&project, PHONE);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);

        set_active_at(&path, &root, "Phone").unwrap();
        set_target_at(&path, &root, "Phone", TargetPreference::Ask).unwrap();
        let listed = record_last_device_at(&path, &root, "Phone", "emulator-5554").unwrap();
        assert_eq!(listed.active.as_deref(), Some("Phone"));
        assert_eq!(listed.local["Phone"].target, TargetPreference::Ask);
        assert_eq!(
            listed.local["Phone"].last_device.as_deref(),
            Some("emulator-5554")
        );

        let deleted = delete_at(&path, &root, "Phone").unwrap();

        assert!(deleted.shared.is_empty());
        assert_eq!(deleted.active, None);
        assert!(!deleted.local.contains_key("Phone"));
        assert_eq!(shared_text(&project), None);
    }

    #[test]
    fn a_name_used_by_a_shared_configuration_is_taken() {
        let project = project(&[":app"]);
        write_shared(&project, PHONE);
        let (_dir, path) = settings_with(vec![entry(project.path())]);

        let err = save_at(&path, &root_of(&project), config("phone", ":app"), None).unwrap_err();

        assert!(err
            .to_string()
            .contains("A run configuration named 'Phone' already exists."));
    }

    #[test]
    fn a_file_with_a_configuration_this_version_cannot_use_is_left_alone() {
        let project = project(&[":app"]);
        write_shared(
            &project,
            &format!(
                r#"{PHONE}, {{"name": "Future", "module": ":app", "variant": "debug", "profile": 1}}"#
            ),
        );
        let before = shared_text(&project).unwrap();
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        list_at(&path, &root).unwrap();

        for result in [
            save_at(&path, &root, config("Mine", ":app"), Some(true)),
            save_at(&path, &root, config("Phone", ":app"), Some(false)),
            delete_at(&path, &root, "Phone"),
        ] {
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("Fix or remove the file first"),
                "{err}"
            );
        }
        assert_eq!(shared_text(&project).unwrap(), before);
        // The refused unshare kept nothing: "Phone" is still shared only.
        let listed = list_at(&path, &root).unwrap();
        assert_eq!(listed.shared, ["Phone"]);
        assert!(!stored(&path, &project)
            .run_configurations
            .unwrap()
            .iter()
            .any(|c| c.name == "Mine"));
    }

    #[test]
    fn approving_a_shared_configuration_records_the_hash_the_user_reviewed() {
        let project = project(&[":app"]);
        write_shared(&project, PHONE);
        let (_dir, path) = settings_with(vec![entry(project.path())]);
        let root = root_of(&project);
        let sha = list_at(&path, &root)
            .unwrap()
            .shared_file
            .unwrap()
            .sha256
            .unwrap();

        let approved = approve_shared_at(&path, &root, "Phone", &sha).unwrap();
        assert_eq!(
            approved.local["Phone"]
                .approved_project_file_sha256
                .as_deref(),
            Some(sha.as_str())
        );

        let stale = "0".repeat(64);
        let err = approve_shared_at(&path, &root, "Phone", &stale).unwrap_err();
        assert!(
            err.to_string().contains("changed since you reviewed them"),
            "{err}"
        );
        let err = approve_shared_at(&path, &root, "Phone", "not a hash").unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        let err = approve_shared_at(&path, &root, DEFAULT_RUN_CONFIGURATION, &sha).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
    }
}
