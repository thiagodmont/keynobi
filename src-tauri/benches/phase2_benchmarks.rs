/// Phase 2 micro-benchmarks: tree-sitter parsing and project search.
///
/// Run with:
///   cd src-tauri && cargo bench --bench phase2_benchmarks
use android_ide_lib::services::{
    search_engine,
    treesitter::TreeSitterService,
};
use android_ide_lib::models::search::SearchOptions;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

// ── Kotlin source fixtures ──────────────────────────────────────────────────

fn make_kotlin_source(line_count: usize) -> String {
    let mut lines = Vec::with_capacity(line_count);
    lines.push("package com.example.bench".to_string());
    lines.push(String::new());
    lines.push("class BenchClass {".to_string());

    let methods_needed = (line_count.saturating_sub(5)) / 4;
    for i in 0..methods_needed {
        lines.push(format!("    fun method{i}(x: Int): Int {{"));
        lines.push(format!("        val result = x * {i} + 1"));
        lines.push("        return result".to_string());
        lines.push("    }".to_string());
    }

    lines.push("}".to_string());

    while lines.len() < line_count {
        lines.push(String::new());
    }

    lines.join("\n")
}

fn make_search_project(file_count: usize) -> TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    let root = dir.path();

    let modules = ["app", "core", "feature-login", "feature-home"];
    let per_module = (file_count / modules.len()).max(1);

    for module in &modules {
        let src = root
            .join(module)
            .join("src/main/kotlin/com/example")
            .join(module.replace('-', "_"));
        fs::create_dir_all(&src).unwrap();

        for i in 0..per_module {
            fs::write(
                src.join(format!("Feature{i}.kt")),
                format!(
                    "package com.example\n\nclass Feature{i} {{\n    fun doWork() = println(\"hello world\")\n}}"
                ),
            )
            .unwrap();
        }
    }

    dir
}

// ── Tree-sitter benchmarks ──────────────────────────────────────────────────

fn bench_treesitter_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("treesitter_parse");

    for line_count in [500, 1000, 2000] {
        let source = make_kotlin_source(line_count);
        let path = Path::new("/bench/Main.kt");

        group.bench_with_input(
            BenchmarkId::new("lines", line_count),
            &source,
            |b, src| {
                b.iter(|| {
                    let mut svc = TreeSitterService::new();
                    black_box(svc.parse_file(black_box(path), black_box(src)));
                });
            },
        );
    }

    group.finish();
}

fn bench_treesitter_reparse(c: &mut Criterion) {
    let source = make_kotlin_source(1000);
    let path = Path::new("/bench/Main.kt");

    let mut svc = TreeSitterService::new();
    svc.parse_file(path, &source);

    let modified = source.replace("method0", "renamedMethod0");

    c.bench_function("treesitter_reparse_1000_lines", |b| {
        b.iter(|| {
            black_box(svc.reparse_file(black_box(path), black_box(&modified)));
        });
    });
}

fn bench_treesitter_extract_symbols(c: &mut Criterion) {
    let source = make_kotlin_source(1000);
    let path = Path::new("/bench/Main.kt");

    let mut svc = TreeSitterService::new();
    svc.parse_file(path, &source);

    c.bench_function("treesitter_extract_symbols", |b| {
        b.iter(|| {
            black_box(svc.extract_symbols(black_box(path), black_box(&source)));
        });
    });
}

// ── Search benchmarks ───────────────────────────────────────────────────────

fn bench_search_project(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_project");

    for file_count in [100, 500, 1000] {
        let dir = make_search_project(file_count);
        let root = dir.path().to_path_buf();
        let opts = SearchOptions::default();

        group.bench_with_input(
            BenchmarkId::new("literal_files", file_count),
            &(root.clone(), opts.clone()),
            |b, (root, opts)| {
                b.iter(|| {
                    black_box(
                        search_engine::search_project(
                            black_box("hello"),
                            black_box(root),
                            black_box(opts),
                        )
                    );
                });
            },
        );
    }

    group.finish();
}

fn bench_search_regex(c: &mut Criterion) {
    let dir = make_search_project(500);
    let root = dir.path().to_path_buf();
    let opts = SearchOptions {
        regex: true,
        ..SearchOptions::default()
    };

    c.bench_function("search_regex_500_files", |b| {
        b.iter(|| {
            black_box(
                search_engine::search_project(
                    black_box("fun \\w+\\("),
                    black_box(&root),
                    black_box(&opts),
                )
            );
        });
    });
}

criterion_group!(
    phase2_benches,
    bench_treesitter_parse,
    bench_treesitter_reparse,
    bench_treesitter_extract_symbols,
    bench_search_project,
    bench_search_regex,
);
criterion_main!(phase2_benches);
