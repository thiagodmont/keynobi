//! Where a build came from, recorded when a successful build finishes: the git
//! commit checked out and whether files differed from it, the SHA-256 of the
//! build files, and the Gradle and JDK versions. Debug sessions keep it with
//! their build, so two sessions can say whether their builds share a source.
//!
//! Nothing here fails or holds up a build for long: git runs once, with an
//! argument vector and [`GIT_STATUS_TIMEOUT`]; build files are read only
//! inside the Gradle root and up to a size; what cannot be read is left out,
//! and why git gave nothing is recorded.

use crate::models::build::{BuildFileHash, BuildProvenance};
use crate::models::settings::AppSettings;
use crate::services::{gradle_modules, jdk, settings_manager};
use crate::utils::path::resolve_project_file;
use crate::utils::process::{
    describe_failure, first_line, output_with_timeout, GIT_STATUS_TIMEOUT,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Most build files hashed for one build.
pub const MAX_PROVENANCE_BUILD_FILES: usize = 16;
/// Largest build file hashed; a larger one is left out.
pub const MAX_HASHED_BUILD_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Longest branch name kept; a longer one is not recorded.
pub const MAX_BRANCH_CHARS: usize = 255;
/// Most of the wrapper properties read for the Gradle version.
const MAX_WRAPPER_PROPERTIES_BYTES: u64 = 64 * 1024;

/// The build setup files, relative to the Gradle root. The application
/// modules' build files follow them.
const SETUP_FILES: &[&str] = &[
    "settings.gradle.kts",
    "settings.gradle",
    "build.gradle.kts",
    "build.gradle",
    "gradle.properties",
    "gradle/libs.versions.toml",
    "gradle/wrapper/gradle-wrapper.properties",
];
const WRAPPER_PROPERTIES: &str = "gradle/wrapper/gradle-wrapper.properties";

/// Install locations of git tried after the process `PATH`.
const GIT_LOCATIONS: &[&str] = &[
    "/opt/homebrew/bin/git",
    "/usr/local/bin/git",
    "/Library/Developer/CommandLineTools/usr/bin/git",
    "/Applications/Xcode.app/Contents/Developer/usr/bin/git",
];

/// Where git is: the process `PATH`, then the usual install locations.
/// `/usr/bin/git` is skipped: without Xcode's command-line tools it is a stub
/// that asks the user to install them.
pub fn find_git() -> Option<PathBuf> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    path_var
        .split(':')
        .filter(|dir| !dir.is_empty() && *dir != "/usr/bin")
        .map(|dir| Path::new(dir).join("git"))
        .chain(GIT_LOCATIONS.iter().map(PathBuf::from))
        .find(|path| path.is_file())
}

/// The provenance of the build of `gradle_root` that just finished.
pub async fn collect(gradle_root: PathBuf) -> BuildProvenance {
    let settings = tokio::task::spawn_blocking(|| settings_manager::load_settings().0)
        .await
        .unwrap_or_default();
    collect_with(gradle_root, find_git(), settings).await
}

/// [`collect`] with `git` (`None`: not found) and `settings` given.
pub async fn collect_with(
    gradle_root: PathBuf,
    git: Option<PathBuf>,
    settings: AppSettings,
) -> BuildProvenance {
    let root = gradle_root.clone();
    let setup = tokio::task::spawn_blocking(move || read_build_setup(&root, &settings));
    let (git, setup) = tokio::join!(git_state(git.as_deref(), &gradle_root), setup);
    let setup = setup.unwrap_or_else(|e| {
        tracing::warn!("Build files not hashed: {e}");
        BuildSetup::default()
    });
    BuildProvenance {
        commit: git.commit,
        branch: git.branch,
        dirty: git.dirty,
        changed_files: git.changed_files,
        git_unavailable: git.unavailable,
        build_files: setup.files,
        gradle_version: setup.gradle_version,
        jdk_version: setup.jdk_version,
    }
}

// ── git ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, PartialEq, Eq)]
struct GitState {
    commit: Option<String>,
    branch: Option<String>,
    dirty: Option<bool>,
    changed_files: Option<u32>,
    unavailable: Option<String>,
}

impl GitState {
    fn unavailable(reason: impl Into<String>) -> Self {
        GitState {
            unavailable: Some(reason.into()),
            ..Default::default()
        }
    }
}

/// `git status` of the repository `root` is in: the commit and branch, and
/// how many files differ from the commit. It never takes the repository's
/// optional locks, so it does not write to the repository.
async fn git_state(git: Option<&Path>, root: &Path) -> GitState {
    let Some(git) = git else {
        return GitState::unavailable("git was not found");
    };
    let out = output_with_timeout(
        tokio::process::Command::new(git)
            .arg("-C")
            .arg(root)
            .args([
                "-c",
                "core.fsmonitor=false",
                "status",
                "--porcelain=v2",
                "--branch",
                "--no-renames",
                "-z",
            ])
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
        GIT_STATUS_TIMEOUT,
    )
    .await;
    let out = match out {
        Ok(out) => out,
        Err(e) => {
            return GitState::unavailable(describe_failure(
                "git status",
                &e,
                "the repository may be too large to read in time",
            ))
        }
    };
    if !out.status.success() {
        let message = first_line(&out.stderr).unwrap_or_default();
        return GitState::unavailable(if message.contains("not a git repository") {
            "not a git repository".to_string()
        } else {
            format!("git status failed: {message}")
        });
    }
    parse_status(&out.stdout)
}

/// Parse `git status --porcelain=v2 --branch -z`.
fn parse_status(stdout: &[u8]) -> GitState {
    let text = String::from_utf8_lossy(stdout);
    let mut state = GitState::default();
    let mut changed: u32 = 0;
    for record in text.split('\0').filter(|r| !r.is_empty()) {
        if let Some(oid) = record.strip_prefix("# branch.oid ") {
            state.commit = is_object_id(oid).then(|| oid.to_ascii_lowercase());
        } else if let Some(head) = record.strip_prefix("# branch.head ") {
            state.branch = (head != "(detached)"
                && head.chars().count() <= MAX_BRANCH_CHARS
                && !head.chars().any(char::is_control))
            .then(|| head.to_string());
        } else if !record.starts_with('#') {
            changed = changed.saturating_add(1);
        }
    }
    state.dirty = Some(changed > 0);
    state.changed_files = Some(changed);
    state
}

/// A full SHA-1 or SHA-256 object id.
fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.chars().all(|c| c.is_ascii_hexdigit())
}

// ── Build files ───────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct BuildSetup {
    files: Vec<BuildFileHash>,
    gradle_version: Option<String>,
    jdk_version: Option<String>,
}

/// Hash the build files of `gradle_root` and read the Gradle and JDK versions.
fn read_build_setup(gradle_root: &Path, settings: &AppSettings) -> BuildSetup {
    let Ok(root) = gradle_root.canonicalize() else {
        return BuildSetup::default();
    };
    let mut candidates: Vec<String> = SETUP_FILES.iter().map(|f| f.to_string()).collect();
    for module in gradle_modules::application_modules(&root) {
        let dir = module.relative_dir(&root);
        if dir != "." {
            candidates.push(format!("{dir}/build.gradle.kts"));
            candidates.push(format!("{dir}/build.gradle"));
        }
    }
    let mut files: Vec<BuildFileHash> = Vec::new();
    for relative in candidates {
        let Ok(path) = resolve_project_file(&root, &relative) else {
            continue;
        };
        if files.len() >= MAX_PROVENANCE_BUILD_FILES {
            tracing::warn!(
                "{relative} was not hashed: more than {MAX_PROVENANCE_BUILD_FILES} build files"
            );
            continue;
        }
        match hash_file(&path) {
            Ok(sha256) => files.push(BuildFileHash {
                path: relative,
                sha256,
            }),
            Err(e) => tracing::warn!("{relative} was not hashed: {e}"),
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    BuildSetup {
        files,
        gradle_version: resolve_project_file(&root, WRAPPER_PROPERTIES)
            .ok()
            .and_then(|path| read_capped(&path, MAX_WRAPPER_PROPERTIES_BYTES).ok())
            .and_then(|bytes| wrapper_gradle_version(&String::from_utf8_lossy(&bytes))),
        jdk_version: jdk::gradle_jdk_version(settings, &root),
    }
}

fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = read_capped(path, MAX_HASHED_BUILD_FILE_BYTES)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The whole file, refused when it is larger than `max` bytes.
fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > max {
        return Err(format!("larger than {max} bytes"));
    }
    Ok(bytes)
}

/// The version in the wrapper's `distributionUrl`
/// (`…/distributions/gradle-8.7-bin.zip` → `8.7`).
fn wrapper_gradle_version(properties: &str) -> Option<String> {
    const MAX_VERSION_CHARS: usize = 32;
    let url = properties.lines().rev().find_map(|line| {
        let rest = line.trim_start().strip_prefix("distributionUrl")?;
        let rest = rest.trim_start().strip_prefix(['=', ':'])?;
        Some(rest.trim().to_string())
    })?;
    let stem = url
        .rsplit('/')
        .next()?
        .strip_prefix("gradle-")?
        .strip_suffix(".zip")?;
    let version = stem
        .strip_suffix("-bin")
        .or_else(|| stem.strip_suffix("-all"))?;
    (!version.is_empty()
        && version.len() <= MAX_VERSION_CHARS
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
    .then(|| version.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Git for building test repositories, isolated from the user's and the
    /// system's configuration.
    fn git(dir: &Path, home: &Path, args: &[&str]) {
        let git = find_git().expect("git is needed for these tests");
        let status = Command::new(git)
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .env("HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }

    struct Project {
        _dir: TempDir,
        root: PathBuf,
        home: PathBuf,
    }

    impl Project {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let root = dir.path().canonicalize().unwrap().join("project");
            let home = dir.path().canonicalize().unwrap().join("home");
            std::fs::create_dir_all(root.join("app")).unwrap();
            std::fs::create_dir_all(root.join("gradle/wrapper")).unwrap();
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(
                root.join("settings.gradle.kts"),
                "rootProject.name = \"demo\"\ninclude(\":app\")\n",
            )
            .unwrap();
            std::fs::write(
                root.join("app/build.gradle.kts"),
                "plugins { id(\"com.android.application\") }\n",
            )
            .unwrap();
            std::fs::write(
                root.join("gradle/libs.versions.toml"),
                "[versions]\nagp = \"8.5.0\"\n",
            )
            .unwrap();
            std::fs::write(
                root.join(WRAPPER_PROPERTIES),
                "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.7-bin.zip\n",
            )
            .unwrap();
            Project {
                _dir: dir,
                root,
                home,
            }
        }

        fn git(&self, args: &[&str]) {
            git(&self.root, &self.home, args);
        }

        fn committed(self) -> Self {
            self.git(&["init", "-q"]);
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", "first"]);
            self
        }

        async fn provenance(&self) -> BuildProvenance {
            collect_with(self.root.clone(), find_git(), AppSettings::default()).await
        }
    }

    #[tokio::test]
    async fn a_clean_repository_records_its_commit_and_branch() {
        let project = Project::new().committed();
        let p = project.provenance().await;
        let commit = p.commit.clone().expect("a commit");
        assert!(is_object_id(&commit), "{commit}");
        assert_eq!(p.branch.as_deref(), Some("main"));
        assert_eq!(p.dirty, Some(false));
        assert_eq!(p.changed_files, Some(0));
        assert_eq!(p.git_unavailable, None);
        assert_eq!(p.gradle_version.as_deref(), Some("8.7"));
        let paths: Vec<&str> = p.build_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "app/build.gradle.kts",
                "gradle/libs.versions.toml",
                "gradle/wrapper/gradle-wrapper.properties",
                "settings.gradle.kts",
            ]
        );
        assert_eq!(
            p.build_files[1].sha256,
            sha256_hex(b"[versions]\nagp = \"8.5.0\"\n")
        );
    }

    #[tokio::test]
    async fn changed_and_untracked_files_make_the_tree_dirty() {
        let project = Project::new().committed();
        let before = project.provenance().await;
        std::fs::write(
            project.root.join("gradle/libs.versions.toml"),
            "[versions]\nagp = \"8.6.0\"\n",
        )
        .unwrap();
        std::fs::write(project.root.join("notes.txt"), "new\n").unwrap();
        let after = project.provenance().await;
        assert_eq!(after.commit, before.commit);
        assert_eq!(after.dirty, Some(true));
        assert_eq!(after.changed_files, Some(2));
        let catalog = |p: &BuildProvenance| {
            p.build_files
                .iter()
                .find(|f| f.path == "gradle/libs.versions.toml")
                .map(|f| f.sha256.clone())
        };
        assert_ne!(catalog(&after), catalog(&before));

        // Committing it gives a new, clean commit.
        project.git(&["add", "-A"]);
        project.git(&["commit", "-q", "-m", "second"]);
        let committed = project.provenance().await;
        assert_ne!(committed.commit, before.commit);
        assert_eq!(committed.dirty, Some(false));
    }

    #[tokio::test]
    async fn a_detached_head_has_no_branch() {
        let project = Project::new().committed();
        project.git(&["checkout", "-q", "--detach"]);
        let p = project.provenance().await;
        assert!(p.commit.is_some());
        assert_eq!(p.branch, None);
    }

    #[tokio::test]
    async fn a_project_outside_a_repository_says_so_and_still_hashes_its_files() {
        let project = Project::new();
        let p = project.provenance().await;
        assert_eq!(p.commit, None);
        assert_eq!(p.dirty, None);
        assert_eq!(p.git_unavailable.as_deref(), Some("not a git repository"));
        assert_eq!(p.build_files.len(), 4);
    }

    #[tokio::test]
    async fn missing_git_is_recorded_and_the_files_are_still_hashed() {
        let project = Project::new().committed();
        let fake = project.home.join("no-such-git");
        let p = collect_with(project.root.clone(), Some(fake), AppSettings::default()).await;
        assert_eq!(p.commit, None);
        assert_eq!(p.branch, None);
        assert_eq!(p.dirty, None);
        let reason = p.git_unavailable.expect("a reason");
        assert!(reason.starts_with("git status failed:"), "{reason}");
        assert_eq!(p.build_files.len(), 4);

        let p = collect_with(project.root.clone(), None, AppSettings::default()).await;
        assert_eq!(p.git_unavailable.as_deref(), Some("git was not found"));
    }

    #[test]
    fn build_files_outside_the_project_or_too_large_are_left_out() {
        let project = Project::new();
        let outside = project.home.join("outside.gradle.kts");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, project.root.join("build.gradle.kts")).unwrap();
        let big = vec![b'#'; MAX_HASHED_BUILD_FILE_BYTES as usize + 1];
        std::fs::write(project.root.join("gradle.properties"), big).unwrap();
        let setup = read_build_setup(&project.root, &AppSettings::default());
        let paths: Vec<&str> = setup.files.iter().map(|f| f.path.as_str()).collect();
        assert!(!paths.contains(&"build.gradle.kts"), "{paths:?}");
        assert!(!paths.contains(&"gradle.properties"), "{paths:?}");
        assert_eq!(paths.len(), 4, "{paths:?}");
    }

    #[test]
    fn at_most_the_capped_number_of_build_files_is_hashed() {
        let project = Project::new();
        let mut settings = String::from("include(\":app\")\n");
        for i in 0..MAX_PROVENANCE_BUILD_FILES {
            let dir = project.root.join(format!("app{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("build.gradle.kts"),
                "plugins { id(\"com.android.application\") }\n",
            )
            .unwrap();
            settings.push_str(&format!("include(\":app{i}\")\n"));
        }
        std::fs::write(project.root.join("settings.gradle.kts"), settings).unwrap();
        let setup = read_build_setup(&project.root, &AppSettings::default());
        assert_eq!(setup.files.len(), MAX_PROVENANCE_BUILD_FILES);
    }

    #[test]
    fn porcelain_status_is_parsed() {
        let oid = "a".repeat(40);
        let out = format!(
            "# branch.oid {oid}\0# branch.head feature/x\0# branch.ab +1 -0\0\
             1 .M N... 100644 100644 100644 {oid} {oid} app/a\nb.kt\0? new.txt\0"
        );
        let state = parse_status(out.as_bytes());
        assert_eq!(state.commit, Some(oid));
        assert_eq!(state.branch.as_deref(), Some("feature/x"));
        assert_eq!(state.changed_files, Some(2));
        assert_eq!(state.dirty, Some(true));

        let state = parse_status(b"# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!(state.commit, None);
        assert_eq!(state.branch, None);
        assert_eq!(state.dirty, Some(false));

        let long = "b".repeat(MAX_BRANCH_CHARS + 1);
        let state = parse_status(format!("# branch.head {long}\0").as_bytes());
        assert_eq!(state.branch, None);
    }

    #[test]
    fn the_gradle_version_comes_from_the_distribution_url() {
        let url = |u: &str| wrapper_gradle_version(&format!("distributionUrl={u}\n"));
        assert_eq!(
            url("https\\://services.gradle.org/distributions/gradle-8.7-bin.zip").as_deref(),
            Some("8.7")
        );
        assert_eq!(
            url("https\\://services.gradle.org/distributions/gradle-8.10.2-all.zip").as_deref(),
            Some("8.10.2")
        );
        assert_eq!(
            url("https\\://example.com/gradle-8.7-rc-1-bin.zip").as_deref(),
            Some("8.7-rc-1")
        );
        assert_eq!(url("https\\://example.com/custom.zip"), None);
        assert_eq!(wrapper_gradle_version("# nothing\n"), None);
    }
}
