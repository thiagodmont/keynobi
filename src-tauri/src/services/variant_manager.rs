use crate::models::variant::{BuildVariant, VariantList};
use std::path::Path;

/// Compute the capitalized variant name from build type + flavors.
///
/// e.g. flavors = ["free"], build_type = "debug" → "freeDebug"
fn variant_name(flavors: &[&str], build_type: &str) -> String {
    let mut name = String::new();
    for (i, flavor) in flavors.iter().enumerate() {
        if i == 0 {
            name.push_str(flavor);
        } else {
            let mut chars = flavor.chars();
            if let Some(first) = chars.next() {
                name.extend(first.to_uppercase());
                name.push_str(chars.as_str());
            }
        }
    }
    // Capitalize first letter of build_type before appending.
    let mut bt_chars = build_type.chars();
    if let Some(first) = bt_chars.next() {
        name.extend(first.to_uppercase());
        name.push_str(bt_chars.as_str());
    }
    name
}

/// Capitalize the first letter of a string slice.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Build the Gradle task name from components.
///
/// e.g. flavors = ["free"], build_type = "debug" → "assembleFreeDebug"
fn task_name(prefix: &str, flavors: &[&str], build_type: &str) -> String {
    let mut name = prefix.to_owned();
    for flavor in flavors {
        name.push_str(&capitalize(flavor));
    }
    name.push_str(&capitalize(build_type));
    name
}

/// Parse `build.gradle.kts` content and extract variants.
///
/// Returns `None` if the content cannot yield any variants;
/// the caller must fall back to the Gradle tasks list in that case.
pub fn parse_variants_from_gradle(
    file_path: &Path,
    content: &str,
) -> Option<VariantList> {
    let _ = file_path; // path kept for logging / future use

    let build_types = extract_block_names(content, "buildTypes");
    let (flavor_dimensions, flavors_map) = extract_flavors(content);

    // Return None when the buildTypes block is completely absent — signals the
    // caller to fall through to the Gradle task query for a full discovery.
    // When any buildTypes block IS present, always include both "debug" and
    // "release" because AGP provides them implicitly in every Android project.
    let effective_build_types: Vec<String> = if build_types.is_empty() {
        return None;
    } else {
        let mut types = build_types;
        // AGP guarantees debug and release exist even when not declared.
        if !types.iter().any(|t| t == "debug") {
            types.insert(0, "debug".into());
        }
        if !types.iter().any(|t| t == "release") {
            types.push("release".into());
        }
        types
    };

    let mut variants = Vec::new();

    if flavors_map.is_empty() {
        // No flavors: one variant per build type.
        for bt in &effective_build_types {
            let name = capitalize(bt);
            // For single build type the variant name is just the build type.
            let variant_name = bt.clone();
            variants.push(BuildVariant {
                name: variant_name.clone(),
                build_type: bt.clone(),
                flavors: vec![],
                assemble_task: format!("assemble{name}"),
                install_task: format!("install{name}"),
            });
        }
    } else {
        // Compute Cartesian product of flavors (across dimensions) × build types.
        let flavor_combos = cartesian_product(&flavor_dimensions, &flavors_map);
        for combo in &flavor_combos {
            let combo_refs: Vec<&str> = combo.iter().map(|s| s.as_str()).collect();
            for bt in &effective_build_types {
                let vname = variant_name(&combo_refs, bt);
                variants.push(BuildVariant {
                    name: vname.clone(),
                    build_type: bt.clone(),
                    flavors: combo.clone(),
                    assemble_task: task_name("assemble", &combo_refs, bt),
                    install_task: task_name("install", &combo_refs, bt),
                });
            }
        }
    }

    Some(VariantList { variants, active: None })
}

/// Extract the names of entries inside a named block.
///
/// Looks for patterns like:
/// ```
/// buildTypes {
///     debug { ... }
///     release { ... }
/// }
/// ```
fn extract_block_names(content: &str, block_name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let search = format!("{block_name} {{");

    let start = match content.find(&search) {
        Some(pos) => pos + search.len(),
        None => return names,
    };

    // Walk characters to find the matching closing brace (handles nesting).
    let chars: Vec<char> = content[start..].chars().collect();
    let mut depth = 1usize;
    let mut i = 0;
    let mut current_name = String::new();

    while i < chars.len() && depth > 0 {
        let c = chars[i];
        match c {
            '{' => {
                depth += 1;
                // The text before this `{` at depth 1 is an entry name.
                if depth == 2 {
                    let name = current_name.trim().to_owned();
                    if !name.is_empty() && !name.contains('\n') && is_identifier(&name) {
                        names.push(name);
                    }
                }
                current_name.clear();
            }
            '}' => {
                depth -= 1;
                current_name.clear();
            }
            '\n' => {
                // After each line break at depth 1, capture the next name.
                if depth == 1 {
                    current_name.clear();
                }
            }
            _ => {
                if depth == 1 {
                    current_name.push(c);
                }
            }
        }
        i += 1;
    }

    names
}

/// Return true if `s` could be an identifier (build type or flavor name).
fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// Extract flavor dimensions and the flavors per dimension.
///
/// Looks for:
/// ```
/// flavorDimensions("api", "mode")
/// productFlavors {
///     demo { dimension "mode" }
///     full { dimension "mode" }
///     minApi24 { dimension "api" }
/// }
/// ```
fn extract_flavors(content: &str) -> (Vec<String>, std::collections::HashMap<String, Vec<String>>) {
    use std::collections::HashMap;
    use regex::Regex;

    let dim_re = Regex::new(r#"flavorDimensions\s*\(([^)]+)\)"#).expect("static");
    // Note: flavor_re is reserved for future use with multi-line Regex support.

    let mut dimensions: Vec<String> = Vec::new();
    let mut flavors_map: HashMap<String, Vec<String>> = HashMap::new();

    if let Some(caps) = dim_re.captures(content) {
        let dims_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        for d in dims_str.split(',') {
            let dim = d.trim().trim_matches(|c| c == '"' || c == '\'').to_owned();
            if !dim.is_empty() {
                dimensions.push(dim.clone());
                flavors_map.insert(dim, vec![]);
            }
        }
    }

    // Walk the productFlavors block line-by-line to find flavors with dimension assignments.
    // This avoids the multi-line regex issues with arbitrary Gradle DSL content.
    let prod_flavors_start = content.find("productFlavors {").map(|p| p + "productFlavors {".len());
    if let Some(start) = prod_flavors_start {
        let block = find_block_content(&content[start..]);
        // Parse each `name { ... dimension "X" ... }` sub-block.
        let sub_re = Regex::new(r"(\w+)\s*\{([^}]*)\}").expect("static");
        let dim_inner_re = Regex::new(r#"dimension\s*["'](\w+)["']"#).expect("static");
        for caps in sub_re.captures_iter(block) {
            let flavor = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let body = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if !is_identifier(flavor) {
                continue;
            }
            if let Some(dcaps) = dim_inner_re.captures(body) {
                let dim = dcaps.get(1).map(|m| m.as_str()).unwrap_or("").to_owned();
                if !dim.is_empty() {
                    flavors_map.entry(dim).or_default().push(flavor.to_owned());
                }
            }
        }
    }

    // If no dimensions declared, collect all productFlavors names into a single unnamed dimension.
    if dimensions.is_empty() {
        let all_flavors = extract_block_names(content, "productFlavors");
        if !all_flavors.is_empty() {
            let dim = "default".to_owned();
            dimensions.push(dim.clone());
            flavors_map.insert(dim, all_flavors);
        }
    }

    (dimensions, flavors_map)
}

/// Extract the raw content between the opening `{` that follows `start` and its matching `}`.
fn find_block_content(after_open: &str) -> &str {
    let mut depth = 1usize;
    let mut end = 0;
    for (i, c) in after_open.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    &after_open[..end]
}

/// Compute the Cartesian product of flavors across dimensions.
///
/// e.g. dimensions ["api", "mode"], flavors {"api": ["24", "26"], "mode": ["demo", "full"]}
/// → [["24", "demo"], ["24", "full"], ["26", "demo"], ["26", "full"]]
fn cartesian_product(
    dimensions: &[String],
    map: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<Vec<String>> {
    let mut result: Vec<Vec<String>> = vec![vec![]];
    for dim in dimensions {
        let flavors = match map.get(dim) {
            Some(f) if !f.is_empty() => f,
            _ => continue,
        };
        let mut new_result = Vec::new();
        for combo in &result {
            for flavor in flavors {
                let mut new_combo = combo.clone();
                new_combo.push(flavor.clone());
                new_result.push(new_combo);
            }
        }
        result = new_result;
    }
    result.retain(|v| !v.is_empty());
    result
}

/// Parse the output of `./gradlew :app:tasks --console=plain` and
/// extract assemble/install tasks that represent real build variants.
///
/// Excludes test-related tasks (AndroidTest, UnitTest) which AGP generates
/// automatically and are not selectable build variants.
pub fn parse_variants_from_tasks_output(tasks_output: &str) -> VariantList {
    use regex::Regex;
    let re = Regex::new(r"^(assemble|install)([A-Z]\w+)").expect("static");

    // Test task suffixes generated by AGP — not real build variants.
    // e.g. assembleDebugAndroidTest, assembleReleaseUnitTest,
    //      installDebugAndroidTest, assembleAndroidTest
    let is_test_suffix = |suffix: &str| -> bool {
        suffix.contains("AndroidTest")
            || suffix.contains("UnitTest")
            || suffix.contains("TestCoverage")
            || suffix.contains("TestReport")
            // Skip the bare "All" lifecycle task (assembleAll)
            || suffix == "All"
    };

    let mut assemble: Vec<(String, String)> = Vec::new(); // (task_suffix, full_task)
    let mut install: Vec<(String, String)> = Vec::new();

    for line in tasks_output.lines() {
        let trimmed = line.trim();
        if let Some(caps) = re.captures(trimmed) {
            let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let suffix = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_owned();

            // Skip test assembly/install tasks — they are not build variants.
            if is_test_suffix(&suffix) {
                continue;
            }

            let task = format!("{}{}", prefix, suffix);
            match prefix {
                "assemble" => assemble.push((suffix, task)),
                "install" => install.push((suffix, task)),
                _ => {}
            }
        }
    }

    // Build variants from assemble tasks; pair with install tasks where possible.
    let install_map: std::collections::HashMap<String, String> = install.into_iter().collect();
    let mut variants: Vec<BuildVariant> = assemble
        .into_iter()
        .map(|(suffix, assemble_task)| {
            // Variant name is the suffix with first letter lowercased.
            // e.g. "Debug" → "debug", "FreeDebug" → "freeDebug"
            let name = {
                let mut chars = suffix.chars();
                chars.next().map(|c| c.to_lowercase().to_string()).unwrap_or_default()
                    + chars.as_str()
            };
            let install_task = install_map.get(&suffix).cloned()
                .unwrap_or_else(|| format!("install{suffix}"));
            BuildVariant {
                name: name.clone(),
                build_type: infer_build_type(&suffix),
                flavors: vec![],
                assemble_task,
                install_task,
            }
        })
        .collect();

    // Deduplicate by name (keep first occurrence).
    let mut seen = std::collections::HashSet::new();
    variants.retain(|v| seen.insert(v.name.clone()));

    VariantList { variants, active: None }
}

fn infer_build_type(suffix: &str) -> String {
    // Heuristic: if the suffix ends with "Debug" or "Release", use that.
    if suffix.ends_with("Debug") {
        return "debug".into();
    }
    if suffix.ends_with("Release") {
        return "release".into();
    }
    // Otherwise lowercase the whole suffix as the build type.
    suffix
        .chars()
        .enumerate()
        .map(|(i, c)| if i == 0 { c.to_lowercase().next().unwrap_or(c) } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_name_no_flavor() {
        assert_eq!(variant_name(&[], "debug"), "Debug");
    }

    #[test]
    fn variant_name_single_flavor() {
        assert_eq!(variant_name(&["free"], "debug"), "freeDebug");
    }

    #[test]
    fn variant_name_multi_flavor() {
        assert_eq!(variant_name(&["free", "minApi24"], "release"), "freeMinApi24Release");
    }

    #[test]
    fn task_name_assemble_no_flavor() {
        assert_eq!(task_name("assemble", &[], "debug"), "assembleDebug");
    }

    #[test]
    fn task_name_assemble_flavor() {
        assert_eq!(task_name("assemble", &["free"], "release"), "assembleFreeRelease");
    }

    #[test]
    fn extract_build_types_simple() {
        let content = r#"
android {
    buildTypes {
        debug {
            minifyEnabled false
        }
        release {
            minifyEnabled true
        }
        staging {
        }
    }
}
"#;
        let types = extract_block_names(content, "buildTypes");
        assert!(types.contains(&"debug".to_string()));
        assert!(types.contains(&"release".to_string()));
        assert!(types.contains(&"staging".to_string()));
    }

    /// Real-world scenario: only "release" is declared; debug is an AGP implicit
    /// and must always be present in the preview alongside release.
    #[test]
    fn preview_returns_only_explicitly_declared_types() {
        let content = r#"
android {
    compileSdk 35
    buildTypes {
        release {
            minifyEnabled false
            proguardFiles getDefaultProguardFile('proguard-android-optimize.txt'), 'proguard-rules.pro'
        }
    }
}
"#;
        let list = parse_variants_from_gradle(std::path::Path::new("build.gradle"), content)
            .expect("should return a list when buildTypes block is present");
        let names: Vec<&str> = list.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"release"), "release must be present: got {names:?}");
        // debug is AGP-implicit and must be guaranteed by the preview.
        assert!(names.contains(&"debug"), "debug must be guaranteed in preview: got {names:?}");
    }

    /// When buildTypes block is completely absent, return None so the caller
    /// falls through to the Gradle task query.
    #[test]
    fn returns_none_when_no_build_types_declared() {
        let content = r#"
android {
    compileSdk 35
    defaultConfig {
        applicationId "com.example.app"
    }
}
"#;
        let result = parse_variants_from_gradle(std::path::Path::new("build.gradle"), content);
        assert!(result.is_none(), "should return None when no buildTypes block exists");
    }

    /// Custom build type alongside explicitly-declared release — preview returns
    /// debug (guaranteed), release, and the custom type.
    #[test]
    fn custom_build_type_alongside_release() {
        let content = r#"
android {
    buildTypes {
        release { minifyEnabled true }
        staging { initWith debug }
    }
}
"#;
        let list = parse_variants_from_gradle(std::path::Path::new("build.gradle"), content)
            .expect("should return a list");
        let names: Vec<&str> = list.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"debug"), "debug must be guaranteed: got {names:?}");
        assert!(names.contains(&"release"), "release must be present: got {names:?}");
        assert!(names.contains(&"staging"), "staging must be present: got {names:?}");
    }

    #[test]
    fn parse_variants_from_tasks_simple() {
        let output = "\
assembleDebug - Assembles main debug output.\n\
assembleRelease - Assembles main release output.\n\
installDebug - Installs the Debug build.\n\
";
        let list = parse_variants_from_tasks_output(output);
        assert_eq!(list.variants.len(), 2);
        let names: Vec<&str> = list.variants.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"debug"));
        assert!(names.contains(&"release"));
    }

    /// Test tasks generated by AGP must be excluded from the variant list.
    /// assembleAndroidTest, assembleDebugAndroidTest, etc. are NOT build variants.
    #[test]
    fn test_tasks_are_excluded_from_variants() {
        let output = "\
assemble - Assembles main outputs for all variants.\n\
assembleAndroidTest - Assembles all the Test applications.\n\
assembleDebug - Assembles main outputs for the debug variant.\n\
assembleDebugAndroidTest - Assembles the per-variant merged res for debug androidTest.\n\
assembleDebugUnitTest - Runs the debug unit tests.\n\
assembleRelease - Assembles main outputs for the release variant.\n\
assembleReleaseUnitTest - Runs the release unit tests.\n\
installDebug - Installs the Debug build.\n\
installDebugAndroidTest - Installs the Debug androidTest.\n\
";
        let list = parse_variants_from_tasks_output(output);
        let names: Vec<&str> = list.variants.iter().map(|v| v.name.as_str()).collect();

        // Real variants must be present.
        assert!(names.contains(&"debug"), "debug should be present: {names:?}");
        assert!(names.contains(&"release"), "release should be present: {names:?}");

        // Test variants must NOT be present.
        assert!(!names.contains(&"androidTest"), "androidTest must be excluded: {names:?}");
        assert!(!names.contains(&"debugAndroidTest"), "debugAndroidTest must be excluded: {names:?}");
        assert!(!names.contains(&"debugUnitTest"), "debugUnitTest must be excluded: {names:?}");
        assert!(!names.contains(&"releaseUnitTest"), "releaseUnitTest must be excluded: {names:?}");

        assert_eq!(list.variants.len(), 2, "only debug and release should remain: {names:?}");
    }

    #[test]
    fn parse_variants_from_tasks_multi_flavor() {
        let output = "\
assembleFreeDebug\n\
assembleFreeRelease\n\
assemblePaidDebug\n\
assemblePaidRelease\n\
installFreeDebug\n\
";
        let list = parse_variants_from_tasks_output(output);
        assert_eq!(list.variants.len(), 4);
    }

    #[test]
    fn extract_flavors_with_dimensions() {
        let content = r#"
flavorDimensions("mode")
productFlavors {
    demo { dimension "mode" }
    full { dimension "mode" }
}
"#;
        let (dims, map) = extract_flavors(content);
        assert_eq!(dims, vec!["mode".to_string()]);
        assert_eq!(map["mode"], vec!["demo".to_string(), "full".to_string()]);
    }

    #[test]
    fn cartesian_product_two_dimensions() {
        let mut map = std::collections::HashMap::new();
        map.insert("api".to_string(), vec!["v1".to_string(), "v2".to_string()]);
        map.insert("mode".to_string(), vec!["demo".to_string(), "full".to_string()]);
        let dims = vec!["api".to_string(), "mode".to_string()];
        let product = cartesian_product(&dims, &map);
        assert_eq!(product.len(), 4);
        assert!(product.contains(&vec!["v1".to_string(), "demo".to_string()]));
        assert!(product.contains(&vec!["v2".to_string(), "full".to_string()]));
    }
}
