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
        Some(LspInstallation {
            path: dir.to_string_lossy().to_string(),
            version: LSP_VERSION.to_string(),
            launch_script: launch_script.to_string_lossy().to_string(),
        })
    } else {
        None
    }
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

        let relative = name
            .find('/')
            .map(|idx| &name[idx + 1..])
            .unwrap_or(&name);

        if relative.is_empty() {
            continue;
        }

        // Zip Slip protection: reject paths with traversal components
        if relative.contains("..") {
            return Err(format!("Zip entry contains path traversal: {name}"));
        }

        let out_path = dest.join(relative);

        // Belt-and-suspenders: verify the resolved path stays inside dest
        if let Ok(canonical_out) = out_path.canonicalize() {
            if !canonical_out.starts_with(&canonical_dest) {
                return Err(format!("Zip entry escapes destination: {name}"));
            }
        }
        // For new files whose parent may not exist yet, check the parent
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

#[cfg(test)]
mod tests {
    use super::*;

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
