use crate::models::search::{SearchMatch, SearchOptions, SearchResult};
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::SearcherBuilder;
use ignore::WalkBuilder;
use std::path::Path;

pub fn search_project(
    query: &str,
    root: &Path,
    options: &SearchOptions,
) -> Result<Vec<SearchResult>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!options.case_sensitive)
        .word(options.whole_word)
        .fixed_strings(!options.regex)
        .build(query)
        .map_err(|e| format!("Invalid search pattern: {e}"))?;

    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .before_context(2)
        .after_context(2)
        .build();

    let mut results: Vec<SearchResult> = Vec::new();

    let walker = build_walker(root, options);

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let file_path = entry.path();

        if let Some(ref pattern) = options.include_pattern {
            if !matches_glob(file_path, pattern) {
                continue;
            }
        }

        if let Some(ref pattern) = options.exclude_pattern {
            if matches_glob(file_path, pattern) {
                continue;
            }
        }

        let mut file_matches: Vec<SearchMatch> = Vec::new();
        let mut context_lines: Vec<(u64, String, bool)> = Vec::new();

        let sink_result = searcher.search_path(
            &matcher,
            file_path,
            UTF8(|line_num, line_content| {
                let is_match = matcher.find(line_content.as_bytes()).ok().flatten().is_some();
                context_lines.push((line_num, line_content.trim_end().to_string(), is_match));
                Ok(true)
            }),
        );

        if sink_result.is_err() {
            continue;
        }

        // Group context lines into matches
        let mut i = 0;
        while i < context_lines.len() {
            if context_lines[i].2 {
                let (line_num, ref line_content, _) = context_lines[i];

                let mut col: u32 = 0;
                let mut end_col: u32 = line_content.len() as u32;
                if let Ok(Some(m)) = matcher.find(line_content.as_bytes()) {
                    col = m.start() as u32;
                    end_col = m.end() as u32;
                }

                let context_before: Vec<String> = context_lines[..i]
                    .iter()
                    .rev()
                    .take(2)
                    .filter(|c| !c.2)
                    .map(|c| c.1.clone())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();

                let context_after: Vec<String> = context_lines[i + 1..]
                    .iter()
                    .take(2)
                    .filter(|c| !c.2)
                    .map(|c| c.1.clone())
                    .collect();

                file_matches.push(SearchMatch {
                    line: line_num as u32,
                    col,
                    end_col,
                    line_content: line_content.clone(),
                    context_before,
                    context_after,
                });
            }
            i += 1;
        }

        if !file_matches.is_empty() {
            results.push(SearchResult {
                path: file_path.to_string_lossy().to_string(),
                matches: file_matches,
            });
        }
    }

    results.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(results)
}

fn build_walker(root: &Path, _options: &SearchOptions) -> ignore::Walk {
    WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !matches!(
                name.as_ref(),
                "build" | ".gradle" | ".idea" | ".git" | "node_modules" | ".DS_Store"
            )
        })
        .build()
}

fn matches_glob(path: &Path, pattern: &str) -> bool {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    for p in pattern.split(',') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if p.starts_with("*.") {
            let ext = &p[1..];
            if file_name.ends_with(ext) {
                return true;
            }
        } else if file_name.contains(p) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("src/main")).unwrap();
        fs::write(
            root.join("src/main/Main.kt"),
            "fun main() {\n    println(\"hello world\")\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main/Utils.kt"),
            "fun helper() {\n    println(\"helper\")\n}\n\nfun anotherHelper() {\n    println(\"hello again\")\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main/Config.json"),
            "{\"key\": \"hello\"}",
        )
        .unwrap();

        dir
    }

    #[test]
    fn finds_matches_across_files() {
        let dir = setup_test_project();
        let opts = SearchOptions::default();
        let results = search_project("hello", dir.path(), &opts).unwrap();

        assert!(results.len() >= 2, "Should find 'hello' in multiple files");
        let total_matches: usize = results.iter().map(|r| r.matches.len()).sum();
        assert!(
            total_matches >= 2,
            "Should have at least 2 matches, got {total_matches}"
        );
    }

    #[test]
    fn respects_case_sensitivity() {
        let dir = setup_test_project();
        let mut opts = SearchOptions::default();
        opts.case_sensitive = true;

        let results = search_project("Hello", dir.path(), &opts).unwrap();
        let has_capital = results
            .iter()
            .any(|r| r.matches.iter().any(|m| m.line_content.contains("Hello")));
        assert!(!has_capital, "Case-sensitive search for 'Hello' should not match 'hello'");
    }

    #[test]
    fn respects_include_pattern() {
        let dir = setup_test_project();
        let mut opts = SearchOptions::default();
        opts.include_pattern = Some("*.kt".into());

        let results = search_project("hello", dir.path(), &opts).unwrap();
        for r in &results {
            assert!(
                r.path.ends_with(".kt"),
                "Should only include .kt files, got: {}",
                r.path
            );
        }
    }

    #[test]
    fn respects_exclude_pattern() {
        let dir = setup_test_project();
        let mut opts = SearchOptions::default();
        opts.exclude_pattern = Some("*.json".into());

        let results = search_project("hello", dir.path(), &opts).unwrap();
        for r in &results {
            assert!(
                !r.path.ends_with(".json"),
                "Should exclude .json files, got: {}",
                r.path
            );
        }
    }

    #[test]
    fn returns_correct_line_and_col() {
        let dir = setup_test_project();
        let opts = SearchOptions::default();
        let results = search_project("println", dir.path(), &opts).unwrap();

        assert!(!results.is_empty());
        for r in &results {
            for m in &r.matches {
                assert!(m.line > 0, "Line numbers should be 1-based");
                assert!(
                    m.line_content.contains("println"),
                    "Match line should contain search term"
                );
            }
        }
    }

    #[test]
    fn empty_query_returns_empty() {
        let dir = setup_test_project();
        let opts = SearchOptions::default();
        let results = search_project("", dir.path(), &opts).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn regex_search_works() {
        let dir = setup_test_project();
        let mut opts = SearchOptions::default();
        opts.regex = true;

        let results = search_project("fun \\w+\\(", dir.path(), &opts).unwrap();
        assert!(!results.is_empty(), "Regex should match function declarations");
    }

    #[test]
    fn glob_matcher() {
        let path = Path::new("/project/src/Main.kt");
        assert!(matches_glob(path, "*.kt"));
        assert!(!matches_glob(path, "*.java"));
        assert!(matches_glob(path, "*.kt, *.java"));
        assert!(matches_glob(path, "Main"));
    }
}
