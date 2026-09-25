//! Whether the app runs from a path that will not last, such as a mounted
//! disk image or a macOS App Translocation copy. MCP clients start the
//! registered binary path later, so such a path must not be registered.
use std::path::{Component, Path};

const MOVE_HINT: &str = "Move Keynobi to the Applications folder and open it from there, \
                         then set up your AI client.";

/// Why `exe` is a temporary location, phrased for the user, or `None` when
/// the path is expected to last.
pub fn temporary_location_reason(exe: &Path) -> Option<String> {
    let names: Vec<&str> = exe
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();

    if names.contains(&"AppTranslocation") {
        return Some(format!(
            "macOS is running Keynobi from a temporary copy (App Translocation) whose path \
             changes on every launch. {MOVE_HINT}"
        ));
    }
    if exe.has_root() && names.first() == Some(&"Volumes") && names.len() > 2 {
        return Some(format!(
            "Keynobi is running from a disk image or removable volume (/Volumes/{}), \
             whose path stops working once it is ejected. {MOVE_HINT}",
            names[1]
        ));
    }
    None
}

/// [`temporary_location_reason`] for the running binary.
pub fn current_exe_temporary_reason() -> Option<String> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| temporary_location_reason(&exe))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(path: &str) -> Option<String> {
        temporary_location_reason(Path::new(path))
    }

    #[test]
    fn a_mounted_disk_image_is_temporary() {
        let why = reason("/Volumes/Keynobi 0.5.0/Keynobi.app/Contents/MacOS/keynobi")
            .expect("a DMG path is temporary");
        assert!(why.contains("/Volumes/Keynobi 0.5.0"), "{why}");
        assert!(why.contains("Applications folder"), "{why}");
    }

    #[test]
    fn app_translocation_is_temporary() {
        for path in [
            "/private/var/folders/x1/abc/T/AppTranslocation/0F2A-11/d/Keynobi.app/Contents/MacOS/keynobi",
            "/var/folders/x1/abc/T/AppTranslocation/0F2A-11/d/Keynobi.app/Contents/MacOS/keynobi",
        ] {
            let why = reason(path).expect("a translocated path is temporary");
            assert!(why.contains("App Translocation"), "{why}");
        }
    }

    #[test]
    fn installed_locations_are_kept() {
        for path in [
            "/Applications/Keynobi.app/Contents/MacOS/keynobi",
            "/Users/dev/Applications/Keynobi.app/Contents/MacOS/keynobi",
            "/Applications/Tools/Keynobi.app/Contents/MacOS/keynobi",
        ] {
            assert_eq!(reason(path), None, "{path}");
        }
    }

    #[test]
    fn development_builds_are_kept() {
        for path in [
            "/Users/dev/src/keynobi/src-tauri/target/debug/keynobi",
            "/Users/dev/src/keynobi/src-tauri/target/release/bundle/macos/Keynobi.app/Contents/MacOS/keynobi",
            "/Users/dev/Volumes/keynobi",
            "/Volumes",
            "keynobi",
        ] {
            assert_eq!(reason(path), None, "{path}");
        }
    }
}
