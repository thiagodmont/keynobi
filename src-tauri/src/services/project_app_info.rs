//! Read and edit `versionName` / `versionCode` in the app's Gradle build file:
//! the application module's (see `gradle_modules`), else the root project's.
//!
//! Assignments are found with a small lexer that knows Groovy and Kotlin DSL
//! comments and string literals, so a commented-out `// versionCode 1` or a
//! `"//"` inside a string never decides what is read or written. A save edits
//! only a single literal assignment; anything else is refused with a message
//! saying why, and the file is left untouched.

use crate::models::error::AppError;
use crate::models::settings::ProjectAppInfo;
use crate::services::settings_manager;
use std::io::{Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// The largest version code Google Play accepts.
pub const MAX_VERSION_CODE: i64 = 2_100_000_000;

/// Largest build file read; real ones are a few kilobytes.
const MAX_BUILD_FILE_BYTES: u64 = 1024 * 1024;

/// Longest expression quoted back to the user.
const MAX_QUOTED_EXPRESSION_CHARS: usize = 80;

const VERSION_NAME: &str = "versionName";
const VERSION_CODE: &str = "versionCode";

/// The build file App Info reads and edits, relative to `root`: the first
/// that exists of the application module's and the root project's.
///
/// # Errors
/// When the project has several application modules.
pub fn find_build_file(root: &Path) -> Result<Option<String>, AppError> {
    Ok(
        crate::services::gradle_modules::application_build_file_candidates(root)
            .map_err(AppError::InvalidInput)?
            .into_iter()
            .find(|rel| root.join(rel).is_file()),
    )
}

static RE_APPLICATION_ID: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"applicationId\s*=?\s*"([^"]+)""#)
        .expect("RE_APPLICATION_ID: invalid regex")
});

/// The first `applicationId "…"` or `applicationId = "…"` in a build file.
pub fn extract_application_id(content: &str) -> Option<String> {
    let caps = RE_APPLICATION_ID.captures(content)?;
    Some(caps.get(1)?.as_str().to_owned())
}

/// Read `applicationId`, `versionName`, and `versionCode` from the app build file.
///
/// A version field is `None` exactly when its `*_unavailable` reason is set:
/// it is missing, set more than once, or set by an expression.
pub fn read_app_info(root: &Path) -> ProjectAppInfo {
    let unavailable = |reason: String| ProjectAppInfo {
        application_id: None,
        version_name: None,
        version_code: None,
        version_name_unavailable: Some(reason.clone()),
        version_code_unavailable: Some(reason),
    };
    let (rel, _, content) = match read_build_file(root) {
        Ok(found) => found,
        Err(e) => return unavailable(error_message(e)),
    };
    let file = BuildFile::new(&rel, &content);
    let (version_name, version_name_unavailable) = match file.version_name() {
        Ok((value, _)) => (Some(value), None),
        Err(reason) => (None, Some(reason)),
    };
    let (version_code, version_code_unavailable) = match file.version_code() {
        Ok((value, _)) => (Some(value), None),
        Err(reason) => (None, Some(reason)),
    };
    ProjectAppInfo {
        application_id: extract_application_id(&content),
        version_name,
        version_code,
        version_name_unavailable,
        version_code_unavailable,
    }
}

/// Write the given fields back to the app build file. `None` leaves a field as it is.
///
/// Fails without touching the file when a field is not a single literal
/// assignment, or when the file already holds these values.
pub fn save_app_info(
    root: &Path,
    version_name: Option<&str>,
    version_code: Option<i64>,
) -> Result<(), AppError> {
    if version_name.is_none() && version_code.is_none() {
        return Err(AppError::InvalidInput("Nothing to save".to_string()));
    }
    if let Some(name) = version_name {
        validate_version_name(name)?;
    }
    if let Some(code) = version_code {
        validate_version_code(code)?;
    }

    let (rel, path, content) = read_build_file(root)?;
    let file = BuildFile::new(&rel, &content);

    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    if let Some(name) = version_name {
        let (_, span) = file.version_name().map_err(AppError::InvalidInput)?;
        let quote = match content.as_bytes()[span.start] {
            b'\'' if !name.contains('\'') => '\'',
            _ => '"',
        };
        edits.push((span, format!("{quote}{name}{quote}")));
    }
    if let Some(code) = version_code {
        let (_, span) = file.version_code().map_err(AppError::InvalidInput)?;
        edits.push((span, code.to_string()));
    }

    let mut updated = content.clone();
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
    for (span, replacement) in edits {
        updated.replace_range(span, &replacement);
    }
    if updated == content {
        return Err(AppError::InvalidInput(format!(
            "{rel} already has these values; nothing was changed"
        )));
    }
    write_atomically(&path, updated.as_bytes()).map_err(|e| AppError::io(&rel, e))
}

pub fn validate_version_name(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Version name cannot be empty".to_string(),
        ));
    }
    if value
        .chars()
        .any(|c| matches!(c, '"' | '\\' | '$' | '\n' | '\r') || c.is_control())
    {
        return Err(AppError::InvalidInput(
            "Version name cannot contain quotes, backslashes, '$', line breaks, or control characters"
                .to_string(),
        ));
    }
    Ok(())
}

pub fn validate_version_code(value: i64) -> Result<(), AppError> {
    if !(1..=MAX_VERSION_CODE).contains(&value) {
        return Err(AppError::InvalidInput(format!(
            "Version code must be a whole number from 1 to {MAX_VERSION_CODE}"
        )));
    }
    Ok(())
}

/// Find the build file and read it through its canonical path, which must be a
/// regular file inside the canonical project root (a symlink may not lead out).
fn read_build_file(root: &Path) -> Result<(String, PathBuf, String), AppError> {
    let rel = find_build_file(root)?.ok_or_else(|| AppError::NotFound(no_build_file_message()))?;
    let path = crate::utils::path::validate_within_root(root, &rel).map_err(|e| match e {
        AppError::PermissionDenied(_) => AppError::PermissionDenied(format!(
            "{rel} resolves outside the project; only build files inside it can be read or edited"
        )),
        other => other,
    })?;
    let file = std::fs::File::open(&path).map_err(|e| AppError::io(&rel, e))?;
    let metadata = file.metadata().map_err(|e| AppError::io(&rel, e))?;
    if !metadata.is_file() {
        return Err(AppError::InvalidInput(format!(
            "{rel} is not a regular file"
        )));
    }
    let too_large = || {
        AppError::InvalidInput(format!(
            "{rel} is larger than {} MB; edit it directly",
            MAX_BUILD_FILE_BYTES / (1024 * 1024)
        ))
    };
    if metadata.len() > MAX_BUILD_FILE_BYTES {
        return Err(too_large());
    }
    let mut content = String::new();
    file.take(MAX_BUILD_FILE_BYTES + 1)
        .read_to_string(&mut content)
        .map_err(|e| AppError::io(&rel, e))?;
    if content.len() as u64 > MAX_BUILD_FILE_BYTES {
        return Err(too_large());
    }
    Ok((rel, path, content))
}

/// The message of an error, without the kind prefix `Display` adds.
fn error_message(e: AppError) -> String {
    match e {
        AppError::NotFound(m)
        | AppError::PermissionDenied(m)
        | AppError::InvalidInput(m)
        | AppError::Io(m)
        | AppError::ProcessFailed(m)
        | AppError::SettingsError(m)
        | AppError::McpError(m)
        | AppError::Other(m) => m,
    }
}

fn no_build_file_message() -> String {
    "No build.gradle.kts or build.gradle found in the application module or the project root"
        .to_string()
}

/// Replace `path` with `bytes` through a unique temporary file in the same
/// directory, keeping the original permissions. The original is untouched
/// unless the final rename succeeds.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let permissions = std::fs::metadata(path)?.permissions();
    let tmp = settings_manager::unique_tmp_path(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::set_permissions(&tmp, permissions))
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

// ── Build file scanning ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Code,
    Comment,
    Str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Groovy,
    Kotlin,
}

struct BuildFile<'a> {
    rel: &'a str,
    content: &'a str,
    dialect: Dialect,
    classes: Vec<Class>,
}

/// One `field = value` (or Groovy `field value`) statement outside comments and strings.
struct Assignment {
    line: usize,
    value: Range<usize>,
}

impl<'a> BuildFile<'a> {
    fn new(rel: &'a str, content: &'a str) -> Self {
        let dialect = if rel.ends_with(".kts") {
            Dialect::Kotlin
        } else {
            Dialect::Groovy
        };
        Self {
            rel,
            content,
            dialect,
            classes: classify(content, dialect),
        }
    }

    /// The literal version name and the span of its string literal (quotes included).
    fn version_name(&self) -> Result<(String, Range<usize>), String> {
        let assignment = self.single_assignment(VERSION_NAME)?;
        let text = &self.content[assignment.value.clone()];
        match string_literal(text) {
            Some(inner) => Ok((inner.to_string(), assignment.value)),
            None => Err(self.expression_message(VERSION_NAME, &assignment)),
        }
    }

    /// The literal version code and the span of its digits.
    fn version_code(&self) -> Result<(i64, Range<usize>), String> {
        let assignment = self.single_assignment(VERSION_CODE)?;
        let text = &self.content[assignment.value.clone()];
        let literal = text
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| text.parse::<i64>().ok())
            .flatten();
        match literal {
            Some(code) => Ok((code, assignment.value)),
            None => Err(self.expression_message(VERSION_CODE, &assignment)),
        }
    }

    fn single_assignment(&self, field: &str) -> Result<Assignment, String> {
        let mut found = self.assignments(field);
        match found.len() {
            0 => Err(format!("No {field} assignment found in {}", self.rel)),
            1 => Ok(found.remove(0)),
            n => {
                let lines: Vec<String> = found.iter().map(|a| a.line.to_string()).collect();
                Err(format!(
                    "{field} is set {n} times in {} (lines {}), for example once per product \
                     flavor. Keynobi cannot tell which one to change; edit the file directly",
                    self.rel,
                    lines.join(", ")
                ))
            }
        }
    }

    fn expression_message(&self, field: &str, assignment: &Assignment) -> String {
        let expression = &self.content[assignment.value.clone()];
        format!(
            "{field} in {} (line {}) is set by `{}`, so its value is defined in {}. Change it there",
            self.rel,
            assignment.line,
            shorten(expression),
            defined_where(expression)
        )
    }

    fn assignments(&self, field: &str) -> Vec<Assignment> {
        let bytes = self.content.as_bytes();
        let mut found = Vec::new();
        for (start, _) in self.content.match_indices(field) {
            let end = start + field.len();
            if self.classes[start] != Class::Code
                || (start > 0 && is_ident_byte(bytes[start - 1]))
                || bytes.get(end).is_some_and(|&b| is_ident_byte(b))
                || self.is_declaration(start)
            {
                continue;
            }
            if let Some(value) = self.assigned_value(end) {
                let line = 1 + bytes[..start].iter().filter(|&&b| b == b'\n').count();
                found.push(Assignment { line, value });
            }
        }
        found
    }

    /// `val versionCode = …` declares a local, not the Android property.
    fn is_declaration(&self, start: usize) -> bool {
        let before = self.content[..start]
            .trim_end_matches([' ', '\t'])
            .as_bytes();
        if before.len() == start {
            return false;
        }
        let token_start = before
            .iter()
            .rposition(|&b| !is_ident_byte(b))
            .map_or(0, |i| i + 1);
        matches!(&before[token_start..], b"val" | b"var" | b"def")
    }

    /// The span of the value assigned right after the field name ending at `after`.
    fn assigned_value(&self, after: usize) -> Option<Range<usize>> {
        let bytes = self.content.as_bytes();
        let j = self.skip(after, false);
        let start = match bytes.get(j) {
            Some(b'=') if bytes.get(j + 1) != Some(&b'=') => self.skip(j + 1, true),
            // Groovy method-call form: `versionCode 5`.
            Some(&b)
                if self.dialect == Dialect::Groovy
                    && j > after
                    && (b.is_ascii_alphanumeric() || matches!(b, b'_' | b'"' | b'\''))
                    && !starts_with_keyword(&self.content[j..], &["in", "as", "instanceof"]) =>
            {
                j
            }
            _ => return None,
        };

        let mut end = start;
        let mut depth = 0usize;
        while end < bytes.len() {
            let b = bytes[end];
            if self.classes[end] == Class::Comment || b == b'\n' {
                break;
            }
            if self.classes[end] == Class::Code {
                match b {
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' if depth == 0 => break,
                    b')' | b']' | b'}' => depth -= 1,
                    b';' if depth == 0 => break,
                    _ => {}
                }
            }
            end += 1;
        }
        let value = self.content[start..end].trim_end();
        (!value.is_empty()).then(|| start..start + value.len())
    }

    /// Skip whitespace and comments from `i`; line breaks too when `newlines`.
    fn skip(&self, mut i: usize, newlines: bool) -> usize {
        let bytes = self.content.as_bytes();
        while i < bytes.len() {
            let b = bytes[i];
            let blank = b == b' ' || b == b'\t' || b == b'\r' || (newlines && b == b'\n');
            if !(blank || (self.classes[i] == Class::Comment && b != b'\n')) {
                break;
            }
            i += 1;
        }
        i
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn starts_with_keyword(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|k| {
        text.strip_prefix(k)
            .is_some_and(|rest| rest.as_bytes().first().is_none_or(|&b| !is_ident_byte(b)))
    })
}

/// The content of a plain single- or double-quoted literal with nothing to interpolate.
fn string_literal(text: &str) -> Option<&str> {
    let quote = *text.as_bytes().first()?;
    if !matches!(quote, b'"' | b'\'') || text.len() < 2 || !text.ends_with(quote as char) {
        return None;
    }
    let inner = &text[1..text.len() - 1];
    let plain = !inner.contains(quote as char)
        && !inner.contains('\\')
        && !inner.contains('\n')
        && (quote == b'\'' || !inner.contains('$'));
    plain.then_some(inner)
}

fn defined_where(expression: &str) -> &'static str {
    if expression.contains("libs.") || expression.contains("versionCatalogs") {
        "the version catalog (usually gradle/libs.versions.toml)"
    } else if ["property(", "Property(", "properties[", "ext.", "extra["]
        .iter()
        .any(|p| expression.contains(p))
    {
        "a Gradle property (for example in gradle.properties)"
    } else {
        "another part of the build scripts"
    }
}

fn shorten(expression: &str) -> String {
    if expression.chars().count() <= MAX_QUOTED_EXPRESSION_CHARS {
        return expression.to_string();
    }
    let cut: String = expression
        .chars()
        .take(MAX_QUOTED_EXPRESSION_CHARS)
        .collect();
    format!("{cut}…")
}

/// Classify every byte of `content` as code, comment, or string literal.
///
/// Handles `//` and `/* */` comments (nested in Kotlin), single, double, and
/// triple-quoted strings, escapes, and `${…}` interpolation inside
/// double-quoted strings.
fn classify(content: &str, dialect: Dialect) -> Vec<Class> {
    enum Ctx {
        /// Code; `Some(depth)` inside a `${…}` interpolation.
        Code(Option<usize>),
        Str {
            quote: u8,
            triple: bool,
        },
    }
    let bytes = content.as_bytes();
    let mut classes = vec![Class::Code; bytes.len()];
    let mut stack = vec![Ctx::Code(None)];
    let starts = |i: usize, pat: &[u8]| bytes[i..].starts_with(pat);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match stack.last_mut() {
            Some(Ctx::Code(interp)) => {
                if starts(i, b"//") {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        classes[i] = Class::Comment;
                        i += 1;
                    }
                    continue;
                }
                if starts(i, b"/*") {
                    let mut depth = 0usize;
                    while i < bytes.len() {
                        if starts(i, b"/*") && (depth == 0 || dialect == Dialect::Kotlin) {
                            depth += 1;
                            classes[i..i + 2].fill(Class::Comment);
                            i += 2;
                        } else if starts(i, b"*/") {
                            depth -= 1;
                            classes[i..i + 2].fill(Class::Comment);
                            i += 2;
                            if depth == 0 {
                                break;
                            }
                        } else {
                            classes[i] = Class::Comment;
                            i += 1;
                        }
                    }
                    continue;
                }
                match b {
                    b'"' | b'\'' => {
                        let triple = starts(i, &[b, b, b]);
                        let len = if triple { 3 } else { 1 };
                        classes[i..i + len].fill(Class::Str);
                        i += len;
                        stack.push(Ctx::Str { quote: b, triple });
                        continue;
                    }
                    b'{' => {
                        if let Some(depth) = interp {
                            *depth += 1;
                        }
                    }
                    b'}' => match interp {
                        Some(0) => {
                            classes[i] = Class::Str;
                            stack.pop();
                            i += 1;
                            continue;
                        }
                        Some(depth) => *depth -= 1,
                        None => {}
                    },
                    _ => {}
                }
                i += 1;
            }
            Some(Ctx::Str { quote, triple }) => {
                let (quote, triple) = (*quote, *triple);
                classes[i] = Class::Str;
                let raw = triple && dialect == Dialect::Kotlin;
                if b == b'\\' && !raw {
                    if i + 1 < bytes.len() {
                        classes[i + 1] = Class::Str;
                    }
                    i += 2;
                } else if triple && starts(i, &[quote, quote, quote]) {
                    classes[i..i + 3].fill(Class::Str);
                    i += 3;
                    stack.pop();
                } else if !triple && b == quote {
                    i += 1;
                    stack.pop();
                } else if quote == b'"' && starts(i, b"${") {
                    classes[i + 1] = Class::Str;
                    i += 2;
                    stack.push(Ctx::Code(Some(0)));
                } else if !triple && b == b'\n' {
                    // Unterminated literal: recover at the line end.
                    classes[i] = Class::Code;
                    i += 1;
                    stack.pop();
                } else {
                    i += 1;
                }
            }
            None => break,
        }
    }
    classes
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn project(rel: &str, content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        dir
    }

    fn info_of(rel: &str, content: &str) -> ProjectAppInfo {
        read_app_info(project(rel, content).path())
    }

    fn read(dir: &TempDir, rel: &str) -> String {
        std::fs::read_to_string(dir.path().join(rel)).unwrap()
    }

    fn message(err: AppError) -> String {
        match err {
            AppError::InvalidInput(m)
            | AppError::NotFound(m)
            | AppError::Io(m)
            | AppError::PermissionDenied(m) => m,
            other => panic!("unexpected error: {other:?}"),
        }
    }

    const KTS: &str = "app/build.gradle.kts";
    const GROOVY: &str = "app/build.gradle";

    #[test]
    fn saves_the_real_assignment_not_a_commented_line_before_it_groovy() {
        let before = "android {\n    defaultConfig {\n        // versionCode 1\n        // versionName \"0.1\"\n        versionCode 7\n        versionName \"1.0\"\n    }\n}\n";
        let dir = project(GROOVY, before);

        save_app_info(dir.path(), Some("1.1"), Some(8)).unwrap();

        assert_eq!(
            read(&dir, GROOVY),
            before
                .replace("versionCode 7", "versionCode 8")
                .replace("versionName \"1.0\"", "versionName \"1.1\"")
        );
    }

    #[test]
    fn saves_the_real_assignment_not_a_block_comment_kts() {
        let before = "android {\n    defaultConfig {\n        /* old:\n        versionCode = 1\n        versionName = \"0.1\" */\n        versionCode = 7 // bump for release\n        versionName = \"1.0\"\n    }\n}\n";
        let dir = project(KTS, before);

        save_app_info(dir.path(), Some("1.1"), Some(8)).unwrap();

        let after = read(&dir, KTS);
        assert!(after.contains("        versionCode = 1\n        versionName = \"0.1\" */"));
        assert!(after.contains("versionCode = 8 // bump for release"));
        assert!(after.contains("versionName = \"1.1\""));
    }

    #[test]
    fn nested_kotlin_block_comments_are_skipped() {
        let content = "/* outer /* inner */ versionCode = 99 */\nversionCode = 3\n";
        let info = info_of(KTS, content);
        assert_eq!(info.version_code, Some(3));
    }

    #[test]
    fn slashes_inside_strings_do_not_start_comments() {
        let before = "android {\n    defaultConfig {\n        buildConfigField \"String\", \"GLOB\", \"\\\"/*\\\"\"\n        resValue \"string\", \"url\", \"https://example.com\"; versionCode 4\n        versionName \"1.0//beta\"\n    }\n}\n";
        let dir = project(GROOVY, before);

        let info = read_app_info(dir.path());
        assert_eq!(info.version_code, Some(4));
        assert_eq!(info.version_name.as_deref(), Some("1.0//beta"));

        save_app_info(dir.path(), Some("2.0"), Some(5)).unwrap();
        let after = read(&dir, GROOVY);
        assert!(after.contains("\"https://example.com\"; versionCode 5\n"));
        assert!(after.contains("versionName \"2.0\"\n"));
    }

    #[test]
    fn mentions_in_strings_and_reads_are_not_assignments() {
        let content = "println(\"versionCode = 1\")\nval shown = \"${android.defaultConfig.versionCode}\"\nif (versionCode == 2) {}\nversionCode = 3\n";
        let info = info_of(KTS, content);
        assert_eq!(info.version_code, Some(3));
        assert_eq!(info.version_code_unavailable, None);
    }

    #[test]
    fn a_local_variable_is_not_the_version_code() {
        let content =
            "val versionCode = 12\nandroid { defaultConfig { versionCode = versionCode } }\n";
        let info = info_of(KTS, content);
        assert_eq!(info.version_code, None);
        let reason = info.version_code_unavailable.unwrap();
        assert!(reason.contains("is set by `versionCode`"), "{reason}");
    }

    #[test]
    fn catalog_references_are_reported_as_defined_elsewhere_and_not_saved() {
        let before = "android {\n    defaultConfig {\n        versionCode = libs.versions.code.get().toInt()\n        versionName = project.property(\"appVersion\") as String\n    }\n}\n";
        let dir = project(KTS, before);

        let info = read_app_info(dir.path());
        assert_eq!(info.version_code, None);
        assert_eq!(info.version_name, None);
        let code = info.version_code_unavailable.unwrap();
        assert!(code.contains("line 3"), "{code}");
        assert!(
            code.contains("`libs.versions.code.get().toInt()`"),
            "{code}"
        );
        assert!(code.contains("version catalog"), "{code}");
        let name = info.version_name_unavailable.unwrap();
        assert!(name.contains("gradle.properties"), "{name}");

        let err = message(save_app_info(dir.path(), None, Some(9)).unwrap_err());
        assert!(err.contains("version catalog"), "{err}");
        assert_eq!(read(&dir, KTS), before);
    }

    #[test]
    fn a_field_set_in_two_places_is_refused() {
        let before = "android {\n    productFlavors {\n        free { versionCode 1 }\n        paid { versionCode 2 }\n    }\n    defaultConfig { versionName \"1.0\" }\n}\n";
        let dir = project(GROOVY, before);

        let info = read_app_info(dir.path());
        assert_eq!(info.version_code, None);
        let reason = info.version_code_unavailable.unwrap();
        assert!(
            reason.contains("set 2 times") && reason.contains("lines 3, 4"),
            "{reason}"
        );
        assert_eq!(info.version_name.as_deref(), Some("1.0"));

        let err = message(save_app_info(dir.path(), Some("1.1"), Some(3)).unwrap_err());
        assert!(err.contains("set 2 times"), "{err}");
        assert_eq!(read(&dir, GROOVY), before);

        // The version name alone can still be saved.
        save_app_info(dir.path(), Some("1.1"), None).unwrap();
        assert!(read(&dir, GROOVY).contains("versionName \"1.1\""));
    }

    #[test]
    fn a_missing_assignment_is_an_error_not_a_silent_success() {
        let before =
            "android {\n    // versionCode 1\n    defaultConfig { versionName = \"1.0\" }\n}\n";
        let dir = project(KTS, before);

        let err = message(save_app_info(dir.path(), Some("1.0"), Some(2)).unwrap_err());
        assert!(err.contains("No versionCode assignment found"), "{err}");
        assert_eq!(read(&dir, KTS), before);
    }

    #[test]
    fn saving_the_current_values_is_reported_as_a_no_op() {
        let before = "versionCode = 3\nversionName = \"1.0\"\n";
        let dir = project(KTS, before);

        let err = message(save_app_info(dir.path(), Some("1.0"), Some(3)).unwrap_err());
        assert!(err.contains("nothing was changed"), "{err}");
        assert_eq!(read(&dir, KTS), before);
    }

    #[test]
    fn single_quoted_groovy_names_keep_their_quotes() {
        let dir = project(GROOVY, "versionName '1.0'\nversionCode 1\n");
        save_app_info(dir.path(), Some("1.1"), None).unwrap();
        assert_eq!(read(&dir, GROOVY), "versionName '1.1'\nversionCode 1\n");
    }

    #[test]
    fn interpolated_names_are_not_literals() {
        let info = info_of(GROOVY, "versionName \"1.${minor}\"\nversionCode 1\n");
        assert_eq!(info.version_name, None);
        assert!(info
            .version_name_unavailable
            .unwrap()
            .contains("`\"1.${minor}\"`"));
    }

    #[test]
    fn a_failed_write_leaves_the_file_intact() {
        let before = "versionCode = 3\nversionName = \"1.0\"\n";
        let dir = project(KTS, before);
        let app = dir.path().join("app");
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = save_app_info(dir.path(), Some("2.0"), Some(4));

        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(result, Err(AppError::Io(_))), "{result:?}");
        assert_eq!(read(&dir, KTS), before);
        let names: Vec<_> = std::fs::read_dir(&app)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("build.gradle.kts")]);
    }

    #[test]
    fn saving_preserves_the_file_mode() {
        let dir = project(KTS, "versionCode = 3\n");
        let path = dir.path().join(KTS);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        save_app_info(dir.path(), None, Some(4)).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn saving_does_not_reuse_a_fixed_temporary_name() {
        let dir = project(GROOVY, "versionCode 3\n");
        let stray = dir.path().join("app/build.gradle.tmp");
        std::fs::write(&stray, "someone else's file").unwrap();

        save_app_info(dir.path(), None, Some(4)).unwrap();

        assert_eq!(read(&dir, GROOVY), "versionCode 4\n");
        assert_eq!(
            std::fs::read_to_string(&stray).unwrap(),
            "someone else's file"
        );
        let path = dir.path().join(GROOVY);
        assert_ne!(
            settings_manager::unique_tmp_path(&path),
            settings_manager::unique_tmp_path(&path)
        );
    }

    #[test]
    fn version_code_range_matches_what_android_accepts() {
        assert!(validate_version_code(-1).is_err());
        assert!(validate_version_code(0).is_err());
        assert!(validate_version_code(1).is_ok());
        assert!(validate_version_code(MAX_VERSION_CODE).is_ok());
        assert!(validate_version_code(MAX_VERSION_CODE + 1).is_err());
        assert!(validate_version_code(i64::MAX).is_err());
    }

    #[test]
    fn out_of_range_codes_are_rejected_before_the_file_is_read() {
        let before = "versionCode = 3\n";
        let dir = project(KTS, before);
        let err = save_app_info(dir.path(), None, Some(MAX_VERSION_CODE + 1)).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
        assert_eq!(read(&dir, KTS), before);
    }

    #[test]
    fn version_name_validation_rejects_gradle_string_breakers() {
        assert!(validate_version_name("").is_err());
        assert!(validate_version_name("1.2\"3").is_err());
        assert!(validate_version_name("1.2\\3").is_err());
        assert!(validate_version_name("1.$0").is_err());
        assert!(validate_version_name("1.2\n3").is_err());
    }

    #[test]
    fn nothing_to_save_is_rejected() {
        let dir = project(KTS, "versionCode = 3\n");
        assert!(matches!(
            save_app_info(dir.path(), None, None),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn a_project_without_a_build_file_reports_why() {
        let dir = TempDir::new().unwrap();
        let info = read_app_info(dir.path());
        assert!(info
            .version_code_unavailable
            .unwrap()
            .contains("No build.gradle.kts or build.gradle found"));
        assert!(matches!(
            save_app_info(dir.path(), None, Some(2)),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn kotlin_raw_strings_do_not_process_escapes() {
        let content = "val s = \"\"\"C:\\\"\"\"\nversionCode = 5\n";
        let info = info_of(KTS, content);
        assert_eq!(info.version_code, Some(5));
    }

    // ── Symlinks and file size ──────────────────────────────────────────────

    const BUILD: &str = "versionCode = 3\nversionName = \"1.0\"\n";

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn an_app_dir_symlinked_outside_the_project_is_neither_read_nor_written() {
        let outside = project(KTS, BUILD);
        let dir = TempDir::new().unwrap();
        std::os::unix::fs::symlink(outside.path().join("app"), dir.path().join("app")).unwrap();

        let info = read_app_info(dir.path());
        assert_eq!(info.version_code, None);
        assert_eq!(info.application_id, None);
        let reason = info.version_code_unavailable.unwrap();
        assert!(reason.contains("outside the project"), "{reason}");

        let err = save_app_info(dir.path(), Some("2.0"), Some(4)).unwrap_err();
        assert!(matches!(err, AppError::PermissionDenied(_)), "{err:?}");
        assert_eq!(read(&outside, KTS), BUILD);
        assert_eq!(
            names_in(&outside.path().join("app")),
            vec!["build.gradle.kts"]
        );
    }

    #[test]
    fn a_build_file_symlinked_outside_the_project_is_refused() {
        let outside = project("build.gradle.kts", BUILD);
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("app")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("build.gradle.kts"),
            dir.path().join(KTS),
        )
        .unwrap();

        assert_eq!(read_app_info(dir.path()).version_code, None);
        let err = save_app_info(dir.path(), None, Some(4)).unwrap_err();
        assert!(matches!(err, AppError::PermissionDenied(_)), "{err:?}");
        assert_eq!(read(&outside, "build.gradle.kts"), BUILD);
        assert_eq!(names_in(outside.path()), vec!["build.gradle.kts"]);
    }

    #[test]
    fn symlinks_that_stay_inside_the_project_work() {
        let dir = project("modules/android/build.gradle.kts", BUILD);
        std::os::unix::fs::symlink(dir.path().join("modules/android"), dir.path().join("app"))
            .unwrap();

        assert_eq!(read_app_info(dir.path()).version_code, Some(3));
        save_app_info(dir.path(), None, Some(4)).unwrap();
        assert_eq!(
            read(&dir, "modules/android/build.gradle.kts"),
            BUILD.replace("= 3", "= 4")
        );
        assert!(std::fs::symlink_metadata(dir.path().join("app"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            names_in(&dir.path().join("modules/android")),
            vec!["build.gradle.kts"]
        );
    }

    #[test]
    fn a_symlinked_build_file_inside_the_project_is_edited_through_the_link() {
        let dir = project("gradle/app.gradle.kts", BUILD);
        std::fs::create_dir(dir.path().join("app")).unwrap();
        std::os::unix::fs::symlink(
            dir.path().join("gradle/app.gradle.kts"),
            dir.path().join(KTS),
        )
        .unwrap();

        save_app_info(dir.path(), None, Some(4)).unwrap();

        assert_eq!(
            read(&dir, "gradle/app.gradle.kts"),
            BUILD.replace("= 3", "= 4")
        );
        assert!(std::fs::symlink_metadata(dir.path().join(KTS))
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn an_oversized_build_file_is_not_read() {
        let mut content = String::from(BUILD);
        content.push_str(&"// padding\n".repeat((MAX_BUILD_FILE_BYTES as usize / 11) + 1));
        let dir = project(KTS, &content);

        let reason = read_app_info(dir.path()).version_code_unavailable.unwrap();
        assert!(reason.contains("larger than"), "{reason}");
        assert!(matches!(
            save_app_info(dir.path(), None, Some(4)),
            Err(AppError::InvalidInput(_))
        ));
        assert_eq!(read(&dir, KTS), content);
    }

    // ── Application module ──────────────────────────────────────────────────

    #[test]
    fn app_info_reads_and_writes_the_application_module_not_named_app() {
        let dir = project(
            "mobile/build.gradle.kts",
            "plugins {\n    id(\"com.android.application\")\n}\nandroid {\n    defaultConfig {\n        applicationId = \"com.example.phone\"\n        versionCode = 3\n        versionName = \"1.0\"\n    }\n}\n",
        );
        std::fs::write(
            dir.path().join("settings.gradle.kts"),
            "include(\":mobile\")\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("build.gradle.kts"), "plugins {}\n").unwrap();

        let info = read_app_info(dir.path());
        assert_eq!(info.application_id.as_deref(), Some("com.example.phone"));
        assert_eq!(info.version_code, Some(3));

        save_app_info(dir.path(), Some("1.1"), Some(4)).unwrap();
        let saved = read(&dir, "mobile/build.gradle.kts");
        assert!(saved.contains("versionCode = 4") && saved.contains("versionName = \"1.1\""));
        assert_eq!(read(&dir, "build.gradle.kts"), "plugins {}\n");
    }

    #[test]
    fn a_library_named_app_is_not_edited() {
        let dir = project(
            "app/build.gradle.kts",
            "plugins {\n    id(\"com.android.library\")\n}\nversionCode = 7\n",
        );
        std::fs::create_dir_all(dir.path().join("androidApp")).unwrap();
        std::fs::write(
            dir.path().join("androidApp/build.gradle"),
            "plugins {\n    id 'com.android.application'\n}\nversionCode 2\nversionName \"2.0\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("settings.gradle"),
            "include ':app', ':androidApp'\n",
        )
        .unwrap();

        assert_eq!(read_app_info(dir.path()).version_code, Some(2));
        save_app_info(dir.path(), None, Some(3)).unwrap();
        assert!(read(&dir, "androidApp/build.gradle").contains("versionCode 3"));
        assert!(read(&dir, "app/build.gradle.kts").contains("versionCode = 7"));
    }

    #[test]
    fn several_application_modules_are_reported_not_guessed() {
        let app = "plugins {\n    id(\"com.android.application\")\n}\nversionCode = 1\n";
        let dir = project("mobile/build.gradle.kts", app);
        std::fs::create_dir_all(dir.path().join("wear")).unwrap();
        std::fs::write(dir.path().join("wear/build.gradle.kts"), app).unwrap();
        std::fs::write(
            dir.path().join("settings.gradle.kts"),
            "include(\":mobile\", \":wear\")\n",
        )
        .unwrap();

        let reason = read_app_info(dir.path()).version_code_unavailable.unwrap();
        assert!(reason.contains(":mobile, :wear"), "{reason}");
        let err = save_app_info(dir.path(), None, Some(2)).unwrap_err();
        assert!(message(err).contains(":mobile, :wear"));
        assert_eq!(read(&dir, "mobile/build.gradle.kts"), app);
    }
}
