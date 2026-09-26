//! Which saved R8 mapping belongs to a crash, from what the trace and the
//! device say: the map id R8 writes into source file names, and the SHA-256
//! of the installed APK.
//!
//! Mappings are looked up in the kept build history and the saved installs.
//! Both are small and capped (`build_runner::MAX_HISTORY` records of at most
//! `MAX_MAPPINGS_PER_BUILD` mappings, `MAX_INSTALLED_TARGETS` installs), so
//! they are scanned on each lookup instead of kept in a separate index that
//! would have to follow retention. `retrace` asks the device and runs the tool.

use crate::models::build::{BuiltApk, InstalledBuild, MappingSnapshot};
use crate::services::build_runner;
use crate::services::installed_builds;
use crate::services::mapping_snapshots;
use std::path::Path;

/// The source file name R8 gives the classes it compiles: `r8-map-id-` and
/// the mapping's `pg_map_id`.
const MAP_ID_SOURCE_PREFIX: &str = "r8-map-id-";

/// Shortest map id matched as a prefix of a saved one: the length R8 used
/// for `pg_map_id` before it switched to the full SHA-256.
pub const MIN_MAP_ID_PREFIX_LEN: usize = 7;

/// Most distinct map ids read from one trace; two already refuse it.
const MAX_TRACE_MAP_IDS: usize = 8;

/// Most characters of a map id read from a frame. R8's default id is a
/// 64-character SHA-256, and saved ids are at most 64 characters.
const MAX_MAP_ID_CHARS: usize = 128;

/// Longest APK path accepted from `pm path`.
const MAX_DEVICE_PATH_LEN: usize = 4096;

// ── Map ids ───────────────────────────────────────────────────────────────────

/// The distinct map ids the trace's frames name, in order: the `<id>` of
/// `at a.b(r8-map-id-<id>:12)` or `at a.b(r8-map-id-<id>)`. Other lines,
/// exception messages included, are not read.
pub fn trace_map_ids(trace: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for line in trace.lines() {
        let Some(frame) = line.trim_start().strip_prefix("at ") else {
            continue;
        };
        let Some(open) = frame.rfind('(') else {
            continue;
        };
        let Some(rest) = frame[open + 1..].strip_prefix(MAP_ID_SOURCE_PREFIX) else {
            continue;
        };
        let id: String = rest
            .chars()
            .take_while(|c| *c != ':' && *c != ')')
            .take(MAX_MAP_ID_CHARS)
            .collect();
        if !ids.contains(&id) {
            if ids.len() >= MAX_TRACE_MAP_IDS {
                break;
            }
            ids.push(id);
        }
    }
    ids
}

/// A saved mapping and the build that wrote it, when that is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedMapping {
    pub mapping: MappingSnapshot,
    pub build_id: Option<u32>,
}

/// How [`find_by_map_id`] matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapIdMatch {
    /// The saved `pg_map_id` is the frame's id.
    Exact(SavedMapping),
    /// The frame's id is the start of exactly one saved mapping's id.
    Prefix(SavedMapping),
}

/// Every mapping the kept history and the saved installs name, once per
/// file: the newest history record naming a file first, then the newest
/// install.
fn saved_mappings(dir: &Path) -> Vec<SavedMapping> {
    let history = build_runner::load_build_history_from(dir);
    let installs = installed_builds::load_installed_builds_from(dir);
    let from_history = history.iter().rev().flat_map(|record| {
        record.mappings.iter().map(|m| SavedMapping {
            mapping: m.clone(),
            build_id: Some(record.id),
        })
    });
    let from_installs = installs.iter().rev().flat_map(|install| {
        install.mappings.iter().map(|m| SavedMapping {
            mapping: m.clone(),
            build_id: install.build_id,
        })
    });
    let mut saved: Vec<SavedMapping> = Vec::new();
    for candidate in from_history.chain(from_installs) {
        if !saved
            .iter()
            .any(|s| s.mapping.sha256 == candidate.mapping.sha256)
        {
            saved.push(candidate);
        }
    }
    saved
}

/// The saved mapping whose `pg_map_id` is `id`. When none is, a frame id of
/// at least [`MIN_MAP_ID_PREFIX_LEN`] characters matches the one saved id it
/// starts; several are refused. Never another mapping.
pub fn find_by_map_id(dir: &Path, id: &str) -> Result<MapIdMatch, String> {
    let saved = saved_mappings(dir);
    let saved_id = |s: &&SavedMapping| s.mapping.pg_map_id.clone().unwrap_or_default();
    let exact: Vec<&SavedMapping> = saved.iter().filter(|s| saved_id(s) == id).collect();
    let prefix: Vec<&SavedMapping> = if exact.is_empty() && id.len() >= MIN_MAP_ID_PREFIX_LEN {
        saved
            .iter()
            .filter(|s| {
                let saved = saved_id(s);
                saved.len() > id.len() && saved.starts_with(id)
            })
            .collect()
    } else {
        Vec::new()
    };
    let found = match (exact.as_slice(), prefix.as_slice()) {
        ([only], _) => MapIdMatch::Exact((*only).clone()),
        ([], [only]) => MapIdMatch::Prefix((*only).clone()),
        ([], []) => {
            return Err(format!(
                "no saved mapping for map id {id}: Keynobi keeps the R8 mappings of the builds \
                 in its history and of the builds it installed, and none has this id"
            ))
        }
        ([], several) => {
            return Err(format!(
                "the map id {id} in the trace is the start of {} saved mappings' ids, so it does \
                 not pick one",
                several.len()
            ))
        }
        (several, _) => {
            return Err(format!(
                "{} different saved mappings have map id {id}, so it does not pick one",
                several.len()
            ))
        }
    };
    let saved = match &found {
        MapIdMatch::Exact(saved) | MapIdMatch::Prefix(saved) => saved,
    };
    let present = mapping_snapshots::snapshot_path(dir, &saved.mapping.sha256)
        .is_some_and(|path| path.is_file());
    if !present {
        return Err(format!(
            "the saved R8 mapping with map id {id} is missing from Keynobi's data directory"
        ));
    }
    Ok(found)
}

// ── The installed APK ─────────────────────────────────────────────────────────

/// The APK paths `pm path <package>` lists (`package:/data/app/…/base.apk`),
/// one per APK: one for a single APK, several for split APKs.
pub fn parse_pm_path(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("package:"))
        .map(|path| path.trim().to_string())
        .collect()
}

/// Whether `path` from `pm path` looks like an installed APK: absolute, an
/// `.apk`, bounded, and without control characters. It is quoted for the
/// device shell whatever it holds.
pub fn is_device_apk_path(path: &str) -> bool {
    path.starts_with('/')
        && path.ends_with(".apk")
        && path.len() <= MAX_DEVICE_PATH_LEN
        && !path.chars().any(char::is_control)
}

/// The SHA-256 in `sha256sum` output (`<hash>  <path>`), lowercase.
pub fn parse_sha256sum(text: &str) -> Option<String> {
    let hash = text.split_whitespace().next()?;
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

/// Where an APK with a given SHA-256 came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApkOwner {
    /// The install Keynobi recorded for this device and package.
    Recorded(InstalledBuild),
    /// A build in the kept history wrote it; its saved mappings of the APK's
    /// module and variant.
    Built {
        build_id: u32,
        apk: BuiltApk,
        mappings: Vec<MappingSnapshot>,
    },
    /// An install Keynobi recorded on another device or for another package.
    OtherInstall(InstalledBuild),
}

/// The APK whose SHA-256 is `sha256`: the device's recorded install first,
/// then the newest kept build that wrote it (which covers an APK Keynobi built
/// and another tool installed), then any other recorded install.
pub fn find_apk_owner(
    dir: &Path,
    sha256: &str,
    recorded: Option<&InstalledBuild>,
) -> Option<ApkOwner> {
    if let Some(recorded) = recorded.filter(|r| r.apk_sha256 == sha256) {
        return Some(ApkOwner::Recorded(recorded.clone()));
    }
    let history = build_runner::load_build_history_from(dir);
    let built = history.iter().rev().find_map(|record| {
        let apk = record.apks.iter().find(|apk| apk.sha256 == sha256)?;
        Some(ApkOwner::Built {
            build_id: record.id,
            apk: apk.clone(),
            mappings: installed_builds::mappings_of(dir, &record.mappings, apk),
        })
    });
    built.or_else(|| {
        installed_builds::load_installed_builds_from(dir)
            .into_iter()
            .rev()
            .find(|install| install.apk_sha256 == sha256)
            .map(ApkOwner::OtherInstall)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::build::{BuildRecord, BuildStatus};
    use tempfile::TempDir;

    const FULL_ID: &str = "6b1c2f0a9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b";

    fn snapshot(n: u8, pg_map_id: Option<&str>) -> MappingSnapshot {
        MappingSnapshot {
            module: ":app".into(),
            variant: "release".into(),
            sha256: format!("{n:064x}"),
            bytes: 1,
            pg_map_id: pg_map_id.map(str::to_string),
        }
    }

    fn apk(sha256: &str, variant: &str) -> BuiltApk {
        BuiltApk {
            module: ":app".into(),
            variant: variant.into(),
            application_id: Some("com.example.app".into()),
            version_code: Some(42),
            sha256: sha256.into(),
            bytes: 1,
            path: format!("app/build/outputs/apk/{variant}/app-{variant}.apk"),
        }
    }

    fn record(id: u32, mappings: Vec<MappingSnapshot>, apks: Vec<BuiltApk>) -> BuildRecord {
        BuildRecord {
            id,
            task: "assembleRelease".into(),
            status: BuildStatus::Idle,
            errors: vec![],
            started_at: String::new(),
            project_root: None,
            origin: None,
            cancelled_by: None,
            launch: None,
            mappings,
            apks,
        }
    }

    fn install(serial: &str, sha256: &str, mappings: Vec<MappingSnapshot>) -> InstalledBuild {
        InstalledBuild {
            serial: serial.into(),
            avd_name: None,
            model: None,
            package: "com.example.app".into(),
            apk_sha256: sha256.into(),
            build_id: Some(3),
            version_code: Some(42),
            mappings,
            installed_at: "2026-09-25T10:32:00Z".into(),
        }
    }

    /// Save `records` (oldest first) and `installs`, and every snapshot file
    /// they name.
    fn save(dir: &Path, records: &[BuildRecord], installs: &[InstalledBuild]) {
        let newest_first: Vec<&BuildRecord> = records.iter().rev().collect();
        std::fs::write(
            dir.join("build-history.json"),
            serde_json::to_string(&newest_first).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("installed-builds.json"),
            serde_json::to_string(installs).unwrap(),
        )
        .unwrap();
        std::fs::create_dir_all(mapping_snapshots::mappings_dir(dir)).unwrap();
        let named = records
            .iter()
            .flat_map(|r| &r.mappings)
            .chain(installs.iter().flat_map(|i| &i.mappings));
        for m in named {
            std::fs::write(
                mapping_snapshots::snapshot_path(dir, &m.sha256).unwrap(),
                "x",
            )
            .unwrap();
        }
    }

    // ── Map ids in the trace ─────────────────────────────────────────────────

    #[test]
    fn map_ids_are_read_from_frames_only() {
        let trace = format!(
            "java.lang.IllegalStateException: see (r8-map-id-fromthemessage:1)\n\
             \tat a.a.b(r8-map-id-{FULL_ID}:12)\n\
             \tat a.a.c(r8-map-id-{FULL_ID})\n\
             \tat android.app.Activity.performCreate(Activity.java:8595)\n\
             Caused by: java.lang.NullPointerException\n\
             \tat b.c.d(r8-map-id-{FULL_ID}:3)\n\
             \t... 5 more\n"
        );
        assert_eq!(trace_map_ids(&trace), vec![FULL_ID.to_string()]);
        assert!(trace_map_ids("\tat a.a.b(SourceFile:1)\n\tat c.d(Unknown Source)\n").is_empty());
    }

    #[test]
    fn different_map_ids_are_all_reported() {
        let trace = "\tat a.a.b(r8-map-id-aaaaaaa:1)\n\tat c.d(r8-map-id-bbbbbbb:2)\n\
                     \tat e.f(r8-map-id-aaaaaaa:3)\n";
        assert_eq!(trace_map_ids(trace), vec!["aaaaaaa", "bbbbbbb"]);
    }

    // ── Finding the mapping by map id ────────────────────────────────────────

    #[test]
    fn a_map_id_matches_the_saved_mapping_with_that_id() {
        let dir = TempDir::new().unwrap();
        let wanted = snapshot(1, Some(FULL_ID));
        save(
            dir.path(),
            &[
                record(11, vec![snapshot(2, Some("0123456789"))], vec![]),
                record(12, vec![wanted.clone()], vec![]),
            ],
            &[],
        );

        assert_eq!(
            find_by_map_id(dir.path(), FULL_ID),
            Ok(MapIdMatch::Exact(SavedMapping {
                mapping: wanted,
                build_id: Some(12),
            }))
        );
    }

    #[test]
    fn a_map_id_matches_a_mapping_only_an_install_keeps() {
        let dir = TempDir::new().unwrap();
        let wanted = snapshot(1, Some(FULL_ID));
        save(
            dir.path(),
            &[],
            &[install("R5CT", &"a1".repeat(32), vec![wanted.clone()])],
        );

        assert_eq!(
            find_by_map_id(dir.path(), FULL_ID),
            Ok(MapIdMatch::Exact(SavedMapping {
                mapping: wanted,
                build_id: Some(3),
            }))
        );
    }

    #[test]
    fn an_unknown_map_id_is_refused() {
        let dir = TempDir::new().unwrap();
        save(
            dir.path(),
            &[record(12, vec![snapshot(1, Some(FULL_ID))], vec![])],
            &[],
        );

        let err = find_by_map_id(dir.path(), "ffffffffffff").unwrap_err();
        assert!(
            err.starts_with("no saved mapping for map id ffffffffffff"),
            "{err}"
        );
    }

    #[test]
    fn a_missing_snapshot_file_is_refused() {
        let dir = TempDir::new().unwrap();
        let wanted = snapshot(1, Some(FULL_ID));
        save(dir.path(), &[record(12, vec![wanted.clone()], vec![])], &[]);
        std::fs::remove_file(mapping_snapshots::snapshot_path(dir.path(), &wanted.sha256).unwrap())
            .unwrap();

        let err = find_by_map_id(dir.path(), FULL_ID).unwrap_err();
        assert!(
            err.contains("missing from Keynobi's data directory"),
            "{err}"
        );
    }

    #[test]
    fn a_unique_prefix_of_a_saved_id_matches() {
        let dir = TempDir::new().unwrap();
        let wanted = snapshot(1, Some(FULL_ID));
        save(
            dir.path(),
            &[record(
                12,
                vec![wanted.clone(), snapshot(2, Some("9f8e7d6c5b"))],
                vec![],
            )],
            &[],
        );

        assert_eq!(
            find_by_map_id(dir.path(), &FULL_ID[..MIN_MAP_ID_PREFIX_LEN]),
            Ok(MapIdMatch::Prefix(SavedMapping {
                mapping: wanted,
                build_id: Some(12),
            }))
        );
    }

    #[test]
    fn a_short_or_shared_prefix_is_refused() {
        let dir = TempDir::new().unwrap();
        let other = format!("{}ffff", &FULL_ID[..10]);
        save(
            dir.path(),
            &[record(
                12,
                vec![snapshot(1, Some(FULL_ID)), snapshot(2, Some(&other))],
                vec![],
            )],
            &[],
        );

        // Shorter than the minimum, though only one id starts with it.
        let err = find_by_map_id(dir.path(), &FULL_ID[..6]).unwrap_err();
        assert!(err.starts_with("no saved mapping for map id"), "{err}");
        // Long enough, but two ids start with it.
        let err = find_by_map_id(dir.path(), &FULL_ID[..8]).unwrap_err();
        assert!(
            err.contains("is the start of 2 saved mappings' ids"),
            "{err}"
        );
        // A frame id longer than the saved one is not a prefix match.
        let err = find_by_map_id(dir.path(), &format!("{FULL_ID}0")).unwrap_err();
        assert!(err.starts_with("no saved mapping for map id"), "{err}");
    }

    #[test]
    fn two_files_with_one_map_id_are_refused_and_one_file_twice_is_not() {
        let dir = TempDir::new().unwrap();
        let first = snapshot(1, Some(FULL_ID));
        save(
            dir.path(),
            &[
                record(11, vec![first.clone()], vec![]),
                record(12, vec![first.clone()], vec![]),
            ],
            &[install("R5CT", &"a1".repeat(32), vec![first.clone()])],
        );
        // The same file in two records and an install: the newest record.
        assert_eq!(
            find_by_map_id(dir.path(), FULL_ID),
            Ok(MapIdMatch::Exact(SavedMapping {
                mapping: first.clone(),
                build_id: Some(12),
            }))
        );

        save(
            dir.path(),
            &[
                record(11, vec![first], vec![]),
                record(12, vec![snapshot(2, Some(FULL_ID))], vec![]),
            ],
            &[],
        );
        let err = find_by_map_id(dir.path(), FULL_ID).unwrap_err();
        assert!(err.contains("2 different saved mappings"), "{err}");
    }

    // ── The installed APK ────────────────────────────────────────────────────

    #[test]
    fn pm_path_lists_one_path_per_apk() {
        assert_eq!(
            parse_pm_path("package:/data/app/~~a==/com.example.app-b==/base.apk\n"),
            vec!["/data/app/~~a==/com.example.app-b==/base.apk"]
        );
        assert_eq!(
            parse_pm_path(
                "package:/data/app/x/base.apk\npackage:/data/app/x/split_config.arm64_v8a.apk\n\
                 package:/data/app/x/split_config.xxhdpi.apk\n"
            )
            .len(),
            3
        );
        assert!(parse_pm_path("").is_empty());
        assert!(is_device_apk_path("/data/app/x/base.apk"));
        assert!(!is_device_apk_path("data/app/x/base.apk"));
        assert!(!is_device_apk_path("/data/app/x/base.apk\u{1b}[2J"));
        assert!(!is_device_apk_path("/data/app/x/base.odex"));
    }

    #[test]
    fn sha256sum_output_is_read_as_one_hash() {
        let hash = "A1".repeat(32);
        assert_eq!(
            parse_sha256sum(&format!("{hash}  /data/app/x/base.apk\n")),
            Some("a1".repeat(32))
        );
        assert_eq!(
            parse_sha256sum("/system/bin/sh: sha256sum: inaccessible or not found\n"),
            None
        );
        assert_eq!(parse_sha256sum(""), None);
        assert_eq!(parse_sha256sum(&"g".repeat(64)), None);
    }

    #[test]
    fn an_apk_is_owned_by_the_recorded_install_then_a_build_then_another_install() {
        let dir = TempDir::new().unwrap();
        let sha = "b2".repeat(32);
        let release = snapshot(1, Some(FULL_ID));
        let staging = MappingSnapshot {
            variant: "staging".into(),
            ..snapshot(2, None)
        };
        let recorded = install("R5CT", &sha, vec![]);
        let elsewhere = install("ZX1G", &sha, vec![]);
        save(
            dir.path(),
            &[
                record(11, vec![], vec![apk(&sha, "release")]),
                record(
                    12,
                    vec![release.clone(), staging],
                    vec![apk(&sha, "release")],
                ),
            ],
            std::slice::from_ref(&elsewhere),
        );

        assert_eq!(
            find_apk_owner(dir.path(), &sha, Some(&recorded)),
            Some(ApkOwner::Recorded(recorded.clone()))
        );
        // The device's record names another APK: the newest build that wrote
        // this one, with its mapping of the APK's variant only.
        let stale = install("R5CT", &"c3".repeat(32), vec![]);
        assert_eq!(
            find_apk_owner(dir.path(), &sha, Some(&stale)),
            Some(ApkOwner::Built {
                build_id: 12,
                apk: apk(&sha, "release"),
                mappings: vec![release],
            })
        );

        save(dir.path(), &[], std::slice::from_ref(&elsewhere));
        assert_eq!(
            find_apk_owner(dir.path(), &sha, None),
            Some(ApkOwner::OtherInstall(elsewhere))
        );
        assert_eq!(find_apk_owner(dir.path(), &"d4".repeat(32), None), None);
    }
}
