use std::path::{Path, PathBuf};

const KNOWN_BUILD_TYPES: &[&str] = &["debug", "release", "staging", "benchmark", "profile"];

#[derive(Debug, serde::Serialize)]
pub struct BuildConfig {
    pub module: String,
    pub file: String,
    pub compile_sdk: Option<i64>,
    pub min_sdk: Option<i64>,
    pub target_sdk: Option<i64>,
    pub application_id: Option<String>,
    pub namespace: Option<String>,
    pub build_types: Vec<BuildType>,
    pub product_flavors: Vec<ProductFlavor>,
}

#[derive(Debug, serde::Serialize)]
pub struct BuildType {
    pub name: String,
    pub minify_enabled: Option<bool>,
    pub debuggable: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct ProductFlavor {
    pub name: String,
    pub dimension: Option<String>,
}

pub fn parse_build_config(gradle_root: &Path, module: &str) -> Result<BuildConfig, String> {
    let module_dir = gradle_root.join(module);
    if !module_dir.is_dir() {
        return Err(format!(
            "Module '{}' not found under {}",
            module,
            gradle_root.display()
        ));
    }

    let (file_path, content) = read_build_gradle(&module_dir)?;
    let relative = file_path
        .strip_prefix(gradle_root)
        .unwrap_or(&file_path)
        .to_string_lossy()
        .to_string();

    let compile_sdk = extract_int_value(&content, &["compileSdk", "compileSdkVersion"]);
    let min_sdk = extract_int_value(&content, &["minSdk", "minSdkVersion"]);
    let target_sdk = extract_int_value(&content, &["targetSdk", "targetSdkVersion"]);

    let conv = if compile_sdk.is_none() || min_sdk.is_none() || target_sdk.is_none() {
        sdk_from_convention_plugins(gradle_root)
    } else {
        (None, None, None)
    };

    let build_types = parse_build_types(&content);
    let product_flavors = {
        let from_file = parse_product_flavors(&content);
        if from_file.is_empty() {
            flavors_from_apk_output(gradle_root, module, &build_types)
        } else {
            from_file
        }
    };
    Ok(BuildConfig {
        module: module.to_string(),
        file: relative,
        compile_sdk: compile_sdk.or(conv.0),
        min_sdk: min_sdk.or(conv.1),
        target_sdk: target_sdk.or(conv.2),
        application_id: extract_string_value(&content, "applicationId"),
        namespace: extract_string_value(&content, "namespace"),
        build_types,
        product_flavors,
    })
}

/// Recursively visit every `.kt` file under `dir`, calling `f` with its text content.
fn walk_kt_files(dir: &Path, f: &mut impl FnMut(&str)) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_kt_files(&path, f);
        } else if path.extension().and_then(|e| e.to_str()) == Some("kt") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                f(&content);
            }
        }
    }
}

/// Scan `{gradle_root}/build-logic/` for SDK levels defined in convention plugin `.kt` files.
/// Returns `(compile_sdk, min_sdk, target_sdk)`.
fn sdk_from_convention_plugins(gradle_root: &Path) -> (Option<i64>, Option<i64>, Option<i64>) {
    let build_logic = gradle_root.join("build-logic");
    if !build_logic.is_dir() {
        return (None, None, None);
    }

    let mut compile_sdk: Option<i64> = None;
    let mut min_sdk: Option<i64> = None;
    let mut target_sdk: Option<i64> = None;

    walk_kt_files(&build_logic, &mut |content| {
        if compile_sdk.is_none() {
            compile_sdk = extract_int_value(content, &["compileSdk", "compileSdkVersion"]);
        }
        if min_sdk.is_none() {
            min_sdk = extract_int_value(content, &["minSdk", "minSdkVersion"]);
        }
        if target_sdk.is_none() {
            target_sdk = extract_int_value(content, &["targetSdk", "targetSdkVersion"]);
        }
    });

    (compile_sdk, min_sdk, target_sdk)
}

/// Infer product flavors from AGP's APK output directory structure.
/// Looks at immediate subdirectories of `{gradle_root}/{module}/build/outputs/apk/`
/// and returns any that are not standard build type names or already-parsed build types.
fn flavors_from_apk_output(
    gradle_root: &Path,
    module: &str,
    parsed_build_types: &[BuildType],
) -> Vec<ProductFlavor> {
    let apk_dir = gradle_root
        .join(module)
        .join("build")
        .join("outputs")
        .join("apk");
    if !apk_dir.is_dir() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&apk_dir) else {
        return Vec::new();
    };

    let mut flavors = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            continue;
        };
        // Exclude statically-known AGP build types AND any build types parsed from the file
        if KNOWN_BUILD_TYPES.contains(&name.as_str()) {
            continue;
        }
        if parsed_build_types.iter().any(|bt| bt.name == name) {
            continue;
        }
        flavors.push(ProductFlavor {
            name,
            dimension: None,
        });
    }

    flavors.sort_by(|a, b| a.name.cmp(&b.name));
    flavors
}

fn read_build_gradle(module_dir: &Path) -> Result<(PathBuf, String), String> {
    for name in &["build.gradle.kts", "build.gradle"] {
        let p = module_dir.join(name);
        if p.is_file() {
            let content = std::fs::read_to_string(&p)
                .map_err(|e| format!("Failed to read {}: {e}", p.display()))?;
            return Ok((p, content));
        }
    }
    Err(format!(
        "No build.gradle(.kts) found in {}",
        module_dir.display()
    ))
}

/// Extract an integer value for any of the given key names.
/// Matches: `compileSdk = 35`, `compileSdkVersion(35)`, `compileSdkVersion = 35`
/// Also finds the key when it appears mid-line, e.g. `defaultConfig { minSdk = 24 }`.
#[allow(clippy::manual_strip)]
fn extract_int_value(content: &str, keys: &[&str]) -> Option<i64> {
    for key in keys {
        for line in content.lines() {
            let trimmed = line.trim();
            // Find all occurrences of the key in the line (typically just one).
            let mut search_from = 0;
            while let Some(offset) = trimmed[search_from..].find(key) {
                let abs = search_from + offset;
                // Guard: char before must not be alphanumeric or '_' (no prefix)
                if abs > 0 {
                    let prev = trimmed[..abs].chars().next_back();
                    if matches!(prev, Some(c) if c.is_alphanumeric() || c == '_') {
                        search_from = abs + key.len();
                        continue;
                    }
                }
                // Guard: char immediately after the key must not be alphanumeric or '_'
                let after = trimmed[abs + key.len()..].chars().next();
                if matches!(after, Some(c) if c.is_alphanumeric() || c == '_') {
                    search_from = abs + key.len();
                    continue;
                }
                let rest = trimmed[abs + key.len()..].trim_start();
                let digits: String = rest
                    .trim_start_matches(['=', '(', ' '])
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(n) = digits.parse::<i64>() {
                    return Some(n);
                }
                search_from = abs + key.len();
            }
        }
    }
    None
}

/// Extract a quoted string value: `applicationId = "com.example"` or `applicationId "com.example"`
#[allow(clippy::manual_strip)]
fn extract_string_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(key) {
            // Guard: next char must be a separator, not part of a longer identifier
            let after = trimmed[key.len()..].chars().next();
            if matches!(after, Some(c) if c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = trimmed[key.len()..].trim_start();
            let rest = rest.trim_start_matches(['=', ' ']);
            if let Some(start) = rest.find('"') {
                if let Some(end) = rest[start + 1..].find('"') {
                    return Some(rest[start + 1..start + 1 + end].to_string());
                }
            }
        }
    }
    None
}

/// Extract the content inside a top-level named block: `buildTypes { ... }`.
fn extract_block<'a>(content: &'a str, block_name: &str) -> Option<&'a str> {
    let marker = format!("{} {{", block_name);
    let alt_marker = format!("{}{{", block_name);
    let start_pos = content
        .find(&marker)
        .or_else(|| content.find(&alt_marker))?;
    let brace_start = content[start_pos..].find('{')? + start_pos;
    let inner_start = brace_start + 1;

    let mut depth = 1usize;
    let mut pos = inner_start;
    for c in content[inner_start..].chars() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&content[inner_start..pos]);
                }
            }
            _ => {}
        }
        pos += c.len_utf8();
    }
    None
}

fn parse_build_types(content: &str) -> Vec<BuildType> {
    let block = match extract_block(content, "buildTypes") {
        Some(b) => b,
        None => return Vec::new(),
    };

    parse_named_blocks(block)
        .into_iter()
        .map(|(name, inner)| BuildType {
            name,
            minify_enabled: extract_bool_value(&inner, &["isMinifyEnabled", "minifyEnabled"]),
            debuggable: extract_bool_value(&inner, &["isDebuggable", "debuggable"]),
        })
        .collect()
}

fn parse_product_flavors(content: &str) -> Vec<ProductFlavor> {
    let block = match extract_block(content, "productFlavors") {
        Some(b) => b,
        None => return Vec::new(),
    };

    parse_named_blocks(block)
        .into_iter()
        .map(|(name, inner)| ProductFlavor {
            name,
            dimension: extract_string_value(&inner, "dimension"),
        })
        .collect()
}

/// Parse immediate child blocks from a block body.
/// Handles: `debug { ... }` and `create("release") { ... }`
fn parse_named_blocks(block: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut pos = 0;
    let bytes = block.as_bytes();

    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let word_start = pos;
        while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
            pos += 1;
        }
        if pos == word_start {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        let word = &block[word_start..pos];

        while pos < bytes.len() && bytes[pos] == b' ' {
            pos += 1;
        }

        let name = if pos < bytes.len() && bytes[pos] == b'(' {
            if let Some(q_start) = block[pos..].find('"') {
                let abs_start = pos + q_start + 1;
                if let Some(q_end) = block[abs_start..].find('"') {
                    let n = block[abs_start..abs_start + q_end].to_string();
                    pos = abs_start + q_end + 1;
                    while pos < bytes.len() && bytes[pos] != b'{' {
                        pos += 1;
                    }
                    n
                } else {
                    word.to_string()
                }
            } else {
                word.to_string()
            }
        } else {
            while pos < bytes.len() && bytes[pos] != b'{' && bytes[pos] != b'\n' {
                pos += 1;
            }
            if pos >= bytes.len() || bytes[pos] == b'\n' {
                continue;
            }
            word.to_string()
        };

        if pos >= bytes.len() || bytes[pos] != b'{' {
            continue;
        }

        let inner_start = pos + 1;
        let mut depth = 1usize;
        pos = inner_start;
        for c in block[inner_start..].chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            pos += c.len_utf8();
        }
        let inner = &block[inner_start..pos];
        pos += 1;

        if !matches!(
            name.as_str(),
            "getByName" | "maybeCreate" | "all" | "configureEach"
        ) {
            result.push((name, inner.to_string()));
        }
    }
    result
}

#[allow(clippy::manual_strip)]
fn extract_bool_value(content: &str, keys: &[&str]) -> Option<bool> {
    for key in keys {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(key) {
                // Guard: next char must be a separator, not part of a longer identifier
                let after = trimmed[key.len()..].chars().next();
                if matches!(after, Some(c) if c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = trimmed[key.len()..].trim();
                let rest = rest.trim_start_matches(['=', ' ']);
                if rest.starts_with("true") {
                    return Some(true);
                }
                if rest.starts_with("false") {
                    return Some(false);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_project(module: &str, content: &str) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join(module);
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(mod_dir.join("build.gradle.kts"), content).unwrap();
        dir
    }

    #[test]
    fn extract_sdk_levels_kts() {
        let dir = make_project(
            "app",
            r#"
android {
    compileSdk = 35
    defaultConfig {
        applicationId = "com.example.app"
        minSdk = 24
        targetSdk = 35
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.compile_sdk, Some(35));
        assert_eq!(cfg.min_sdk, Some(24));
        assert_eq!(cfg.target_sdk, Some(35));
        assert_eq!(cfg.application_id.as_deref(), Some("com.example.app"));
    }

    #[test]
    fn extract_sdk_levels_groovy() {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join("app");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(
            mod_dir.join("build.gradle"),
            r#"
android {
    compileSdkVersion 33
    defaultConfig {
        applicationId "com.example.groovy"
        minSdkVersion 21
        targetSdkVersion 33
    }
}
"#,
        )
        .unwrap();
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.compile_sdk, Some(33));
        assert_eq!(cfg.min_sdk, Some(21));
        assert_eq!(cfg.application_id.as_deref(), Some("com.example.groovy"));
    }

    #[test]
    fn parse_build_types_groovy_style() {
        let dir = make_project(
            "app",
            r#"
android {
    buildTypes {
        release {
            minifyEnabled true
            debuggable false
        }
        debug {
            minifyEnabled false
            debuggable true
        }
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        let release = cfg
            .build_types
            .iter()
            .find(|b| b.name == "release")
            .unwrap();
        assert_eq!(release.minify_enabled, Some(true));
        assert_eq!(release.debuggable, Some(false));
        let debug = cfg.build_types.iter().find(|b| b.name == "debug").unwrap();
        assert_eq!(debug.minify_enabled, Some(false));
        assert_eq!(debug.debuggable, Some(true));
    }

    #[test]
    fn parse_build_types_kts_style() {
        let dir = make_project(
            "app",
            r#"
android {
    buildTypes {
        release {
            isMinifyEnabled = true
            isDebuggable = false
        }
        debug {
            isMinifyEnabled = false
            isDebuggable = true
        }
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        let release = cfg
            .build_types
            .iter()
            .find(|b| b.name == "release")
            .unwrap();
        assert_eq!(release.minify_enabled, Some(true));
    }

    #[test]
    fn parse_product_flavors() {
        let dir = make_project(
            "app",
            r#"
android {
    flavorDimensions "tier"
    productFlavors {
        free {
            dimension "tier"
        }
        paid {
            dimension "tier"
        }
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.product_flavors.len(), 2);
        let free = cfg
            .product_flavors
            .iter()
            .find(|f| f.name == "free")
            .unwrap();
        assert_eq!(free.dimension.as_deref(), Some("tier"));
    }

    #[test]
    fn missing_module_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = parse_build_config(dir.path(), "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn no_gradle_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("app")).unwrap();
        let result = parse_build_config(dir.path(), "app");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No build.gradle"));
    }

    #[test]
    fn missing_fields_return_null() {
        let dir = make_project("app", "android { }");
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert!(cfg.compile_sdk.is_none());
        assert!(cfg.min_sdk.is_none());
        assert!(cfg.application_id.is_none());
        assert!(cfg.build_types.is_empty());
        assert!(cfg.product_flavors.is_empty());
    }

    #[test]
    fn extract_string_value_does_not_match_longer_key() {
        let dir = make_project(
            "app",
            r#"
android {
    defaultConfig {
        applicationIdSuffix ".debug"
        applicationId = "com.example.app"
    }
}
"#,
        );
        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(cfg.application_id.as_deref(), Some("com.example.app"));
    }

    #[test]
    fn sdk_levels_from_convention_plugin_kt() {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join("app");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(
            mod_dir.join("build.gradle.kts"),
            r#"
android {
    defaultConfig {
        applicationId = "com.example.app"
    }
}
"#,
        )
        .unwrap();
        let kt_dir = dir
            .path()
            .join("build-logic")
            .join("convention")
            .join("src")
            .join("main")
            .join("kotlin");
        fs::create_dir_all(&kt_dir).unwrap();
        fs::write(
            kt_dir.join("KotlinAndroid.kt"),
            r#"
fun configureKotlinAndroid(extension: CommonExtension<*, *, *, *, *, *>) {
    extension.apply {
        compileSdk = 36
        defaultConfig {
            minSdk = 23
        }
    }
}
"#,
        )
        .unwrap();

        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(
            cfg.compile_sdk,
            Some(36),
            "compileSdk should come from convention plugin"
        );
        assert_eq!(
            cfg.min_sdk,
            Some(23),
            "minSdk should come from convention plugin"
        );
        assert_eq!(cfg.application_id.as_deref(), Some("com.example.app"));
    }

    #[test]
    fn sdk_levels_from_build_file_take_precedence_over_convention() {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join("app");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(
            mod_dir.join("build.gradle.kts"),
            r#"
android {
    compileSdk = 35
    defaultConfig { minSdk = 24 }
}
"#,
        )
        .unwrap();
        let kt_dir = dir.path().join("build-logic");
        fs::create_dir_all(&kt_dir).unwrap();
        fs::write(
            kt_dir.join("KotlinAndroid.kt"),
            "compileSdk = 36\nminSdk = 21\n",
        )
        .unwrap();

        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert_eq!(
            cfg.compile_sdk,
            Some(35),
            "module-level compileSdk must win"
        );
        assert_eq!(cfg.min_sdk, Some(24), "module-level minSdk must win");
    }

    #[test]
    fn product_flavors_from_apk_output_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join("app");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(
            mod_dir.join("build.gradle.kts"),
            r#"
android {
    buildTypes { debug {}; release {} }
}
"#,
        )
        .unwrap();
        // Simulate AGP build output: flavors appear as subdirs
        for flavor in &["demo", "prod"] {
            for bt in &["debug", "release"] {
                let apk_dir = dir
                    .path()
                    .join("app")
                    .join("build")
                    .join("outputs")
                    .join("apk")
                    .join(flavor)
                    .join(bt);
                fs::create_dir_all(&apk_dir).unwrap();
                fs::write(apk_dir.join(format!("app-{flavor}-{bt}.apk")), b"fake").unwrap();
            }
        }

        let cfg = parse_build_config(dir.path(), "app").unwrap();
        let names: Vec<&str> = cfg
            .product_flavors
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(names.contains(&"demo"), "expected demo flavor: {names:?}");
        assert!(names.contains(&"prod"), "expected prod flavor: {names:?}");
        assert_eq!(cfg.product_flavors.len(), 2);
    }

    #[test]
    fn apk_output_does_not_treat_build_types_as_flavors() {
        let dir = tempfile::tempdir().unwrap();
        let mod_dir = dir.path().join("app");
        fs::create_dir_all(&mod_dir).unwrap();
        fs::write(mod_dir.join("build.gradle.kts"), "android {}").unwrap();
        // Only build-type directories — no flavors
        for bt in &["debug", "release"] {
            let apk_dir = dir
                .path()
                .join("app")
                .join("build")
                .join("outputs")
                .join("apk")
                .join(bt);
            fs::create_dir_all(&apk_dir).unwrap();
        }

        let cfg = parse_build_config(dir.path(), "app").unwrap();
        assert!(
            cfg.product_flavors.is_empty(),
            "build-type dirs must not become flavors"
        );
    }
}
