/// File-system micro-benchmarks.
///
/// Run with:
///   cd src-tauri && cargo bench
///
/// HTML reports are written to target/criterion/report/index.html.
use android_ide_lib::services::fs_manager::{build_file_tree, expand_directory};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs;
use tempfile::TempDir;

// ── Synthetic project fixtures ────────────────────────────────────────────────

/// Create a synthetic Android-like project with a configurable number of Kotlin files
/// spread across multiple packages and modules.
fn make_synthetic_project(file_count: usize) -> TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    let root = dir.path();

    let modules = ["app", "core", "feature-login", "feature-home"];
    let per_module = (file_count / modules.len()).max(1);

    for module in &modules {
        let src_root = root
            .join(module)
            .join("src/main/kotlin/com/example")
            .join(module.replace('-', "_"));
        fs::create_dir_all(&src_root).unwrap();

        for i in 0..per_module {
            fs::write(
                src_root.join(format!("Feature{i}.kt")),
                format!("package com.example\n\nclass Feature{i}"),
            )
            .unwrap();
        }

        // Also add a build.gradle.kts
        fs::write(
            root.join(module).join("build.gradle.kts"),
            format!("plugins {{ id(\"com.android.library\") }}\n// {module}"),
        )
        .unwrap();
    }

    // Root-level files
    fs::write(root.join("settings.gradle.kts"), "rootProject.name = \"bench-app\"").unwrap();
    fs::write(root.join("build.gradle.kts"), "// Root build").unwrap();

    // Simulate compiled output that must be excluded
    let build_dir = root.join("app").join("build").join("intermediates");
    fs::create_dir_all(&build_dir).unwrap();
    for i in 0..50 {
        fs::write(build_dir.join(format!("File{i}.class")), "compiled").unwrap();
    }

    dir
}

// ── build_file_tree benchmarks ────────────────────────────────────────────────

fn bench_build_file_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("build_file_tree");

    for file_count in [100, 500, 1000] {
        let dir = make_synthetic_project(file_count);
        let root = dir.path().to_path_buf();

        group.bench_with_input(
            BenchmarkId::new("files", file_count),
            &root,
            |b, root| {
                b.iter(|| {
                    black_box(build_file_tree(black_box(root)));
                });
            },
        );
    }

    group.finish();
}

// ── expand_directory benchmarks ───────────────────────────────────────────────

fn bench_expand_directory(c: &mut Criterion) {
    let mut group = c.benchmark_group("expand_directory");

    // Benchmark expanding the root of various project sizes.
    for file_count in [100, 500, 1000] {
        let dir = make_synthetic_project(file_count);
        let root = dir.path().to_path_buf();

        group.bench_with_input(
            BenchmarkId::new("root_children", file_count),
            &root,
            |b, root| {
                b.iter(|| {
                    black_box(expand_directory(black_box(root)));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_build_file_tree, bench_expand_directory);
criterion_main!(benches);
