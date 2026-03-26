/// File-system micro-benchmarks.
///
/// Run with:
///   cd src-tauri && cargo bench --bench fs_benchmarks
use android_ide_lib::services::fs_manager::find_gradle_root;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::fs;

fn make_temp_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    let root = dir.path();
    fs::create_dir_all(root.join("app/src/main/kotlin")).unwrap();
    fs::write(root.join("settings.gradle.kts"), "rootProject.name = \"bench\"").unwrap();
    dir
}

fn bench_find_gradle_root(c: &mut Criterion) {
    let dir = make_temp_project();
    let deep = dir.path().join("app/src/main/kotlin");
    c.bench_function("find_gradle_root", |b| {
        b.iter(|| find_gradle_root(black_box(&deep)))
    });
}

criterion_group!(benches, bench_find_gradle_root);
criterion_main!(benches);
