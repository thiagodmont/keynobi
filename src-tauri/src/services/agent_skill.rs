//! The Keynobi agent skill: a `SKILL.md` that tells an AI agent when to use
//! Keynobi and when to use Android CLI.
//!
//! It is built into the binary, served as the MCP resource
//! [`RESOURCE_URI`], and installed for Claude Code only when the user asks
//! (`~/.claude/skills/keynobi/SKILL.md`).

use crate::models::error::AppError;
use crate::services::settings_manager::unique_tmp_path;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// The skill's `SKILL.md`, as shipped.
pub const SKILL_MARKDOWN: &str = include_str!("../../../skills/keynobi/SKILL.md");
/// The MCP resource that serves [`SKILL_MARKDOWN`].
pub const RESOURCE_URI: &str = "keynobi://skill";
/// The skill's name, which is also its folder name.
pub const SKILL_NAME: &str = "keynobi";
/// Largest existing `SKILL.md` read to compare with ours; a bigger one is
/// reported as different.
const MAX_EXISTING_BYTES: u64 = 1024 * 1024;

/// Whether the skill is installed for Claude Code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum AgentSkillState {
    /// Nothing at the install path.
    NotInstalled,
    /// The install path holds this version of the skill.
    Installed,
    /// The install path holds something else: an older version, or the
    /// user's own file. Replacing it needs the user's confirmation.
    Different,
}

/// The Keynobi agent skill and its Claude Code installation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct AgentSkillStatus {
    /// Where the skill is installed for Claude Code (user scope).
    pub path: String,
    pub state: AgentSkillState,
    /// The `SKILL.md` that installing writes.
    pub content: String,
    /// The MCP resource that serves the same file to any client.
    pub resource_uri: String,
}

/// `<home>/.claude/skills`, where Claude Code reads user skills.
pub fn claude_skills_dir(home: &Path) -> PathBuf {
    home.join(".claude").join("skills")
}

/// `<home>/.claude/skills/keynobi/SKILL.md`.
pub fn claude_skill_path(home: &Path) -> PathBuf {
    claude_skills_dir(home).join(SKILL_NAME).join("SKILL.md")
}

/// The skill's install state. Reads only; never creates anything.
pub fn status(home: &Path) -> AgentSkillStatus {
    let path = claude_skill_path(home);
    let state = state_of(&path);
    status_with(&path, state)
}

/// Install the skill for Claude Code. An existing different `SKILL.md` is
/// replaced only when `replace` is true. The file is written to a temporary
/// file next to it and renamed over it, and only inside
/// `<home>/.claude/skills/keynobi` (a `keynobi` folder that resolves
/// elsewhere is refused).
pub fn install(home: &Path, replace: bool) -> Result<AgentSkillStatus, AppError> {
    let skills = claude_skills_dir(home);
    let folder = skills.join(SKILL_NAME);
    std::fs::create_dir_all(&folder).map_err(|e| AppError::io(folder.display(), e))?;
    let folder = crate::utils::path::validate_within_root(&skills, SKILL_NAME)?;
    if !folder.is_dir() {
        return Err(AppError::InvalidInput(format!(
            "{} is not a folder",
            folder.display()
        )));
    }
    let target = folder.join("SKILL.md");

    match state_of(&target) {
        AgentSkillState::Installed => {
            return Ok(status_with(
                &claude_skill_path(home),
                AgentSkillState::Installed,
            ))
        }
        AgentSkillState::Different if !replace => {
            return Err(AppError::InvalidInput(format!(
                "{} already exists and differs from Keynobi's skill; confirm to replace it",
                target.display()
            )))
        }
        _ => {}
    }
    if target
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_dir())
    {
        return Err(AppError::InvalidInput(format!(
            "{} is a folder",
            target.display()
        )));
    }

    write_atomically(&target, SKILL_MARKDOWN.as_bytes())
        .map_err(|e| AppError::io(target.display(), e))?;
    Ok(status_with(
        &claude_skill_path(home),
        AgentSkillState::Installed,
    ))
}

fn status_with(path: &Path, state: AgentSkillState) -> AgentSkillStatus {
    AgentSkillStatus {
        path: path.to_string_lossy().into_owned(),
        state,
        content: SKILL_MARKDOWN.to_string(),
        resource_uri: RESOURCE_URI.to_string(),
    }
}

/// Compare what is at `path` with the shipped skill. A symlink or anything
/// that is not a small regular file counts as different.
fn state_of(path: &Path) -> AgentSkillState {
    let Ok(meta) = path.symlink_metadata() else {
        return AgentSkillState::NotInstalled;
    };
    if !meta.file_type().is_file() || meta.len() > MAX_EXISTING_BYTES {
        return AgentSkillState::Different;
    }
    let mut existing = Vec::new();
    let read = std::fs::File::open(path)
        .and_then(|f| f.take(MAX_EXISTING_BYTES).read_to_end(&mut existing));
    if read.is_ok() && existing == SKILL_MARKDOWN.as_bytes() {
        AgentSkillState::Installed
    } else {
        AgentSkillState::Different
    }
}

/// Write `bytes` to a new temporary file beside `path`, then rename it over
/// `path`. The temporary file is removed when any step fails.
fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = unique_tmp_path(path);
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        (dir, home)
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_skill_names_itself_and_says_when_to_use_it() {
        let front = SKILL_MARKDOWN
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .map(|(front, _)| front)
            .expect("SKILL.md starts with frontmatter");
        assert!(
            front.lines().any(|l| l == format!("name: {SKILL_NAME}")),
            "{front}"
        );
        let description = front
            .lines()
            .find_map(|l| l.strip_prefix("description: "))
            .expect("a description");
        assert!(description.len() <= 1024, "{} chars", description.len());
        assert!(description.contains("Android CLI"), "{description}");
    }

    #[test]
    fn status_only_reads() {
        let (_dir, home) = home();

        let status = status(&home);

        assert_eq!(status.state, AgentSkillState::NotInstalled);
        assert_eq!(
            PathBuf::from(&status.path),
            home.join(".claude/skills/keynobi/SKILL.md")
        );
        assert_eq!(status.content, SKILL_MARKDOWN);
        assert_eq!(status.resource_uri, "keynobi://skill");
        assert!(files_in(&home).is_empty(), "{:?}", files_in(&home));
    }

    #[test]
    fn install_writes_the_skill_and_reports_it_installed() {
        let (_dir, home) = home();

        let installed = install(&home, false).unwrap();

        assert_eq!(installed.state, AgentSkillState::Installed);
        let folder = home.join(".claude/skills/keynobi");
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            SKILL_MARKDOWN
        );
        assert_eq!(files_in(&folder), vec!["SKILL.md"]);
        assert_eq!(status(&home).state, AgentSkillState::Installed);
        // Installing again changes nothing.
        assert_eq!(
            install(&home, false).unwrap().state,
            AgentSkillState::Installed
        );
    }

    #[test]
    fn a_different_skill_is_kept_unless_the_user_confirms() {
        let (_dir, home) = home();
        let folder = home.join(".claude/skills/keynobi");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("SKILL.md"), "my own notes\n").unwrap();
        assert_eq!(status(&home).state, AgentSkillState::Different);

        let refused = install(&home, false).unwrap_err();

        assert!(refused.to_string().contains("confirm"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            "my own notes\n"
        );
        assert_eq!(files_in(&folder), vec!["SKILL.md"]);

        assert_eq!(
            install(&home, true).unwrap().state,
            AgentSkillState::Installed
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            SKILL_MARKDOWN
        );
    }

    #[test]
    fn replacing_renames_a_new_file_over_the_old_one() {
        let (_dir, home) = home();
        let folder = home.join(".claude/skills/keynobi");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("SKILL.md"), "old\n").unwrap();
        // A second name for the old file: a write in place would change it too.
        let old = home.join("old-skill.md");
        std::fs::hard_link(folder.join("SKILL.md"), &old).unwrap();

        install(&home, true).unwrap();

        assert_eq!(std::fs::read_to_string(&old).unwrap(), "old\n");
        assert_eq!(
            std::fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            SKILL_MARKDOWN
        );
        assert_eq!(files_in(&folder), vec!["SKILL.md"]);
    }

    #[test]
    fn a_skill_folder_that_leads_outside_the_skills_folder_is_refused() {
        let (_dir, home) = home();
        let outside = home.join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        let skills = home.join(".claude/skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::os::unix::fs::symlink(&outside, skills.join("keynobi")).unwrap();

        let refused = install(&home, true).unwrap_err();

        assert!(
            matches!(refused, AppError::PermissionDenied(_)),
            "{refused}"
        );
        assert!(files_in(&outside).is_empty(), "{:?}", files_in(&outside));
    }

    #[test]
    fn a_symlinked_skill_file_counts_as_different_and_only_the_link_is_replaced() {
        let (_dir, home) = home();
        let folder = home.join(".claude/skills/keynobi");
        std::fs::create_dir_all(&folder).unwrap();
        let linked = home.join("dotfiles-skill.md");
        std::fs::write(&linked, SKILL_MARKDOWN).unwrap();
        std::os::unix::fs::symlink(&linked, folder.join("SKILL.md")).unwrap();
        assert_eq!(status(&home).state, AgentSkillState::Different);

        install(&home, true).unwrap();

        assert!(folder
            .join("SKILL.md")
            .symlink_metadata()
            .unwrap()
            .is_file());
        assert_eq!(std::fs::read_to_string(&linked).unwrap(), SKILL_MARKDOWN);
    }
}
