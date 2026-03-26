/// workspace_json.rs
///
/// Generates a `workspace.json` for the Kotlin LSP by scanning the Gradle
/// project structure from the filesystem.  This is the **IDE-side** fix for
/// Android multi-module projects where the LSP's own Gradle import reports
/// "empty set of source sets" for every module — a known limitation when
/// modules use AGP convention plugins from a composite build.
///
/// When `workspace.json` exists in the workspace root, the kotlin-lsp's
/// `JsonWorkspaceImporter` uses it **instead of** the Gradle import, so it
/// learns the correct source roots without needing to run Gradle itself.
///
/// Format reverse-engineered from the LSP's `language-server.workspace-import`
/// JAR (`com.jetbrains.ls.imports.json` package, kotlinx.serialization data
/// classes):
///
///   WorkspaceData   → { modules, libraries, sdks, kotlinSettings, javaSettings }
///   ModuleData      → { name, type, dependencies, contentRoots, facets }
///   ContentRootData → { path, excludedPatterns, excludedUrls, sourceRoots }
///   SourceRootData  → { path, type }
///
/// Source root type values (from IdeaProjectMapper string constants):
///   "java"               – main Java/Kotlin source root (src/main/java, src/main/kotlin)
///   "java-resource"      – main resources (src/main/res, src/main/resources)
///   "java-test"          – test sources (src/test/java, src/test/kotlin)
///   "java-test-resource" – test resources (src/test/resources)

use serde::Serialize;
use std::path::{Path, PathBuf};

// ── JSON schema types (must match kotlinx.serialization field names) ──────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceData {
    modules: Vec<ModuleData>,
    libraries: Vec<serde_json::Value>,
    sdks: Vec<serde_json::Value>,
    kotlin_settings: Vec<serde_json::Value>,
    java_settings: Vec<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleData {
    name: String,
    #[serde(rename = "type")]
    module_type: String,
    dependencies: Vec<serde_json::Value>,
    content_roots: Vec<ContentRootData>,
    facets: Vec<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentRootData {
    path: String,
    excluded_patterns: Vec<String>,
    excluded_urls: Vec<String>,
    source_roots: Vec<SourceRootData>,
}

#[derive(Serialize)]
struct SourceRootData {
    path: String,
    #[serde(rename = "type")]
    root_type: String,
}

// ── Source root type constants ────────────────────────────────────────────────

const TYPE_JAVA: &str = "java";
const TYPE_JAVA_RESOURCE: &str = "java-resource";
const TYPE_JAVA_TEST: &str = "java-test";
const TYPE_JAVA_TEST_RESOURCE: &str = "java-test-resource";

/// Standard Android/Gradle source directories that we scan.
/// Each entry is `(relative_path_within_module, source_root_type)`.
const STANDARD_SOURCE_DIRS: &[(&str, &str)] = &[
    ("src/main/java",        TYPE_JAVA),
    ("src/main/kotlin",      TYPE_JAVA),
    ("src/main/resources",   TYPE_JAVA_RESOURCE),
    ("src/main/res",         TYPE_JAVA_RESOURCE),
    ("src/test/java",        TYPE_JAVA_TEST),
    ("src/test/kotlin",      TYPE_JAVA_TEST),
    ("src/test/resources",   TYPE_JAVA_TEST_RESOURCE),
    ("src/androidTest/java", TYPE_JAVA_TEST),
    ("src/androidTest/kotlin", TYPE_JAVA_TEST),
    // Also handle older-style src/ directly (rare, but possible)
    ("src",                  TYPE_JAVA),
];

// ── Module discovery ──────────────────────────────────────────────────────────

/// Parse all `include(":")` calls from a `settings.gradle.kts` (or
/// `settings.gradle`) file.  Returns the raw Gradle module paths, e.g.
/// `[":app", ":features:home", ":core:navigation"]`.
///
/// Handles both single and double quotes and optional whitespace.
fn parse_included_modules(settings_content: &str) -> Vec<String> {
    let mut modules = Vec::new();

    for line in settings_content.lines() {
        let trimmed = line.trim();
        // Match:  include(":foo:bar")  or  include ':foo:bar'
        if !trimmed.starts_with("include") {
            continue;
        }
        // Extract everything between quotes after "include"
        let mut remaining = trimmed
            .trim_start_matches("include")
            .trim_start_matches(|c: char| c.is_whitespace() || c == '(');

        // Strip leading quote
        let quote = if remaining.starts_with('"') {
            Some('"')
        } else if remaining.starts_with('\'') {
            Some('\'')
        } else {
            None
        };

        if let Some(q) = quote {
            remaining = &remaining[1..];
            if let Some(end) = remaining.find(q) {
                let module_path = remaining[..end].to_string();
                if module_path.starts_with(':') {
                    modules.push(module_path);
                }
            }
        }
    }

    modules
}

/// Convert a Gradle module path like `:features:home` to a relative filesystem
/// path like `features/home`.
fn gradle_path_to_rel_dir(gradle_path: &str) -> String {
    gradle_path.trim_start_matches(':').replace(':', "/")
}

/// Find all source root directories that exist inside `module_dir`.
fn find_source_roots(module_dir: &Path) -> Vec<SourceRootData> {
    let mut roots = Vec::new();

    for (rel, root_type) in STANDARD_SOURCE_DIRS {
        let candidate = module_dir.join(rel);
        if candidate.is_dir() {
            roots.push(SourceRootData {
                path: candidate.to_string_lossy().to_string(),
                root_type: root_type.to_string(),
            });
        }
    }

    roots
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate `workspace.json` in `workspace_root` by scanning the project's
/// Gradle module structure from the filesystem.  The generated file tells the
/// kotlin-lsp exactly where each module's source roots are, bypassing the
/// Gradle import that fails for Android projects using AGP convention plugins
/// from a composite build.
///
/// Returns the absolute path of the written file.
pub fn generate(workspace_root: &Path) -> Result<PathBuf, String> {
    // Find the settings file.
    let settings_kts = workspace_root.join("settings.gradle.kts");
    let settings_groovy = workspace_root.join("settings.gradle");
    let settings_path = if settings_kts.is_file() {
        settings_kts
    } else if settings_groovy.is_file() {
        settings_groovy
    } else {
        return Err("No settings.gradle(.kts) found in workspace root".into());
    };

    let settings_content = std::fs::read_to_string(&settings_path)
        .map_err(|e| format!("Failed to read {}: {e}", settings_path.display()))?;

    let gradle_modules = parse_included_modules(&settings_content);

    if gradle_modules.is_empty() {
        return Err("No modules found in settings.gradle(.kts)".into());
    }

    tracing::info!(
        "workspace.json: found {} modules in {}",
        gradle_modules.len(),
        settings_path.display()
    );

    let mut modules: Vec<ModuleData> = Vec::new();

    for gradle_path in &gradle_modules {
        let rel_dir = gradle_path_to_rel_dir(gradle_path);
        let module_dir = workspace_root.join(&rel_dir);

        if !module_dir.is_dir() {
            tracing::warn!(
                "workspace.json: module {} → {} not found, skipping",
                gradle_path,
                module_dir.display()
            );
            continue;
        }

        let source_roots = find_source_roots(&module_dir);

        // Build the content root for this module.  The content root path is
        // the module directory itself; source roots are nested inside it.
        let content_root = ContentRootData {
            path: module_dir.to_string_lossy().to_string(),
            excluded_patterns: vec![
                "build/**".to_string(),
                ".gradle/**".to_string(),
            ],
            excluded_urls: vec![],
            source_roots,
        };

        // Use the Gradle path as the module name (e.g. ":features:home").
        // The LSP uses this for cross-module dependency resolution.
        let module_name = gradle_path.trim_start_matches(':').replace(':', ".");

        modules.push(ModuleData {
            name: module_name,
            module_type: "JAVA_MODULE".to_string(),
            dependencies: vec![],
            content_roots: vec![content_root],
            facets: vec![],
        });
    }

    tracing::info!(
        "workspace.json: {} modules with source roots",
        modules.iter().filter(|m| !m.content_roots[0].source_roots.is_empty()).count()
    );

    let workspace_data = WorkspaceData {
        modules,
        libraries: vec![],
        sdks: vec![],
        kotlin_settings: vec![],
        java_settings: vec![],
    };

    let json = serde_json::to_string_pretty(&workspace_data)
        .map_err(|e| format!("JSON serialization failed: {e}"))?;

    let output_path = workspace_root.join("workspace.json");
    std::fs::write(&output_path, json)
        .map_err(|e| format!("Failed to write workspace.json: {e}"))?;

    tracing::info!("workspace.json written to {}", output_path.display());

    Ok(output_path)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kts_includes() {
        let content = r#"
rootProject.name = "My App"
include(":app")
include(":features:home")
include(":features:auth:login")
include(":core:navigation-api")
        "#;
        let modules = parse_included_modules(content);
        assert_eq!(modules, vec![
            ":app",
            ":features:home",
            ":features:auth:login",
            ":core:navigation-api",
        ]);
    }

    #[test]
    fn parses_groovy_includes_with_single_quotes() {
        let content = r#"
include ':app'
include ':core:ui'
        "#;
        let modules = parse_included_modules(content);
        assert_eq!(modules, vec![":app", ":core:ui"]);
    }

    #[test]
    fn ignores_non_include_lines() {
        let content = r#"
pluginManagement { }
dependencyResolutionManagement { }
rootProject.name = "App"
include(":app")
        "#;
        let modules = parse_included_modules(content);
        assert_eq!(modules, vec![":app"]);
    }

    #[test]
    fn converts_gradle_path_to_dir() {
        assert_eq!(gradle_path_to_rel_dir(":features:home"), "features/home");
        assert_eq!(gradle_path_to_rel_dir(":app"), "app");
        assert_eq!(gradle_path_to_rel_dir(":features:auth:login"), "features/auth/login");
        assert_eq!(gradle_path_to_rel_dir(":core:navigation-api"), "core/navigation-api");
    }

    #[test]
    fn finds_source_roots_in_android_module() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // Create standard Android source dirs
        std::fs::create_dir_all(dir.join("src/main/java")).unwrap();
        std::fs::create_dir_all(dir.join("src/main/kotlin")).unwrap();
        std::fs::create_dir_all(dir.join("src/main/res")).unwrap();
        std::fs::create_dir_all(dir.join("src/test/java")).unwrap();

        let roots = find_source_roots(dir);
        let paths: Vec<_> = roots.iter().map(|r| r.root_type.as_str()).collect();

        assert!(paths.contains(&"java"), "should have java source root");
        assert!(paths.contains(&"java-resource"), "should have resource root");
        assert!(paths.contains(&"java-test"), "should have test root");
    }

    #[test]
    fn generates_workspace_json_for_multi_module_project() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path();

        // Settings
        std::fs::write(
            workspace.join("settings.gradle.kts"),
            r#"include(":app")
include(":features:home")
include(":core:ui")"#,
        ).unwrap();

        // Module dirs
        for module in &["app", "features/home", "core/ui"] {
            let src = workspace.join(module).join("src/main/java");
            std::fs::create_dir_all(&src).unwrap();
        }

        generate(workspace).unwrap();

        let content = std::fs::read_to_string(workspace.join("workspace.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

        let modules = parsed["modules"].as_array().unwrap();
        assert_eq!(modules.len(), 3);

        let home = modules.iter().find(|m| m["name"] == "features.home").unwrap();
        let content_root = &home["contentRoots"][0];
        let source_root = &content_root["sourceRoots"][0];
        assert_eq!(source_root["type"], "java");
        assert!(source_root["path"].as_str().unwrap().ends_with("src/main/java"));
    }
}
