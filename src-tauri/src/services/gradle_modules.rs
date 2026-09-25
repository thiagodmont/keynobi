//! Which Gradle modules of a project are Android applications.
//!
//! Nothing may assume the application module is `:app`. The modules come from
//! the settings file's `include` list, and a module is an application when its
//! build file applies `com.android.application`, directly, through a
//! version-catalog alias, or through a convention plugin whose id ends in
//! `android.application`.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Max modules read from a settings file.
pub const MAX_GRADLE_MODULES: usize = 500;

/// Max bytes read from one settings, build, or version-catalog file.
const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;

/// A Gradle project of the build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradleModule {
    /// Gradle path: `:mobile`, `:apps:mobile`, or `:` for the root project.
    pub path: String,
    /// The module's directory.
    pub dir: PathBuf,
}

impl GradleModule {
    /// The module's `build.gradle.kts` and `build.gradle`, in that order.
    pub fn build_files(&self) -> [PathBuf; 2] {
        [
            self.dir.join("build.gradle.kts"),
            self.dir.join("build.gradle"),
        ]
    }

    /// The module directory relative to `gradle_root` (`.` for the root project).
    pub fn relative_dir(&self, gradle_root: &Path) -> String {
        let canonical_root = gradle_root.canonicalize().ok();
        let relative = self.dir.strip_prefix(gradle_root).ok().or_else(|| {
            canonical_root
                .as_deref()
                .and_then(|root| self.dir.strip_prefix(root).ok())
        });
        match relative {
            Some(rel) if rel.as_os_str().is_empty() => ".".to_string(),
            Some(rel) => rel.to_string_lossy().into_owned(),
            None => self.dir.to_string_lossy().into_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleKind {
    Application,
    /// Applies another Android plugin (library, dynamic feature, test).
    OtherAndroid,
    Unknown,
}

/// The project's application modules, sorted by Gradle path.
///
/// When no module is recognised as an application, a module directory named
/// `app` that applies no other Android plugin is used, as before module
/// detection (its plugin may come from a convention plugin with another id).
pub fn application_modules(gradle_root: &Path) -> Vec<GradleModule> {
    let catalog = plugin_catalog(gradle_root);
    let mut apps: Vec<GradleModule> = Vec::new();
    let mut app_dir_kind = None;
    for module in project_modules(gradle_root) {
        let kind = module_kind(gradle_root, &module, &catalog);
        if module.path == ":app" {
            app_dir_kind = Some(kind);
        }
        if kind == ModuleKind::Application {
            apps.push(module);
        }
    }
    if apps.is_empty() {
        let app_dir = gradle_root.join("app");
        let kind = app_dir_kind.unwrap_or_else(|| {
            module_kind(
                gradle_root,
                &GradleModule {
                    path: ":app".into(),
                    dir: app_dir.clone(),
                },
                &catalog,
            )
        });
        if app_dir.is_dir() && kind == ModuleKind::Unknown {
            apps.push(GradleModule {
                path: ":app".into(),
                dir: app_dir,
            });
        }
    }
    apps.sort_by(|a, b| a.path.cmp(&b.path));
    apps
}

/// The application module to use.
///
/// `requested` names a module (`:mobile`, `mobile`) or a task in one
/// (`:mobile:assembleDebug`); it must be one of the application modules.
/// Without it the project must have exactly one application module.
///
/// # Errors
/// A message listing the application modules when there is none, when there
/// are several and none was named, or when the named one is not among them.
pub fn resolve_application_module(
    gradle_root: &Path,
    requested: Option<&str>,
) -> Result<GradleModule, String> {
    let modules = application_modules(gradle_root);
    let listed = || listed_paths(&modules);

    if let Some(requested) = requested.map(str::trim).filter(|r| !r.is_empty()) {
        let path = if requested.starts_with(':') {
            requested.to_string()
        } else {
            format!(":{requested}")
        };
        let task_module = path.rsplit_once(':').map(|(module, _)| module);
        let found = modules.iter().find(|m| m.path == path).or_else(|| {
            modules
                .iter()
                .find(|m| Some(m.path.as_str()) == task_module)
        });
        return match found {
            Some(module) => Ok(module.clone()),
            None if modules.is_empty() => Err(no_application_module(gradle_root)),
            None => Err(format!(
                "'{requested}' does not name an application module of this project. \
                 Application modules: {}.",
                listed()
            )),
        };
    }

    match modules.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(no_application_module(gradle_root)),
        several => Err(several_application_modules(several)),
    }
}

fn listed_paths(modules: &[GradleModule]) -> String {
    modules
        .iter()
        .map(|m| m.path.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn several_application_modules(modules: &[GradleModule]) -> String {
    format!(
        "This project has several application modules ({}), and none was named. Name the \
         module where the call accepts one (for example {}, or a task in it such as \
         {}:assembleDebug); the app cannot choose between them yet.",
        listed_paths(modules),
        modules[0].path,
        modules[0].path
    )
}

fn no_application_module(gradle_root: &Path) -> String {
    format!(
        "No application module found in {}: no module included in the settings file \
         applies com.android.application.",
        gradle_root.display()
    )
}

/// Build files that describe the project's app, relative to `gradle_root`:
/// the application module's, then the root project's. Only the root
/// project's when no application module is found.
///
/// # Errors
/// When the project has several application modules.
pub fn application_build_file_candidates(gradle_root: &Path) -> Result<Vec<String>, String> {
    let mut files: Vec<String> = match application_modules(gradle_root).as_slice() {
        [only] => match only.relative_dir(gradle_root).as_str() {
            "." => Vec::new(),
            dir => vec![
                format!("{dir}/build.gradle.kts"),
                format!("{dir}/build.gradle"),
            ],
        },
        [] => Vec::new(),
        several => return Err(several_application_modules(several)),
    };
    files.extend(["build.gradle.kts".to_string(), "build.gradle".to_string()]);
    Ok(files)
}

/// [`application_build_file_candidates`] joined to `gradle_root`.
pub fn application_build_files(gradle_root: &Path) -> Result<Vec<PathBuf>, String> {
    Ok(application_build_file_candidates(gradle_root)?
        .iter()
        .map(|rel| gradle_root.join(rel))
        .collect())
}

// ── Settings ──────────────────────────────────────────────────────────────────

static RE_PROJECT_DIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"project\(\s*["']([^"']+)["']\s*\)\s*\.projectDir\s*=\s*(?:file\(\s*["']([^"']+)["']\s*\)|(?:new\s+)?File\(\s*(?:rootDir|settingsDir)\s*,\s*["']([^"']+)["']\s*\))"#,
    )
    .expect("static regex")
});

static RE_QUOTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""([^"\n]*)"|'([^'\n]*)'"#).expect("static regex"));

/// The projects the settings file includes, or the root project alone when it
/// includes none. Directories that do not exist, or resolve outside the root,
/// are skipped.
fn project_modules(gradle_root: &Path) -> Vec<GradleModule> {
    let settings = ["settings.gradle.kts", "settings.gradle"]
        .iter()
        .find_map(|name| read_script(gradle_root, &gradle_root.join(name)))
        .map(|text| strip_comments(&text))
        .unwrap_or_default();

    let paths = included_paths(&settings);
    if paths.is_empty() {
        return vec![GradleModule {
            path: ":".into(),
            dir: gradle_root.to_path_buf(),
        }];
    }

    let custom_dirs: Vec<(String, String)> = RE_PROJECT_DIR
        .captures_iter(&settings)
        .filter_map(|c| {
            let dir = c.get(2).or_else(|| c.get(3))?.as_str().to_string();
            Some((normalize_path(&c[1]), dir))
        })
        .collect();

    let Ok(canonical_root) = gradle_root.canonicalize() else {
        return Vec::new();
    };
    paths
        .into_iter()
        .take(MAX_GRADLE_MODULES)
        .filter_map(|path| {
            let relative = custom_dirs
                .iter()
                .rev()
                .find(|(p, _)| *p == path)
                .map(|(_, dir)| PathBuf::from(dir))
                .unwrap_or_else(|| path.trim_start_matches(':').split(':').collect());
            let dir = crate::utils::path::validate_within_root(
                &canonical_root,
                &relative.to_string_lossy(),
            )
            .ok()?;
            dir.is_dir().then_some(GradleModule { path, dir })
        })
        .collect()
}

/// Gradle paths named by `include` statements, normalized to start with `:`.
fn included_paths(settings: &str) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let bytes = settings.as_bytes();
    let mut search = 0;
    while let Some(found) = settings[search..].find("include") {
        let start = search + found;
        let after = start + "include".len();
        search = after;
        let preceded_by_identifier = start > 0
            && (bytes[start - 1].is_ascii_alphanumeric()
                || bytes[start - 1] == b'_'
                || bytes[start - 1] == b'.');
        let rest = &settings[after..];
        let Some(next) = rest.chars().next() else {
            break;
        };
        if preceded_by_identifier || !(next == '(' || next == ' ' || next == '\t') {
            continue;
        }
        let args = include_arguments(rest);
        for c in RE_QUOTED.captures_iter(args) {
            if let Some(m) = c.get(1).or_else(|| c.get(2)) {
                let path = normalize_path(m.as_str());
                if path != ":" && !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

/// The text of one `include` call: up to its closing parenthesis, or for the
/// Groovy form without parentheses, the line and any lines a trailing comma
/// continues it onto.
fn include_arguments(rest: &str) -> &str {
    let trimmed = rest.trim_start_matches([' ', '\t']);
    let offset = rest.len() - trimmed.len();
    if trimmed.starts_with('(') {
        let mut depth = 0usize;
        for (i, ch) in trimmed.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return &rest[offset..offset + i + 1];
                    }
                }
                _ => {}
            }
        }
        return &rest[offset..];
    }
    let mut end = 0;
    for line in rest.split_inclusive('\n') {
        end += line.len();
        if !line.trim_end().ends_with(',') {
            break;
        }
    }
    &rest[..end]
}

fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.starts_with(':') {
        path.to_string()
    } else {
        format!(":{path}")
    }
}

// ── Build files ───────────────────────────────────────────────────────────────

static RE_PLUGIN_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:\bid\s*\(?\s*|\bapply\s+plugin\s*:\s*|\bapply\s*\(\s*plugin\s*=\s*)["']([A-Za-z0-9_.\-]+)["']"#,
    )
    .expect("static regex")
});

static RE_ALIAS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\balias\s*\(\s*[A-Za-z0-9_]+\.plugins\.([A-Za-z0-9_.]+)\s*\)")
        .expect("static regex")
});

fn module_kind(
    gradle_root: &Path,
    module: &GradleModule,
    catalog: &[(String, String)],
) -> ModuleKind {
    let Some(text) = module
        .build_files()
        .iter()
        .find_map(|f| read_script(gradle_root, f))
    else {
        return ModuleKind::Unknown;
    };
    let text = strip_comments(&text);
    let mut ids: Vec<String> = RE_PLUGIN_ID
        .captures_iter(&text)
        .map(|c| c[1].to_string())
        .collect();
    for c in RE_ALIAS.captures_iter(&text) {
        let accessor = catalog_key(&c[1]);
        match catalog.iter().find(|(key, _)| *key == accessor) {
            Some((_, id)) => ids.push(id.clone()),
            None => ids.push(c[1].to_string()),
        }
    }
    let squashed: Vec<String> = ids.iter().map(|id| squash(id)).collect();
    if squashed.iter().any(|id| id.ends_with("androidapplication")) {
        ModuleKind::Application
    } else if squashed.iter().any(|id| {
        id.ends_with("androidlibrary")
            || id.ends_with("androiddynamicfeature")
            || id == "comandroidtest"
            || id.ends_with("androidtest")
    }) {
        ModuleKind::OtherAndroid
    } else {
        ModuleKind::Unknown
    }
}

/// Lowercase with `.`, `-`, and `_` removed, so ids and catalog accessors in
/// any spelling compare equal (`android-application`, `androidApplication`).
fn squash(id: &str) -> String {
    id.chars()
        .filter(|c| !matches!(c, '.' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

/// A version-catalog alias or accessor in the form both compare in.
fn catalog_key(alias: &str) -> String {
    squash(alias)
}

/// `[plugins]` of `gradle/libs.versions.toml`: (alias key, plugin id).
fn plugin_catalog(gradle_root: &Path) -> Vec<(String, String)> {
    let Some(text) = read_script(
        gradle_root,
        &gradle_root.join("gradle").join("libs.versions.toml"),
    ) else {
        return Vec::new();
    };
    static RE_TOML_ID: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^\s*([A-Za-z0-9_.\-]+)\s*=\s*(?:\{[^}]*\bid\s*=\s*"([^"]+)"|"([^":]+))"#)
            .expect("static regex")
    });
    let mut in_plugins = false;
    let mut entries = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_plugins = trimmed == "[plugins]";
            continue;
        }
        if !in_plugins {
            continue;
        }
        if let Some(c) = RE_TOML_ID.captures(line) {
            if let Some(id) = c.get(2).or_else(|| c.get(3)) {
                entries.push((catalog_key(&c[1]), id.as_str().to_string()));
            }
        }
    }
    entries
}

/// Read a settings, build, or catalog file whose canonical path is a regular
/// file inside the canonical `gradle_root`; a symlink that leads out is not
/// read.
fn read_script(gradle_root: &Path, path: &Path) -> Option<String> {
    let canonical = path.canonicalize().ok()?;
    if !canonical.starts_with(gradle_root.canonicalize().ok()?) {
        return None;
    }
    let meta = std::fs::metadata(&canonical).ok()?;
    if !meta.is_file() || meta.len() > MAX_SCRIPT_BYTES {
        return None;
    }
    std::fs::read_to_string(&canonical).ok()
}

/// `text` without `//` and `/* */` comments; string literals are kept whole,
/// so `"https://…"` is not cut.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == q || c == '\n' {
                quote = None;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && next == '/' {
                        break;
                    }
                    prev = next;
                }
            }
            ('"' | '\'', _) => {
                quote = Some(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn paths(root: &Path) -> Vec<String> {
        application_modules(root)
            .into_iter()
            .map(|m| m.path)
            .collect()
    }

    const APP_KTS: &str = "plugins {\n    id(\"com.android.application\")\n}\n";
    const LIB_KTS: &str = "plugins {\n    id(\"com.android.library\")\n}\n";

    #[test]
    fn a_single_application_module_not_named_app_is_found() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "include(\":mobile\")\ninclude(\":core\")\n",
        );
        write(dir.path(), "mobile/build.gradle.kts", APP_KTS);
        write(dir.path(), "core/build.gradle.kts", LIB_KTS);

        let module = resolve_application_module(dir.path(), None).unwrap();

        assert_eq!(module.path, ":mobile");
        assert_eq!(
            module.dir,
            dir.path().canonicalize().unwrap().join("mobile")
        );
    }

    #[test]
    fn a_library_named_app_is_not_the_application_module() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle",
            "include ':app', ':androidApp'\n",
        );
        write(
            dir.path(),
            "app/build.gradle",
            "apply plugin: 'com.android.library'\n",
        );
        write(
            dir.path(),
            "androidApp/build.gradle",
            "plugins {\n    id 'com.android.application'\n}\n",
        );

        assert_eq!(paths(dir.path()), vec![":androidApp"]);
    }

    #[test]
    fn a_library_named_app_alone_is_no_application_module() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "settings.gradle.kts", "include(\":app\")\n");
        write(dir.path(), "app/build.gradle.kts", LIB_KTS);

        let err = resolve_application_module(dir.path(), None).unwrap_err();

        assert!(err.contains("No application module"), "{err}");
    }

    #[test]
    fn several_application_modules_are_an_error_unless_one_is_named() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "include(\n    \":mobile\",\n    \":wear\",\n)\n",
        );
        write(dir.path(), "mobile/build.gradle.kts", APP_KTS);
        write(dir.path(), "wear/build.gradle.kts", APP_KTS);

        let err = resolve_application_module(dir.path(), None).unwrap_err();
        assert!(err.contains(":mobile, :wear"), "{err}");

        for named in [":wear", "wear", ":wear:assembleDebug"] {
            assert_eq!(
                resolve_application_module(dir.path(), Some(named))
                    .unwrap()
                    .path,
                ":wear",
                "{named}"
            );
        }
        let err = resolve_application_module(dir.path(), Some(":tv:assembleDebug")).unwrap_err();
        assert!(err.contains(":mobile, :wear"), "{err}");
    }

    #[test]
    fn a_version_catalog_alias_is_resolved() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "include(\":client\", \":shared\")\n",
        );
        write(
            dir.path(),
            "gradle/libs.versions.toml",
            "[versions]\nagp = \"8.5.0\"\n\n[plugins]\n\
             agp-app = { id = \"com.android.application\", version.ref = \"agp\" }\n\
             agp-lib = { id = \"com.android.library\", version.ref = \"agp\" }\n",
        );
        write(
            dir.path(),
            "client/build.gradle.kts",
            "plugins {\n    alias(libs.plugins.agp.app)\n}\n",
        );
        write(
            dir.path(),
            "shared/build.gradle.kts",
            "plugins {\n    alias(libs.plugins.agp.lib)\n}\n",
        );

        assert_eq!(paths(dir.path()), vec![":client"]);
    }

    #[test]
    fn a_conventional_alias_without_a_catalog_entry_is_recognised() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "settings.gradle.kts", "include(\":mobile\")\n");
        write(
            dir.path(),
            "mobile/build.gradle.kts",
            "plugins {\n    alias(libs.plugins.android.application)\n    \
             alias(libs.plugins.kotlin.android)\n}\n",
        );

        assert_eq!(paths(dir.path()), vec![":mobile"]);
    }

    #[test]
    fn a_convention_plugin_id_is_recognised() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "include(\":app-mobile\")\n",
        );
        write(
            dir.path(),
            "app-mobile/build.gradle.kts",
            "plugins {\n    id(\"acme.android.application\")\n}\n",
        );

        assert_eq!(paths(dir.path()), vec![":app-mobile"]);
    }

    #[test]
    fn nested_and_relocated_modules_use_their_directories() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "include(\":apps:phone\")\ninclude(\":tv\")\n\
             project(\":tv\").projectDir = file(\"platforms/tv\")\n",
        );
        write(dir.path(), "apps/phone/build.gradle.kts", APP_KTS);
        write(dir.path(), "platforms/tv/build.gradle.kts", APP_KTS);
        let root = dir.path().canonicalize().unwrap();

        let modules = application_modules(dir.path());

        assert_eq!(
            modules,
            vec![
                GradleModule {
                    path: ":apps:phone".into(),
                    dir: root.join("apps").join("phone"),
                },
                GradleModule {
                    path: ":tv".into(),
                    dir: root.join("platforms").join("tv"),
                },
            ]
        );
    }

    #[test]
    fn commented_out_includes_and_plugins_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "settings.gradle.kts",
            "// include(\":old\")\n/* include(\":older\") */\ninclude(\":mobile\")\n\
             includeBuild(\"build-logic\")\n",
        );
        write(dir.path(), "old/build.gradle.kts", APP_KTS);
        write(dir.path(), "older/build.gradle.kts", APP_KTS);
        write(dir.path(), "build-logic/build.gradle.kts", APP_KTS);
        write(
            dir.path(),
            "mobile/build.gradle.kts",
            "plugins {\n    // id(\"com.android.library\")\n    id(\"com.android.application\")\n}\n\
             val url = \"https://example.com\" // a comment\n",
        );

        assert_eq!(paths(dir.path()), vec![":mobile"]);
    }

    #[test]
    fn a_module_directory_outside_the_root_is_skipped() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("project");
        write(
            &root,
            "settings.gradle.kts",
            "include(\":escape\")\nproject(\":escape\").projectDir = file(\"../elsewhere\")\n",
        );
        write(outer.path(), "elsewhere/build.gradle.kts", APP_KTS);

        assert!(application_modules(&root).is_empty());
    }

    #[test]
    fn a_single_module_project_is_its_own_application() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "build.gradle",
            "apply plugin: 'com.android.application'\n",
        );

        assert_eq!(paths(dir.path()), vec![":"]);
    }

    #[test]
    fn an_unrecognised_app_directory_is_still_used() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "settings.gradle.kts", "include(\":app\")\n");
        write(
            dir.path(),
            "app/build.gradle.kts",
            "plugins {\n    id(\"acme.mobile\")\n}\n",
        );

        assert_eq!(paths(dir.path()), vec![":app"]);
    }

    #[test]
    fn application_build_files_come_from_the_application_module() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "settings.gradle.kts", "include(\":mobile\")\n");
        write(dir.path(), "mobile/build.gradle.kts", APP_KTS);

        assert_eq!(
            application_build_file_candidates(dir.path()).unwrap(),
            vec![
                "mobile/build.gradle.kts",
                "mobile/build.gradle",
                "build.gradle.kts",
                "build.gradle"
            ]
        );
        assert_eq!(
            application_build_files(dir.path()).unwrap()[0],
            dir.path().join("mobile/build.gradle.kts")
        );
    }

    #[cfg(unix)]
    #[test]
    fn build_files_outside_the_root_are_not_read() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("project");
        write(&root, "settings.gradle.kts", "include(\":app\")\n");
        write(outer.path(), "elsewhere/build.gradle.kts", APP_KTS);
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::os::unix::fs::symlink(
            outer.path().join("elsewhere/build.gradle.kts"),
            root.join("app/build.gradle.kts"),
        )
        .unwrap();

        assert_eq!(
            module_kind(
                &root,
                &GradleModule {
                    path: ":app".into(),
                    dir: root.join("app"),
                },
                &[],
            ),
            ModuleKind::Unknown
        );
    }
}
