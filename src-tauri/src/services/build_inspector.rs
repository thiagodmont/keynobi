use std::path::{Path, PathBuf};

// ── Public types ──────────────────────────────────────────────────────────────

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

// ── Public API ────────────────────────────────────────────────────────────────

pub fn parse_build_config(
    gradle_root: &Path,
    module: &str,
) -> Result<BuildConfig, String> {
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

    Ok(BuildConfig {
        module: module.to_string(),
        file: relative,
        compile_sdk: extract_int_value(&content, &["compileSdk", "compileSdkVersion"]),
        min_sdk: extract_int_value(&content, &["minSdk", "minSdkVersion"]),
        target_sdk: extract_int_value(&content, &["targetSdk", "targetSdkVersion"]),
        application_id: extract_string_value(&content, "applicationId"),
        namespace: extract_string_value(&content, "namespace"),
        build_types: parse_build_types(&content),
        product_flavors: parse_product_flavors(&content),
    })
}

// ── Internal ──────────────────────────────────────────────────────────────────

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
fn extract_int_value(content: &str, keys: &[&str]) -> Option<i64> {
    for key in keys {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(key) {
                let rest = trimmed[key.len()..].trim_start();
                let digits: String = rest
                    .trim_start_matches(|c| c == '=' || c == '(' || c == ' ')
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(n) = digits.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Extract a quoted string value: `applicationId = "com.example"` or `applicationId "com.example"`
fn extract_string_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(key) {
            let rest = trimmed[key.len()..].trim_start();
            let rest = rest.trim_start_matches(|c: char| c == '=' || c == ' ');
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
        while pos < bytes.len()
            && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_')
        {
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

        if !matches!(name.as_str(), "getByName" | "maybeCreate" | "all" | "configureEach") {
            result.push((name, inner.to_string()));
        }
    }
    result
}

fn extract_bool_value(content: &str, keys: &[&str]) -> Option<bool> {
    for key in keys {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(key) {
                let rest = trimmed[key.len()..].trim();
                let rest = rest.trim_start_matches(|c: char| c == '=' || c == ' ');
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

// ── Tests ─────────────────────────────────────────────────────────────────────

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
        let release = cfg.build_types.iter().find(|b| b.name == "release").unwrap();
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
        let release = cfg.build_types.iter().find(|b| b.name == "release").unwrap();
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
        let free = cfg.product_flavors.iter().find(|f| f.name == "free").unwrap();
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
}
