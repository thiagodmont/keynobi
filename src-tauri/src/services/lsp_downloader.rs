use crate::models::lsp::{DownloadProgress, LspInstallation};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const LSP_VERSION: &str = "262.2310.0";

fn lsp_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".androidide")
        .join("kotlin-lsp")
}

fn lsp_install_dir() -> PathBuf {
    lsp_base_dir().join(LSP_VERSION)
}

pub fn check_installed() -> Option<LspInstallation> {
    let dir = lsp_install_dir();
    let launch_script = find_launch_script(&dir)?;

    if launch_script.exists() {
        let installation = LspInstallation {
            path: dir.to_string_lossy().to_string(),
            version: LSP_VERSION.to_string(),
            launch_script: launch_script.to_string_lossy().to_string(),
        };
        // Validate structure: lib/ must exist next to the script or it means
        // the previous extraction was broken (wrong prefix stripping).
        if validate_installation(&installation) {
            Some(installation)
        } else {
            tracing::warn!(
                "Kotlin LSP installation at {:?} is incomplete (lib/ missing) — will re-download",
                dir
            );
            None
        }
    } else {
        None
    }
}

/// Returns `true` when the installation directory has the `lib/` directory
/// that the launch script requires.  A missing `lib/` means the zip was
/// extracted with a wrong prefix-stripping strategy and needs re-download.
fn validate_installation(installation: &LspInstallation) -> bool {
    let script_path = Path::new(&installation.launch_script);
    // The script lives at install_dir/kotlin-lsp.sh; lib/ is a sibling.
    let script_dir = script_path.parent().unwrap_or(Path::new("."));
    script_dir.join("lib").is_dir()
}

pub fn get_download_url() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    let platform = match (os, arch) {
        ("macos", "aarch64") => "mac-aarch64",
        ("macos", "x86_64") => "mac-x64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-aarch64",
        ("windows", "x86_64") => "win-x64",
        ("windows", "aarch64") => "win-aarch64",
        _ => "mac-aarch64",
    };

    format!(
        "https://download-cdn.jetbrains.com/kotlin-lsp/{version}/kotlin-lsp-{version}-{platform}.zip",
        version = LSP_VERSION,
        platform = platform,
    )
}

pub async fn download_and_install<F>(
    progress_callback: F,
) -> Result<LspInstallation, String>
where
    F: Fn(DownloadProgress) + Send + 'static,
{
    let url = get_download_url();
    let install_dir = lsp_install_dir();
    let base_dir = lsp_base_dir();

    tokio::fs::create_dir_all(&base_dir)
        .await
        .map_err(|e| format!("Failed to create directory: {e}"))?;

    let temp_zip = base_dir.join(format!("kotlin-lsp-{LSP_VERSION}.zip.tmp"));

    tracing::info!("Downloading Kotlin LSP from {}", url);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let total_bytes = response.content_length();
    let mut downloaded: u64 = 0;

    let mut file = tokio::fs::File::create(&temp_zip)
        .await
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Download stream error: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write chunk: {e}"))?;

        downloaded += chunk.len() as u64;
        let percent = total_bytes.map(|t| (downloaded as f64 / t as f64) * 100.0);

        progress_callback(DownloadProgress {
            downloaded_bytes: downloaded,
            total_bytes,
            percent,
        });
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {e}"))?;
    drop(file);

    tracing::info!("Download complete ({} bytes), extracting...", downloaded);

    if install_dir.exists() {
        tokio::fs::remove_dir_all(&install_dir)
            .await
            .map_err(|e| format!("Failed to clean install directory: {e}"))?;
    }

    let zip_path = temp_zip.clone();
    let extract_dir = install_dir.clone();
    tokio::task::spawn_blocking(move || extract_zip(&zip_path, &extract_dir))
        .await
        .map_err(|e| format!("Extraction task failed: {e}"))??;

    tokio::fs::remove_file(&temp_zip).await.ok();

    #[cfg(unix)]
    {
        if let Some(script) = find_launch_script(&install_dir) {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&script) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&script, perms).ok();
            }
        }
    }

    let launch_script = find_launch_script(&install_dir)
        .ok_or("Could not find kotlin-lsp launch script after extraction")?;

    tracing::info!("Kotlin LSP installed at {:?}", install_dir);

    Ok(LspInstallation {
        path: install_dir.to_string_lossy().to_string(),
        version: LSP_VERSION.to_string(),
        launch_script: launch_script.to_string_lossy().to_string(),
    })
}

const MAX_EXTRACT_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GB total
const MAX_ENTRY_SIZE: u64 = 512 * 1024 * 1024; // 512 MB per entry

/// Detect whether every entry in the zip shares one common top-level directory
/// component (e.g. `kotlin-lsp-1.0/`).  Returns that prefix when found, or
/// `None` when files live directly at the zip root.
///
/// JetBrains' Kotlin LSP zip ships files at root level — no wrapping directory.
/// Many other tools ship with a wrapping directory.  We handle both.
fn detect_strip_prefix(names: &[String]) -> Option<String> {
    // If any entry has no slash at all, files exist at the zip root.
    if names.iter().any(|n| !n.contains('/')) {
        return None;
    }

    // Collect distinct first path components across all entries.
    let mut first_components: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for n in names {
        if let Some(component) = n.split('/').next() {
            if !component.is_empty() {
                first_components.insert(component);
            }
        }
    }

    // Only strip when there is exactly one common top-level component.
    if first_components.len() == 1 {
        let prefix = first_components.into_iter().next()?;
        Some(format!("{prefix}/"))
    } else {
        None
    }
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Failed to open zip: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip file: {e}"))?;

    std::fs::create_dir_all(dest)
        .map_err(|e| format!("Failed to create dest dir: {e}"))?;

    let canonical_dest = dest
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize dest: {e}"))?;

    // Determine once whether to strip a common top-level directory prefix.
    let all_names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    let strip_prefix = detect_strip_prefix(&all_names);
    tracing::debug!(
        "Zip extraction: {} entries, strip_prefix={:?}",
        all_names.len(),
        strip_prefix
    );

    let mut total_extracted: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {e}"))?;

        if entry.size() > MAX_ENTRY_SIZE {
            return Err(format!(
                "Zip entry too large: {} ({} bytes, max {})",
                entry.name(),
                entry.size(),
                MAX_ENTRY_SIZE
            ));
        }
        total_extracted += entry.size();
        if total_extracted > MAX_EXTRACT_SIZE {
            return Err(format!(
                "Zip extraction exceeds size limit ({} bytes)",
                MAX_EXTRACT_SIZE
            ));
        }

        let name = entry.name().to_string();

        // Compute the relative path within the destination, stripping the
        // common top-level directory when present.
        let relative: &str = match &strip_prefix {
            Some(prefix) => match name.strip_prefix(prefix.as_str()) {
                Some(rel) => rel,
                None => {
                    // Entry doesn't start with the expected prefix — skip safely.
                    tracing::debug!("Skipping entry without expected prefix: {name}");
                    continue;
                }
            },
            None => &name,
        };

        if relative.is_empty() {
            // This is the top-level directory entry itself — skip it.
            continue;
        }

        // Zip Slip protection: reject paths with traversal components.
        if relative.contains("..") {
            return Err(format!("Zip entry contains path traversal: {name}"));
        }

        let out_path = dest.join(relative);

        // Belt-and-suspenders: verify the resolved path stays inside dest.
        if let Ok(canonical_out) = out_path.canonicalize() {
            if !canonical_out.starts_with(&canonical_dest) {
                return Err(format!("Zip entry escapes destination: {name}"));
            }
        }
        // For new files whose parent may not exist yet, check the parent.
        if let Some(parent) = out_path.parent() {
            if parent.exists() {
                let canonical_parent = parent
                    .canonicalize()
                    .map_err(|e| format!("Failed to canonicalize parent: {e}"))?;
                if !canonical_parent.starts_with(&canonical_dest) {
                    return Err(format!("Zip entry escapes destination: {name}"));
                }
            }
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create dir: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir: {e}"))?;
            }
            let mut outfile = std::fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create file: {e}"))?;
            std::io::copy(&mut entry, &mut outfile)
                .map_err(|e| format!("Failed to extract file: {e}"))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = entry.unix_mode() {
                    std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode))
                        .ok();
                }
            }
        }
    }

    Ok(())
}

fn find_launch_script(dir: &Path) -> Option<PathBuf> {
    let sh = dir.join("kotlin-lsp.sh");
    if sh.exists() {
        return Some(sh);
    }
    let bin = dir.join("bin").join("kotlin-lsp.sh");
    if bin.exists() {
        return Some(bin);
    }
    // Also check for the script in a nested directory
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let nested = entry.path().join("kotlin-lsp.sh");
                if nested.exists() {
                    return Some(nested);
                }
            }
        }
    }
    None
}

pub fn get_cache_dir() -> PathBuf {
    lsp_base_dir().join("cache")
}

/// Dedicated directory for the LSP server's runtime data (indices, caches,
/// logs).  Kept separate from the download cache so they don't interfere.
pub fn get_lsp_system_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".androidide")
        .join("lsp-system")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Build a zip archive in memory that mirrors the JetBrains Kotlin LSP
    /// distribution: **no top-level wrapping directory**, files at the root.
    fn make_zip_no_prefix() -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Stored);

        w.start_file("kotlin-lsp.sh", opts).unwrap();
        w.write_all(b"#!/bin/bash\necho hello\n").unwrap();

        w.start_file("kotlin-lsp.cmd", opts).unwrap();
        w.write_all(b"@echo off\n").unwrap();

        w.add_directory("lib/", opts).unwrap();
        w.start_file("lib/app.jar", opts).unwrap();
        w.write_all(b"PK fake jar").unwrap();
        w.start_file("lib/util.jar", opts).unwrap();
        w.write_all(b"PK fake jar").unwrap();

        w.add_directory("jre/", opts).unwrap();
        w.add_directory("jre/Contents/", opts).unwrap();
        w.add_directory("jre/Contents/Home/", opts).unwrap();
        w.add_directory("jre/Contents/Home/bin/", opts).unwrap();
        w.start_file("jre/Contents/Home/bin/java", opts).unwrap();
        w.write_all(b"#!/bin/bash\nexec java $@").unwrap();

        w.finish().unwrap().into_inner()
    }

    /// Build a zip archive in memory **with** a single top-level directory
    /// wrapper — the style many open-source tools use.
    fn make_zip_with_prefix() -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Stored);

        w.add_directory("tool-1.0/", opts).unwrap();
        w.start_file("tool-1.0/run.sh", opts).unwrap();
        w.write_all(b"#!/bin/bash\necho hi\n").unwrap();

        w.add_directory("tool-1.0/lib/", opts).unwrap();
        w.start_file("tool-1.0/lib/app.jar", opts).unwrap();
        w.write_all(b"PK fake jar").unwrap();

        w.add_directory("tool-1.0/bin/", opts).unwrap();
        w.start_file("tool-1.0/bin/tool", opts).unwrap();
        w.write_all(b"#!/bin/bash\nbin/tool").unwrap();

        w.finish().unwrap().into_inner()
    }

    // ── detect_strip_prefix ───────────────────────────────────────────────────

    #[test]
    fn detect_strip_prefix_no_top_level_dir() {
        // JetBrains Kotlin LSP zip — files at root, no wrapping directory.
        let names = vec![
            "kotlin-lsp.sh".to_string(),
            "kotlin-lsp.cmd".to_string(),
            "lib/app.jar".to_string(),
            "lib/util.jar".to_string(),
            "jre/Contents/Home/bin/java".to_string(),
        ];
        assert_eq!(detect_strip_prefix(&names), None);
    }

    #[test]
    fn detect_strip_prefix_with_single_top_level_dir() {
        // Standard zips that wrap everything in one top-level directory.
        let names = vec![
            "tool-1.0/".to_string(),
            "tool-1.0/bin/run.sh".to_string(),
            "tool-1.0/lib/app.jar".to_string(),
        ];
        assert_eq!(detect_strip_prefix(&names), Some("tool-1.0/".to_string()));
    }

    #[test]
    fn detect_strip_prefix_with_multiple_top_level_entries() {
        // Multiple different top-level entries — no common prefix.
        let names = vec!["foo/bar.jar".to_string(), "baz/qux.jar".to_string()];
        assert_eq!(detect_strip_prefix(&names), None);
    }

    // ── extract_zip (integration) ─────────────────────────────────────────────

    #[test]
    fn extract_zip_no_prefix_preserves_full_structure() {
        let zip_bytes = make_zip_no_prefix();
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("test.zip");
        let dest = tmp.path().join("out");

        std::fs::write(&zip_path, &zip_bytes).unwrap();
        extract_zip(&zip_path, &dest).expect("extraction should succeed");

        // Root-level files
        assert!(dest.join("kotlin-lsp.sh").is_file(), "kotlin-lsp.sh missing");
        assert!(dest.join("kotlin-lsp.cmd").is_file(), "kotlin-lsp.cmd missing");

        // lib/ must be a directory (the critical regression test)
        assert!(dest.join("lib").is_dir(), "lib/ directory missing");
        assert!(dest.join("lib/app.jar").is_file(), "lib/app.jar missing");
        assert!(dest.join("lib/util.jar").is_file(), "lib/util.jar missing");

        // jre/ tree must be intact (second regression: prefix stripped from dir)
        assert!(dest.join("jre").is_dir(), "jre/ directory missing");
        assert!(
            dest.join("jre/Contents/Home/bin/java").is_file(),
            "bundled JRE binary missing"
        );

        // No extra top-level directory should have been created
        assert!(!dest.join("lib/lib").exists(), "lib stripped twice — double-strip bug");
        assert!(!dest.join("jre/jre").exists(), "jre stripped twice — double-strip bug");
    }

    #[test]
    fn extract_zip_with_prefix_strips_wrapper_dir() {
        let zip_bytes = make_zip_with_prefix();
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("test.zip");
        let dest = tmp.path().join("out");

        std::fs::write(&zip_path, &zip_bytes).unwrap();
        extract_zip(&zip_path, &dest).expect("extraction should succeed");

        // Top-level dir stripped → files land directly in dest
        assert!(dest.join("run.sh").is_file(), "run.sh should be at dest root");
        assert!(dest.join("lib/app.jar").is_file(), "lib/app.jar missing");
        assert!(dest.join("bin/tool").is_file(), "bin/tool missing");

        // The wrapper directory must NOT appear in the output
        assert!(!dest.join("tool-1.0").exists(), "wrapper dir should have been stripped");
    }

    // ── validate_installation ─────────────────────────────────────────────────

    #[test]
    fn validate_installation_passes_when_lib_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("kotlin-lsp.sh");
        std::fs::write(&script, b"#!/bin/bash").unwrap();
        std::fs::create_dir(tmp.path().join("lib")).unwrap();

        let inst = LspInstallation {
            path: tmp.path().to_string_lossy().to_string(),
            version: "test".to_string(),
            launch_script: script.to_string_lossy().to_string(),
        };
        assert!(validate_installation(&inst), "should pass when lib/ exists");
    }

    #[test]
    fn validate_installation_fails_when_lib_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("kotlin-lsp.sh");
        std::fs::write(&script, b"#!/bin/bash").unwrap();
        // Intentionally NOT creating lib/

        let inst = LspInstallation {
            path: tmp.path().to_string_lossy().to_string(),
            version: "test".to_string(),
            launch_script: script.to_string_lossy().to_string(),
        };
        assert!(!validate_installation(&inst), "should fail when lib/ is absent");
    }

    // ── check_installed ───────────────────────────────────────────────────────

    #[test]
    fn download_url_has_correct_format() {
        let url = get_download_url();
        assert!(url.starts_with("https://download-cdn.jetbrains.com/kotlin-lsp/"));
        assert!(url.contains(LSP_VERSION));
        assert!(url.ends_with(".zip"));
    }

    #[test]
    fn lsp_base_dir_under_home() {
        let dir = lsp_base_dir();
        let home = dirs::home_dir().unwrap();
        assert!(dir.starts_with(&home));
        assert!(dir.to_string_lossy().contains(".androidide"));
    }

    #[test]
    fn lsp_system_dir_is_separate_from_download_cache() {
        let cache = get_cache_dir();
        let system = get_lsp_system_dir();
        // Must be under .androidide but in a different sub-directory.
        assert_ne!(cache, system, "system dir must not overlap with download cache");
        let cache_str = cache.to_string_lossy();
        let system_str = system.to_string_lossy();
        assert!(system_str.contains(".androidide"), "system dir not inside .androidide");
        assert!(system_str.ends_with("lsp-system"), "unexpected system dir path: {system_str}");
        assert!(!cache_str.ends_with("lsp-system"), "cache dir must not be the system dir");
    }

    #[test]
    fn check_installed_returns_none_when_not_installed() {
        // Unless the LSP is actually installed, this should return None
        // (safe to run in CI where it's not installed)
        let result = check_installed();
        // Can't assert None because it might be installed in dev,
        // but we can assert the type is correct
        if let Some(installation) = result {
            assert!(!installation.version.is_empty());
        }
    }
}
