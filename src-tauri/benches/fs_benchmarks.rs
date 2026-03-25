/// File-system micro-benchmarks.
///
/// Run with:
///   cd src-tauri && cargo bench
///
/// HTML reports are written to target/criterion/report/index.html.
/// Results are picked up by `npm run perf:collect` from target/criterion/*/new/estimates.json.
use android_ide_lib::services::fs_manager::{build_file_tree, expand_directory, read_file, write_file};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// ── Synthetic project fixtures ────────────────────────────────────────────────

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
                format!("package com.example\n\nclass Feature{i} {{\n    fun doWork() = Unit\n}}"),
            )
            .unwrap();
        }

        fs::write(
            root.join(module).join("build.gradle.kts"),
            format!("plugins {{ id(\"com.android.library\") }}\n// {module}"),
        )
        .unwrap();
    }

    fs::write(root.join("settings.gradle.kts"), "rootProject.name = \"bench-app\"").unwrap();
    fs::write(root.join("build.gradle.kts"), "// Root build").unwrap();

    let build_dir = root.join("app").join("build").join("intermediates");
    fs::create_dir_all(&build_dir).unwrap();
    for i in 0..50 {
        fs::write(build_dir.join(format!("File{i}.class")), "compiled").unwrap();
    }

    dir
}

/// Create a file of a specific size for read/write benchmarks.
fn make_sized_file(size_bytes: usize) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("benchmark_file.kt");
    let content: String = "val x = 42\n".repeat(size_bytes / 11 + 1);
    fs::write(&path, &content[..size_bytes]).unwrap();
    (dir, path)
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

// ── read_file benchmarks ──────────────────────────────────────────────────────

fn bench_read_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_file");

    for (label, size) in [("1KB", 1024), ("10KB", 10240), ("100KB", 102400), ("1MB", 1_048_576)] {
        let (_dir, path) = make_sized_file(size);

        group.bench_with_input(
            BenchmarkId::new("size", label),
            &path,
            |b, path| {
                b.iter(|| {
                    black_box(read_file(black_box(path)).unwrap());
                });
            },
        );
    }

    group.finish();
}

// ── write_file benchmarks ─────────────────────────────────────────────────────

fn bench_write_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("write_file");

    for (label, size) in [("1KB", 1024), ("10KB", 10240), ("100KB", 102400), ("1MB", 1_048_576)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("write_bench.kt");
        let content: String = "val x = 42\n".repeat(size / 11 + 1);
        let content = &content[..size];

        group.bench_with_input(
            BenchmarkId::new("size", label),
            &(path.clone(), content.to_string()),
            |b, (path, content)| {
                b.iter(|| {
                    write_file(black_box(path), black_box(content)).unwrap();
                });
            },
        );
    }

    group.finish();
}

// ── canonicalize benchmark (simulates ensure_within_project cost) ─────────────

fn bench_canonicalize(c: &mut Criterion) {
    let mut group = c.benchmark_group("canonicalize");

    let dir = make_synthetic_project(100);
    let root = dir.path().to_path_buf();
    let nested = root.join("app/src/main/kotlin/com/example/app/Feature0.kt");

    group.bench_function("nested_path", |b| {
        b.iter(|| {
            let _ = black_box(nested.canonicalize());
            let _ = black_box(root.canonicalize());
        });
    });

    group.bench_function("root_path", |b| {
        b.iter(|| {
            let _ = black_box(root.canonicalize());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_build_file_tree,
    bench_expand_directory,
    bench_read_file,
    bench_write_file,
    bench_canonicalize,
);
criterion_main!(benches);
