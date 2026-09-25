//! Which JDK Gradle runs on, and whether it works.
//!
//! GUI builds, MCP builds, the Health Center, and the MCP health tools all
//! resolve the JDK here, so the GUI and a headless MCP process pick the same
//! JDK even when they inherit different environments.

use crate::models::health::JdkSource;
use crate::models::settings::AppSettings;
use crate::utils::process::{output_with_timeout, TOOL_PROBE_TIMEOUT};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The oldest JDK the Android Gradle Plugin 8 runs on.
pub const MIN_GRADLE_JDK_MAJOR: u32 = 17;

const GRADLE_JAVA_HOME_PROPERTY: &str = "org.gradle.java.home";
const ANDROID_STUDIO_APPS: [&str; 2] = ["Android Studio.app", "Android Studio Preview.app"];

/// Filesystem locations searched for a JDK. Tests pass temp dirs.
#[derive(Debug, Clone, Default)]
pub struct JdkSearchRoots {
    /// Gradle user home: `$GRADLE_USER_HOME`, else `~/.gradle`.
    pub gradle_user_home: Option<PathBuf>,
    /// Folders that may contain `Android Studio.app`.
    pub application_dirs: Vec<PathBuf>,
    /// Folder of installed JDK bundles, each with a `Contents/Home`.
    pub jvm_dir: Option<PathBuf>,
}

impl JdkSearchRoots {
    /// The real locations on this machine.
    #[cfg(not(test))]
    pub fn system() -> Self {
        let home = dirs::home_dir();
        let gradle_user_home = std::env::var_os("GRADLE_USER_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".gradle")));
        let mut application_dirs = vec![PathBuf::from("/Applications")];
        if let Some(home) = home {
            application_dirs.push(home.join("Applications"));
        }
        Self {
            gradle_user_home,
            application_dirs,
            jvm_dir: Some(PathBuf::from("/Library/Java/JavaVirtualMachines")),
        }
    }

    /// Unit tests never search the real machine.
    #[cfg(test)]
    pub fn system() -> Self {
        Self::default()
    }
}

/// The JDK home Gradle will use and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedJdk {
    pub home: PathBuf,
    pub source: JdkSource,
}

/// Resolve the JDK for Gradle builds in the project at `gradle_root`.
///
/// Order:
/// 1. `org.gradle.java.home` in the Gradle user home `gradle.properties`, then
///    in the project's. Gradle lets the user home file override the project
///    file, and the daemon runs on this JDK whatever `JAVA_HOME` says.
/// 2. The `java.home` setting.
/// 3. Android Studio's bundled runtime.
/// 4. The newest installed JDK 17 or later.
///
/// Explicit choices (1 and 2) are returned even when the path is broken, so
/// Health reports the misconfiguration instead of hiding it.
pub fn resolve_jdk(
    settings: &AppSettings,
    gradle_root: Option<&Path>,
    roots: &JdkSearchRoots,
) -> Option<ResolvedJdk> {
    let from_properties = |dir: Option<&Path>, source| {
        let home = gradle_java_home(dir?)?;
        Some(ResolvedJdk { home, source })
    };
    from_properties(
        roots.gradle_user_home.as_deref(),
        JdkSource::UserGradleProperties,
    )
    .or_else(|| from_properties(gradle_root, JdkSource::ProjectGradleProperties))
    .or_else(|| {
        let home = settings.java.home.as_deref().map(str::trim)?;
        (!home.is_empty()).then(|| ResolvedJdk {
            home: expand_tilde(home),
            source: JdkSource::Settings,
        })
    })
    .or_else(|| discover_jdk(roots))
}

/// Find a JDK 17+ without any configuration: Android Studio's bundled
/// runtime, then the newest installed JDK.
pub fn discover_jdk(roots: &JdkSearchRoots) -> Option<ResolvedJdk> {
    android_studio_jbr(&roots.application_dirs)
        .map(|home| ResolvedJdk {
            home,
            source: JdkSource::AndroidStudio,
        })
        .or_else(|| {
            newest_installed_jdk(roots.jvm_dir.as_deref()?).map(|home| ResolvedJdk {
                home,
                source: JdkSource::InstalledJdk,
            })
        })
}

/// `JAVA_HOME` for a Gradle (or SDK tool) process, or `None` to inherit it.
pub fn java_home_for_gradle(settings: &AppSettings, gradle_root: Option<&Path>) -> Option<String> {
    resolve_jdk(settings, gradle_root, &JdkSearchRoots::system())
        .map(|jdk| jdk.home.to_string_lossy().into_owned())
}

fn android_studio_jbr(application_dirs: &[PathBuf]) -> Option<PathBuf> {
    application_dirs
        .iter()
        .flat_map(|dir| ANDROID_STUDIO_APPS.iter().map(move |app| dir.join(app)))
        .map(|app| app.join("Contents/jbr/Contents/Home"))
        .find(|home| {
            // Old Android Studio releases bundled JDK 11.
            has_java(home) && release_version(home).is_none_or(|v| v[0] >= MIN_GRADLE_JDK_MAJOR)
        })
}

fn newest_installed_jdk(jvm_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(jvm_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("Contents/Home"))
        .filter(|home| has_java(home))
        .filter_map(|home| Some((release_version(&home)?, home)))
        .filter(|(version, _)| version[0] >= MIN_GRADLE_JDK_MAJOR)
        .max()
        .map(|(_, home)| home)
}

fn has_java(home: &Path) -> bool {
    home.join("bin").join("java").is_file()
}

/// Version from the JDK's `release` file (`JAVA_VERSION="17.0.9"`).
fn release_version(home: &Path) -> Option<Vec<u32>> {
    let content = std::fs::read_to_string(home.join("release")).ok()?;
    content.lines().find_map(|line| {
        let value = line.trim().strip_prefix("JAVA_VERSION=")?;
        version_components(value.trim().trim_matches('"'))
    })
}

fn gradle_java_home(dir: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(dir.join("gradle.properties")).ok()?;
    property_value(&content, GRADLE_JAVA_HOME_PROPERTY).map(PathBuf::from)
}

/// The value of `key` in a Java properties file; the last entry wins. An
/// empty value counts as unset.
fn property_value(content: &str, key: &str) -> Option<String> {
    let mut value = None;
    for line in content.lines() {
        let line = line.trim_start();
        if line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let end = line
            .find(|c: char| c == '=' || c == ':' || c.is_whitespace())
            .unwrap_or(line.len());
        if &line[..end] != key {
            continue;
        }
        let rest = line[end..].trim_start();
        let rest = rest.strip_prefix(['=', ':']).unwrap_or(rest);
        let v = unescape(rest.trim());
        value = (!v.is_empty()).then_some(v);
    }
    value
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Numeric components of a Java version with the legacy `1.` prefix dropped:
/// `1.8.0_392` → `[8, 0, 392]`, `17.0.9` → `[17, 0, 9]`, `21` → `[21]`.
fn version_components(version: &str) -> Option<Vec<u32>> {
    let mut parts: Vec<u32> = version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    if parts.len() > 1 && parts[0] == 1 {
        parts.remove(0);
    }
    parts
        .first()
        .is_some_and(|&major| major > 0)
        .then_some(parts)
}

/// Major version of a Java version string: `1.8.0_392` → 8, `17.0.9` → 17.
pub fn java_major_version(version: &str) -> Option<u32> {
    version_components(version).map(|v| v[0])
}

/// The version line of `java -version` output and its major version.
fn parse_java_version_output(output: &str) -> Option<(String, u32)> {
    output.lines().find_map(|line| {
        let (_, rest) = line.split_once(" version \"")?;
        let (version, _) = rest.split_once('"')?;
        Some((line.trim().to_string(), java_major_version(version)?))
    })
}

/// The result of resolving and probing the Gradle JDK. Shared by the GUI
/// Health Center and the MCP health tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaCheck {
    /// The resolved JDK, or `None` when `java` on `PATH` was probed.
    pub jdk: Option<ResolvedJdk>,
    /// The binary that was probed.
    pub bin: PathBuf,
    /// `java -version` exited successfully and reported a version.
    pub found: bool,
    pub version_line: Option<String>,
    pub major: Option<u32>,
}

impl JavaCheck {
    /// Found, but older than the Android Gradle Plugin 8 minimum.
    pub fn is_too_old(&self) -> bool {
        self.found && self.major.is_some_and(|m| m < MIN_GRADLE_JDK_MAJOR)
    }

    /// The MCP view of this check.
    pub fn to_json(&self) -> Value {
        let hint = (!self.found).then(|| {
            let fix = match self.jdk.as_ref().map(|j| j.source) {
                Some(JdkSource::UserGradleProperties | JdkSource::ProjectGradleProperties) => {
                    "check org.gradle.java.home in gradle.properties"
                }
                Some(JdkSource::Settings) => "check java.home in Settings → Tools",
                _ => "install Android Studio or JDK 17+, or set java.home in Settings → Tools",
            };
            format!("No working Java at {} — {fix}", self.bin.display())
        });
        let warning = self.is_too_old().then(|| {
            format!(
                "JDK {} is older than {MIN_GRADLE_JDK_MAJOR}; Android Gradle Plugin 8 and newer need JDK {MIN_GRADLE_JDK_MAJOR}+",
                self.major.unwrap_or_default()
            )
        });
        json!({
            "ok": self.found,
            "java_home": self.jdk.as_ref().map(|j| j.home.to_string_lossy().into_owned()),
            "source": self.jdk.as_ref().map(|j| j.source),
            "major_version": self.major,
            "version": self.version_line,
            "bin": self.bin.to_string_lossy(),
            "warning": warning,
            "hint": hint,
        })
    }
}

/// The `java` binary to probe for a resolved JDK, or `java` on `PATH`.
fn java_bin(jdk: Option<&ResolvedJdk>) -> PathBuf {
    jdk.map(|j| j.home.join("bin").join("java"))
        .unwrap_or_else(|| PathBuf::from("java"))
}

/// Resolve the Gradle JDK and run `java -version` on it.
pub async fn check_java(
    settings: &AppSettings,
    gradle_root: Option<&Path>,
    roots: &JdkSearchRoots,
) -> JavaCheck {
    let jdk = resolve_jdk(settings, gradle_root, roots);
    let bin = java_bin(jdk.as_ref());
    let version = probe_java(&bin).await;
    JavaCheck {
        jdk,
        bin,
        found: version.is_some(),
        major: version.as_ref().map(|(_, major)| *major),
        version_line: version.map(|(line, _)| line),
    }
}

/// [`check_java`] for the open project. An untrusted project's
/// `gradle.properties` is ignored, so its `org.gradle.java.home` cannot choose
/// the binary that is run.
pub async fn check_project_java(
    settings: &AppSettings,
    project_root: Option<&Path>,
    gradle_root: Option<&Path>,
    roots: &JdkSearchRoots,
) -> JavaCheck {
    let trusted = project_root
        .or(gradle_root)
        .is_some_and(|root| crate::services::project_trust::is_trusted(settings, root));
    let project_dir = gradle_root.or(project_root).filter(|_| trusted);
    check_java(settings, project_dir, roots).await
}

/// Run `<bin> -version`. Java counts as present only when it exits
/// successfully and prints a version: the macOS `/usr/bin/java` stub exits
/// non-zero with "Unable to locate a Java Runtime" when no JDK is installed.
async fn probe_java(bin: &Path) -> Option<(String, u32)> {
    let out = output_with_timeout(
        tokio::process::Command::new(bin)
            .arg("-version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
        TOOL_PROBE_TIMEOUT,
    )
    .await
    .ok()?;
    if !out.status.success() {
        return None;
    }
    // `java -version` writes to stderr; some builds use stdout.
    parse_java_version_output(&String::from_utf8_lossy(&out.stderr))
        .or_else(|| parse_java_version_output(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    pub const STUB_JAVA: &str =
        "echo \"The operation couldn't be completed. Unable to locate a Java Runtime.\" >&2\nexit 1";

    pub fn real_java(version: &str) -> String {
        format!(
            "echo 'openjdk version \"{version}\" 2025-07-15' >&2\n\
             echo 'OpenJDK Runtime Environment (build {version})' >&2\nexit 0"
        )
    }

    pub fn write_script(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A JDK home at `home` whose `release` file and `bin/java` report `version`.
    pub fn fake_jdk_home(home: &Path, version: &str) -> PathBuf {
        std::fs::create_dir_all(home).unwrap();
        std::fs::write(
            home.join("release"),
            format!("IMPLEMENTOR=\"Test\"\nJAVA_VERSION=\"{version}\"\n"),
        )
        .unwrap();
        write_script(&home.join("bin").join("java"), &real_java(version));
        home.to_path_buf()
    }

    /// An installed JDK bundle `<jvm_dir>/<name>/Contents/Home`.
    pub fn fake_installed_jdk(jvm_dir: &Path, name: &str, version: &str) -> PathBuf {
        fake_jdk_home(&jvm_dir.join(name).join("Contents/Home"), version)
    }

    /// Android Studio's bundled runtime under `<apps>/<app>`.
    pub fn fake_jbr(apps: &Path, app: &str, version: &str) -> PathBuf {
        fake_jdk_home(&apps.join(app).join("Contents/jbr/Contents/Home"), version)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        roots: JdkSearchRoots,
        project: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let roots = JdkSearchRoots {
                gradle_user_home: Some(root.join("gradle-home")),
                application_dirs: vec![root.join("Applications")],
                jvm_dir: Some(root.join("JavaVirtualMachines")),
            };
            let project = root.join("project");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::create_dir_all(roots.gradle_user_home.as_ref().unwrap()).unwrap();
            Self {
                _dir: dir,
                root,
                roots,
                project,
            }
        }

        fn installed(&self, name: &str, version: &str) -> PathBuf {
            fake_installed_jdk(self.roots.jvm_dir.as_ref().unwrap(), name, version)
        }

        fn jbr(&self, app: &str, version: &str) -> PathBuf {
            fake_jbr(&self.roots.application_dirs[0], app, version)
        }

        fn resolve(&self, settings: &AppSettings) -> Option<ResolvedJdk> {
            resolve_jdk(settings, Some(&self.project), &self.roots)
        }
    }

    fn settings_with_home(home: Option<&Path>) -> AppSettings {
        let mut s = AppSettings::default();
        s.java.home = home.map(|h| h.to_string_lossy().into_owned());
        s
    }

    fn gradle_properties(dir: &Path, java_home: &Path) {
        std::fs::write(
            dir.join("gradle.properties"),
            format!(
                "org.gradle.jvmargs=-Xmx2g\norg.gradle.java.home={}\n",
                java_home.display()
            ),
        )
        .unwrap();
    }

    fn resolved(home: &Path, source: JdkSource) -> Option<ResolvedJdk> {
        Some(ResolvedJdk {
            home: home.to_path_buf(),
            source,
        })
    }

    // ── Version parsing ──────────────────────────────────────────────────────

    #[test]
    fn major_version_handles_legacy_and_modern_formats() {
        assert_eq!(java_major_version("1.8.0_392"), Some(8));
        assert_eq!(java_major_version("11.0.21"), Some(11));
        assert_eq!(java_major_version("17.0.9"), Some(17));
        assert_eq!(java_major_version("21"), Some(21));
        assert_eq!(java_major_version("21-ea"), Some(21));
        assert_eq!(java_major_version("17.0.9+9"), Some(17));
        assert_eq!(java_major_version(""), None);
        assert_eq!(java_major_version("abc"), None);
    }

    #[test]
    fn java_version_output_finds_the_version_line() {
        let out = "Picked up JAVA_TOOL_OPTIONS: -Xmx1g\n\
                   openjdk version \"21.0.8\" 2025-07-15\n\
                   OpenJDK Runtime Environment (build 21.0.8+9)\n";
        assert_eq!(
            parse_java_version_output(out),
            Some(("openjdk version \"21.0.8\" 2025-07-15".into(), 21))
        );
        assert_eq!(
            parse_java_version_output("java version \"1.8.0_392\"\n"),
            Some(("java version \"1.8.0_392\"".into(), 8))
        );
        assert_eq!(
            parse_java_version_output(
                "The operation couldn't be completed. Unable to locate a Java Runtime."
            ),
            None
        );
    }

    #[test]
    fn property_value_follows_java_properties_rules() {
        let props = "# org.gradle.java.home=/commented\n\
                     ! org.gradle.java.home=/also-commented\n\
                     org.gradle.java.home.extra=/other\n\
                     org.gradle.java.home = /first\n\
                     org.gradle.java.home:/Library/My\\ JDK\n";
        assert_eq!(
            property_value(props, GRADLE_JAVA_HOME_PROPERTY).as_deref(),
            Some("/Library/My JDK")
        );
        assert_eq!(
            property_value("org.gradle.java.home=\n", GRADLE_JAVA_HOME_PROPERTY),
            None
        );
        assert_eq!(
            property_value("org.gradle.jvmargs=-Xmx2g\n", GRADLE_JAVA_HOME_PROPERTY),
            None
        );
    }

    // ── Detection order ──────────────────────────────────────────────────────

    #[test]
    fn nothing_found_resolves_to_none() {
        let f = Fixture::new();
        assert_eq!(f.resolve(&settings_with_home(None)), None);
    }

    #[test]
    fn installed_jdk_picks_the_highest_17_or_newer() {
        let f = Fixture::new();
        f.installed("jdk-1.8.jdk", "1.8.0_392");
        f.installed("jdk-11.jdk", "11.0.21");
        f.installed("jdk-17.jdk", "17.0.9");
        let jdk21 = f.installed("jdk-21.jdk", "21");
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&jdk21, JdkSource::InstalledJdk)
        );
    }

    #[test]
    fn installed_jdk_compares_full_versions_within_a_major() {
        let f = Fixture::new();
        let newer = f.installed("a-jdk-21.jdk", "21.0.8");
        f.installed("z-jdk-21.jdk", "21.0.2");
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&newer, JdkSource::InstalledJdk)
        );
    }

    #[test]
    fn installed_jdks_older_than_17_are_never_chosen() {
        let f = Fixture::new();
        f.installed("jdk-1.8.jdk", "1.8.0_392");
        f.installed("jdk-11.jdk", "11.0.21");
        assert_eq!(f.resolve(&settings_with_home(None)), None);
    }

    #[test]
    fn android_studio_runtime_beats_an_installed_jdk_11() {
        let f = Fixture::new();
        f.installed("jdk-11.jdk", "11.0.21");
        let jbr = f.jbr("Android Studio.app", "21.0.8");
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&jbr, JdkSource::AndroidStudio)
        );
    }

    #[test]
    fn android_studio_preview_runtime_is_used_when_stable_is_missing() {
        let f = Fixture::new();
        let jbr = f.jbr("Android Studio Preview.app", "21.0.8");
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&jbr, JdkSource::AndroidStudio)
        );
    }

    #[test]
    fn an_old_android_studio_runtime_is_skipped() {
        let f = Fixture::new();
        f.jbr("Android Studio.app", "11.0.15");
        let jdk17 = f.installed("jdk-17.jdk", "17.0.9");
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&jdk17, JdkSource::InstalledJdk)
        );
    }

    #[test]
    fn the_setting_beats_discovered_jdks() {
        let f = Fixture::new();
        f.jbr("Android Studio.app", "21.0.8");
        let configured = fake_jdk_home(&f.root.join("configured"), "17.0.9");
        assert_eq!(
            f.resolve(&settings_with_home(Some(&configured))),
            resolved(&configured, JdkSource::Settings)
        );
    }

    #[test]
    fn a_blank_setting_is_ignored() {
        let f = Fixture::new();
        let jbr = f.jbr("Android Studio.app", "21.0.8");
        let mut settings = AppSettings::default();
        settings.java.home = Some("  ".into());
        assert_eq!(
            f.resolve(&settings),
            resolved(&jbr, JdkSource::AndroidStudio)
        );
    }

    #[test]
    fn project_gradle_properties_beat_the_setting() {
        let f = Fixture::new();
        let configured = fake_jdk_home(&f.root.join("configured"), "17.0.9");
        let project_jdk = fake_jdk_home(&f.root.join("project-jdk"), "21.0.8");
        gradle_properties(&f.project, &project_jdk);
        assert_eq!(
            f.resolve(&settings_with_home(Some(&configured))),
            resolved(&project_jdk, JdkSource::ProjectGradleProperties)
        );
    }

    #[test]
    fn user_gradle_properties_beat_project_gradle_properties() {
        let f = Fixture::new();
        let project_jdk = fake_jdk_home(&f.root.join("project-jdk"), "21.0.8");
        let user_jdk = fake_jdk_home(&f.root.join("user-jdk"), "17.0.9");
        gradle_properties(&f.project, &project_jdk);
        gradle_properties(f.roots.gradle_user_home.as_ref().unwrap(), &user_jdk);
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&user_jdk, JdkSource::UserGradleProperties)
        );
    }

    #[test]
    fn gradle_properties_without_java_home_fall_through() {
        let f = Fixture::new();
        std::fs::write(
            f.project.join("gradle.properties"),
            "org.gradle.jvmargs=-Xmx2g\n",
        )
        .unwrap();
        let jbr = f.jbr("Android Studio.app", "21.0.8");
        assert_eq!(
            f.resolve(&settings_with_home(None)),
            resolved(&jbr, JdkSource::AndroidStudio)
        );
    }

    #[test]
    fn nothing_resolved_probes_java_on_path() {
        assert_eq!(java_bin(None), PathBuf::from("java"));
        let jdk = ResolvedJdk {
            home: PathBuf::from("/jdk"),
            source: JdkSource::Settings,
        };
        assert_eq!(java_bin(Some(&jdk)), PathBuf::from("/jdk/bin/java"));
    }

    // ── Probe ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_macos_java_stub_is_reported_missing() {
        let f = Fixture::new();
        let stub = f.root.join("usr/bin/java");
        write_script(&stub, STUB_JAVA);
        assert_eq!(probe_java(&stub).await, None);
    }

    #[tokio::test]
    async fn a_working_java_reports_its_version() {
        let f = Fixture::new();
        let bin = f.root.join("bin/java");
        write_script(&bin, &real_java("21.0.8"));
        assert_eq!(
            probe_java(&bin).await,
            Some(("openjdk version \"21.0.8\" 2025-07-15".into(), 21))
        );
    }

    #[tokio::test]
    async fn a_successful_exit_without_a_version_is_missing() {
        let f = Fixture::new();
        let bin = f.root.join("bin/java");
        write_script(&bin, "echo hello >&2\nexit 0");
        assert_eq!(probe_java(&bin).await, None);
    }

    #[tokio::test]
    async fn a_missing_binary_is_missing() {
        assert_eq!(probe_java(Path::new("/nonexistent/bin/java")).await, None);
    }

    #[tokio::test]
    async fn check_java_probes_the_resolved_jdk() {
        let f = Fixture::new();
        f.installed("jdk-11.jdk", "11.0.21");
        let jbr = f.jbr("Android Studio.app", "21.0.8");
        let check = check_java(&settings_with_home(None), Some(&f.project), &f.roots).await;
        assert_eq!(check.jdk, resolved(&jbr, JdkSource::AndroidStudio));
        assert_eq!(check.bin, jbr.join("bin/java"));
        assert!(check.found);
        assert_eq!(check.major, Some(21));
        assert!(!check.is_too_old());
    }

    #[tokio::test]
    async fn check_java_flags_a_configured_jdk_older_than_17() {
        let f = Fixture::new();
        let jdk11 = fake_jdk_home(&f.root.join("jdk11"), "11.0.21");
        let check = check_java(&settings_with_home(Some(&jdk11)), None, &f.roots).await;
        assert!(check.found);
        assert_eq!(check.major, Some(11));
        assert!(check.is_too_old());
        let json = check.to_json();
        assert_eq!(json["ok"], true);
        assert!(json["warning"].as_str().unwrap().contains("JDK 11"));
    }

    #[tokio::test]
    async fn check_java_reports_a_broken_configured_jdk() {
        let f = Fixture::new();
        let broken = f.root.join("broken-jdk");
        write_script(&broken.join("bin/java"), STUB_JAVA);
        let check = check_java(&settings_with_home(Some(&broken)), None, &f.roots).await;
        assert_eq!(check.jdk, resolved(&broken, JdkSource::Settings));
        assert!(!check.found);
        assert_eq!(check.major, None);
        let json = check.to_json();
        assert_eq!(json["ok"], false);
        assert_eq!(json["source"], "settings");
        assert!(json["hint"].is_string());
    }
}
