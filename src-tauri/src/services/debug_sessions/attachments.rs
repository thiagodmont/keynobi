//! Files attached to a debug session under `<id>/attachments/`, each with an
//! `attachment` event named by its `seq`: `screenshot-<seq>.png` and
//! `hierarchy-<seq>.json`.
//!
//! Both are captured on request from the session's device while the session
//! is open: a screenshot through the screenshot service, fitted to
//! [`ATTACHMENT_SCREENSHOT_MAX_DIMENSION`], within [`MAX_ATTACHMENT_BYTES`];
//! a UI hierarchy through the UI hierarchy service, under its device lock,
//! stored as a [`DebugSessionHierarchy`] within
//! [`MAX_HIERARCHY_ATTACHMENT_BYTES`], [`MAX_HIERARCHY_NODES`], and
//! [`MAX_HIERARCHY_DEPTH`]. Together they stay within
//! [`MAX_ATTACHMENTS_PER_SESSION`] and, with the event log,
//! [`MAX_SESSION_BYTES`] (the session's `bytes`). The folder is pruned with
//! its session.

use super::*;
use crate::models::ui_hierarchy::{UiHierarchySnapshot, UiNode};
use crate::services::adb_manager;
use crate::services::device_inspector::{
    fit_screenshot, take_screenshot_scaled, DEFAULT_SCREENSHOT_MAX_DIMENSION,
};
use crate::services::ui_hierarchy::capture_ui_hierarchy_tree;
use crate::services::ui_hierarchy_parse;
use crate::utils::validation::validate_device_serial;
use base64::Engine as _;

/// Most attachments in one session.
pub const MAX_ATTACHMENTS_PER_SESSION: u32 = 10;
/// Largest attachment.
pub const MAX_ATTACHMENT_BYTES: u64 = 4 * 1024 * 1024;
/// Long edge a screenshot is fitted to before it is attached.
pub const ATTACHMENT_SCREENSHOT_MAX_DIMENSION: u32 = DEFAULT_SCREENSHOT_MAX_DIMENSION;
/// Largest attached UI hierarchy file; the last nodes are left out to fit.
pub const MAX_HIERARCHY_ATTACHMENT_BYTES: u64 = 2 * 1024 * 1024;
/// Most nodes of an attached UI hierarchy, as a capture keeps.
pub const MAX_HIERARCHY_NODES: usize = ui_hierarchy_parse::MAX_NODES;
/// Most levels of an attached UI hierarchy, as a capture keeps: every node's
/// `depth` is below it.
pub const MAX_HIERARCHY_DEPTH: u32 = ui_hierarchy_parse::MAX_DEPTH as u32;
/// Longest text of a hierarchy node, in characters, as a capture keeps.
const MAX_HIERARCHY_TEXT_CHARS: usize = ui_hierarchy_parse::MAX_ATTR_LEN;

pub(super) const ATTACHMENTS_DIR: &str = "attachments";
const PNG_MEDIA_TYPE: &str = "image/png";

/// The file of screenshot event `seq`.
pub(super) fn screenshot_name(seq: u32) -> String {
    format!("screenshot-{seq}.png")
}

/// The file of UI hierarchy event `seq`.
pub(super) fn hierarchy_name(seq: u32) -> String {
    format!("hierarchy-{seq}.json")
}

/// Whether `a`, on event `seq`, is shaped as this version attaches it.
pub(super) fn well_formed(a: &DebugSessionAttachment, seq: u32) -> bool {
    match a.kind {
        DebugSessionAttachmentKind::Screenshot => {
            a.name == screenshot_name(seq)
                && a.width.is_some()
                && a.height.is_some()
                && a.node_count.is_none()
        }
        DebugSessionAttachmentKind::Hierarchy => {
            a.name == hierarchy_name(seq)
                && a.node_count.is_some()
                && a.width.is_none()
                && a.height.is_none()
        }
    }
}

/// A PNG within the attachment caps: its width and height. The header is
/// read, not the pixels.
pub(super) fn checked_png(png: &[u8]) -> Result<(u32, u32), String> {
    if png.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err(format!(
            "it is larger than {} MiB",
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    let geometry = fit_screenshot(png.to_vec(), None)?.geometry;
    Ok((geometry.image_width, geometry.image_height))
}

/// A UI hierarchy file as this version writes it: within
/// [`MAX_HIERARCHY_ATTACHMENT_BYTES`], no field it does not know, at most
/// [`MAX_HIERARCHY_NODES`] nodes forming a tree of at most
/// [`MAX_HIERARCHY_DEPTH`] levels, and text within the capture's cap.
pub(super) fn checked_hierarchy(json: &[u8]) -> Result<DebugSessionHierarchy, String> {
    if json.len() as u64 > MAX_HIERARCHY_ATTACHMENT_BYTES {
        return Err(format!(
            "it is larger than {} MiB",
            MAX_HIERARCHY_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }
    let hierarchy: DebugSessionHierarchy = serde_json::from_slice(json)
        .map_err(|e| format!("it is not a UI hierarchy Keynobi writes ({e})"))?;
    if DateTime::parse_from_rfc3339(&hierarchy.captured_at).is_err() {
        return Err("its capture time is not a date".into());
    }
    if hierarchy.nodes.is_empty() {
        return Err("it has no nodes".into());
    }
    if hierarchy.nodes.len() > MAX_HIERARCHY_NODES {
        return Err(format!("it has more than {MAX_HIERARCHY_NODES} nodes"));
    }
    let mut previous: Option<u32> = None;
    for node in &hierarchy.nodes {
        let deepest = previous.map_or(0, |p| p + 1);
        if node.depth > deepest || node.depth >= MAX_HIERARCHY_DEPTH {
            return Err(format!(
                "its nodes do not form a tree of at most {MAX_HIERARCHY_DEPTH} levels"
            ));
        }
        previous = Some(node.depth);
    }
    let too_long = |text: &str| text.chars().count() > MAX_HIERARCHY_TEXT_CHARS;
    let long_text = hierarchy.nodes.iter().any(|n| {
        [
            &n.class,
            &n.resource_id,
            &n.text,
            &n.content_desc,
            &n.bounds,
        ]
        .into_iter()
        .any(|text| too_long(text))
    });
    if long_text
        || hierarchy
            .foreground_activity
            .as_deref()
            .is_some_and(too_long)
    {
        return Err(format!(
            "it has text longer than {MAX_HIERARCHY_TEXT_CHARS} characters"
        ));
    }
    Ok(hierarchy)
}

fn hierarchy_json<T: Serialize>(value: &T) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(value)
        .map_err(|e| AppError::Other(format!("Cannot serialize the UI hierarchy: {e}")))
}

/// `hierarchy` as JSON within the caps, and its node count: text cut to
/// [`MAX_HIERARCHY_TEXT_CHARS`], and nodes past [`MAX_HIERARCHY_DEPTH`],
/// [`MAX_HIERARCHY_NODES`], and [`MAX_HIERARCHY_ATTACHMENT_BYTES`] left out,
/// the last first, with `truncated` set when anything was cut.
pub(super) fn fitted_hierarchy(
    mut hierarchy: DebugSessionHierarchy,
) -> Result<(Vec<u8>, u32), AppError> {
    let mut cut = false;
    let mut fit = |text: &mut String| {
        if text.chars().count() > MAX_HIERARCHY_TEXT_CHARS {
            *text = truncate_chars(text, MAX_HIERARCHY_TEXT_CHARS);
            cut = true;
        }
    };
    if let Some(activity) = hierarchy.foreground_activity.as_mut() {
        fit(activity);
    }
    for node in &mut hierarchy.nodes {
        fit(&mut node.class);
        fit(&mut node.resource_id);
        fit(&mut node.text);
        fit(&mut node.content_desc);
        fit(&mut node.bounds);
    }
    // A node too deep takes its subtree, which follows it, with it.
    let before = hierarchy.nodes.len();
    hierarchy.nodes.retain(|n| n.depth < MAX_HIERARCHY_DEPTH);
    hierarchy.nodes.truncate(MAX_HIERARCHY_NODES);
    cut |= hierarchy.nodes.len() < before;

    let nodes = std::mem::take(&mut hierarchy.nodes);
    // Measured with `truncated: false`, the longer of the two.
    let mut total = hierarchy_json(&hierarchy)?.len();
    let mut kept = 0;
    for node in &nodes {
        let size = hierarchy_json(node)?.len() + 1;
        if (total + size) as u64 > MAX_HIERARCHY_ATTACHMENT_BYTES {
            break;
        }
        total += size;
        kept += 1;
    }
    cut |= kept < nodes.len();
    hierarchy.nodes = nodes;
    hierarchy.nodes.truncate(kept);
    hierarchy.truncated |= cut;
    let count = hierarchy.nodes.len() as u32;
    Ok((hierarchy_json(&hierarchy)?, count))
}

/// The nodes under `root` (the capture's synthetic root), in pre-order. A
/// password field's text is left out.
fn flatten(root: &UiNode) -> Vec<DebugSessionHierarchyNode> {
    let mut nodes = Vec::new();
    let mut stack: Vec<(&UiNode, u32)> = root.children.iter().rev().map(|c| (c, 0)).collect();
    while let Some((node, depth)) = stack.pop() {
        nodes.push(DebugSessionHierarchyNode {
            depth,
            class: node.class.clone(),
            resource_id: node.resource_id.clone(),
            text: if node.password {
                String::new()
            } else {
                node.text.clone()
            },
            content_desc: node.content_desc.clone(),
            bounds: node.bounds.clone(),
        });
        stack.extend(node.children.iter().rev().map(|c| (c, depth + 1)));
    }
    nodes
}

/// What of `snapshot` is attached; refused when the device showed no nodes.
fn hierarchy_of(snapshot: &UiHierarchySnapshot) -> Result<DebugSessionHierarchy, AppError> {
    let nodes = flatten(&snapshot.root);
    if nodes.is_empty() {
        let why = snapshot.warnings.join("; ");
        return Err(AppError::ProcessFailed(if why.is_empty() {
            "The device returned an empty UI hierarchy".into()
        } else {
            format!("The device returned no UI hierarchy: {why}")
        }));
    }
    Ok(DebugSessionHierarchy {
        captured_at: snapshot.captured_at.clone(),
        foreground_activity: snapshot.foreground_activity.clone(),
        truncated: snapshot.truncated,
        nodes,
    })
}

/// The serial of session `id`'s device, to capture `what` from: the session
/// must be open and recorded here, and an emulator must still run the
/// session's AVD.
async fn capture_target(dir: &Path, id: &str, adb: &Path, what: &str) -> Result<String, AppError> {
    checked_recorded_id(id)?;
    let session = read_session_in(dir, id, Utc::now())?;
    if session.closed_at.is_some() {
        return Err(AppError::InvalidInput(format!(
            "Debug session {id} is closed"
        )));
    }
    let serial = session.device.serial.clone();
    validate_device_serial(&serial).map_err(AppError::InvalidInput)?;
    if let Some(avd) = &session.device.avd_name {
        if adb_manager::resolve_avd_name(adb, &serial).await.as_ref() != Some(avd) {
            return Err(AppError::ProcessFailed(format!(
                "{avd} is not running on {serial}; start it to {what}"
            )));
        }
    }
    Ok(serial)
}

/// Take a screenshot of session `id`'s device and attach it. The session
/// must be open and recorded here, and its device online (an emulator must
/// still run the session's AVD).
pub async fn attach_screenshot(
    id: &str,
    adb: &PathBuf,
    by: BuildActor,
) -> Result<DebugSessionEvent, AppError> {
    let dir = data_dir();
    let serial = capture_target(&dir, id, adb, "take a screenshot").await?;
    let shot = take_screenshot_scaled(adb, &serial, Some(ATTACHMENT_SCREENSHOT_MAX_DIMENSION))
        .await
        .map_err(AppError::ProcessFailed)?;
    let id = id.to_string();
    tokio::task::spawn_blocking(move || {
        attach_png_in(&dir, &id, &shot.png, &serial, Some(by), Utc::now())
    })
    .await
    .map_err(|e| AppError::Other(format!("Debug session task failed: {e}")))?
}

/// Capture the UI hierarchy of session `id`'s device and attach it, with the
/// same conditions as [`attach_screenshot`].
pub async fn attach_hierarchy(
    id: &str,
    adb: &Path,
    by: BuildActor,
) -> Result<DebugSessionEvent, AppError> {
    attach_hierarchy_with(data_dir(), id, adb, by).await
}

pub(super) async fn attach_hierarchy_with(
    dir: PathBuf,
    id: &str,
    adb: &Path,
    by: BuildActor,
) -> Result<DebugSessionEvent, AppError> {
    let serial = capture_target(&dir, id, adb, "capture its UI hierarchy").await?;
    let snapshot = capture_ui_hierarchy_tree(adb, &serial)
        .await
        .map_err(AppError::ProcessFailed)?;
    let hierarchy = hierarchy_of(&snapshot)?;
    let id = id.to_string();
    tokio::task::spawn_blocking(move || {
        attach_hierarchy_in(&dir, &id, hierarchy, &serial, Some(by), Utc::now())
    })
    .await
    .map_err(|e| AppError::Other(format!("Debug session task failed: {e}")))?
}

/// Attach `png` to open session `id` under the data lock.
pub(super) fn attach_png_in(
    data_dir: &Path,
    id: &str,
    png: &[u8],
    serial: &str,
    actor: Option<BuildActor>,
    now: DateTime<Utc>,
) -> Result<DebugSessionEvent, AppError> {
    checked_recorded_id(id)?;
    let (width, height) = checked_png(png)
        .map_err(|e| AppError::InvalidInput(format!("The screenshot cannot be attached: {e}")))?;
    let attachment = |name, bytes| DebugSessionAttachment {
        kind: DebugSessionAttachmentKind::Screenshot,
        name,
        bytes,
        width: Some(width),
        height: Some(height),
        node_count: None,
        serial: serial.to_string(),
    };
    store_attachment_in(data_dir, id, png, screenshot_name, attachment, actor, now)
}

/// Attach `hierarchy`, fitted to the caps, to open session `id` under the
/// data lock.
pub(super) fn attach_hierarchy_in(
    data_dir: &Path,
    id: &str,
    hierarchy: DebugSessionHierarchy,
    serial: &str,
    actor: Option<BuildActor>,
    now: DateTime<Utc>,
) -> Result<DebugSessionEvent, AppError> {
    checked_recorded_id(id)?;
    let (json, node_count) = fitted_hierarchy(hierarchy)?;
    let attachment = |name, bytes| DebugSessionAttachment {
        kind: DebugSessionAttachmentKind::Hierarchy,
        name,
        bytes,
        width: None,
        height: None,
        node_count: Some(node_count),
        serial: serial.to_string(),
    };
    store_attachment_in(data_dir, id, &json, hierarchy_name, attachment, actor, now)
}

/// Write `data` as the file `name_of(seq)` of open session `id` and append
/// its `attachment` event, under the data lock and within the caps.
fn store_attachment_in(
    data_dir: &Path,
    id: &str,
    data: &[u8],
    name_of: fn(u32) -> String,
    attachment: impl FnOnce(String, u64) -> DebugSessionAttachment,
    actor: Option<BuildActor>,
    now: DateTime<Utc>,
) -> Result<DebugSessionEvent, AppError> {
    let full = |cap: &str| {
        AppError::InvalidInput(format!(
            "Debug session {id} is full ({cap}); no more attachments fit"
        ))
    };
    with_data_lock_in(data_dir, || {
        let mut index = load_index_locked(data_dir, now);
        let i = index
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| not_found(id))?;
        if !is_open(&index[i], now) {
            return Err(AppError::InvalidInput(format!(
                "Debug session {id} is closed"
            )));
        }
        let mut session = read_manifest(data_dir, id).map_err(AppError::Other)?;
        if session.counts.attachments >= MAX_ATTACHMENTS_PER_SESSION {
            return Err(full("MAX_ATTACHMENTS_PER_SESSION"));
        }
        if session.event_count >= MAX_EVENTS_PER_SESSION {
            return Err(full("MAX_EVENTS_PER_SESSION"));
        }
        let size = data.len() as u64;
        if session.bytes + size > MAX_SESSION_BYTES {
            return Err(full("MAX_SESSION_BYTES"));
        }
        let seq = session.event_count + 1;
        let name = name_of(seq);
        let folder = session_dir(data_dir, id).join(ATTACHMENTS_DIR);
        std::fs::create_dir_all(&folder).map_err(|e| AppError::io(folder.display(), e))?;
        let path = folder.join(&name);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(data))
            .map_err(|e| {
                let _ = std::fs::remove_file(&path);
                AppError::io(path.display(), e)
            })?;
        let event = DebugSessionEventData::Attachment(attachment(name, size));
        // Counted first, so the event's own line must fit beside the file.
        session.bytes += size;
        let appended = append_locked(data_dir, &mut session, actor, event, now);
        let recorded = match appended {
            Ok(Append::Recorded(event)) => *event,
            Ok(Append::Dropped(cap)) => {
                let _ = std::fs::remove_file(&path);
                return Err(full(cap));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(AppError::Other(e));
            }
        };
        save_manifest(data_dir, &session).map_err(AppError::Other)?;
        index[i] = DebugSessionSummary::from(&session);
        save_index(data_dir, &index).map_err(AppError::Other)?;
        Ok(recorded)
    })
    .map_err(AppError::Other)?
}

/// The attachment events among `events`, oldest first.
pub(super) fn attachment_events(events: &[DebugSessionEvent]) -> Vec<DebugSessionEvent> {
    events
        .iter()
        .filter(|e| matches!(e.event, DebugSessionEventData::Attachment(_)))
        .cloned()
        .collect()
}

/// The file `name` of session `id`'s attachments, at most `max` bytes (one
/// more tells it is larger).
fn read_attachment_file(
    data_dir: &Path,
    id: &str,
    seq: u32,
    name: &str,
    max: u64,
) -> Result<Vec<u8>, AppError> {
    checked_id(id)?;
    read_manifest(data_dir, id).map_err(|_| not_found(id))?;
    let path = session_dir(data_dir, id).join(ATTACHMENTS_DIR).join(name);
    let file = std::fs::File::open(&path).map_err(|_| {
        AppError::NotFound(format!(
            "Debug session {id} has no attachment for event {seq}"
        ))
    })?;
    let mut data = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut data)
        .map_err(|e| AppError::io(path.display(), e))?;
    Ok(data)
}

/// The screenshot of attachment event `seq` of session `id`, within the caps.
pub(super) fn read_attachment_in(data_dir: &Path, id: &str, seq: u32) -> Result<Vec<u8>, AppError> {
    let png = read_attachment_file(
        data_dir,
        id,
        seq,
        &screenshot_name(seq),
        MAX_ATTACHMENT_BYTES,
    )?;
    checked_png(&png).map_err(|e| {
        AppError::InvalidInput(format!("Attachment {seq} of {id} cannot be shown: {e}"))
    })?;
    Ok(png)
}

/// The UI hierarchy of attachment event `seq` of session `id`, checked as an
/// import checks it.
pub(super) fn read_hierarchy_in(
    data_dir: &Path,
    id: &str,
    seq: u32,
) -> Result<DebugSessionHierarchy, AppError> {
    let json = read_attachment_file(
        data_dir,
        id,
        seq,
        &hierarchy_name(seq),
        MAX_HIERARCHY_ATTACHMENT_BYTES,
    )?;
    checked_hierarchy(&json).map_err(|e| {
        AppError::InvalidInput(format!("Attachment {seq} of {id} cannot be shown: {e}"))
    })
}

/// Attachment `seq` of session `id` (recorded or imported), for display.
pub fn get_attachment(id: &str, seq: u32) -> Result<DebugSessionAttachmentData, AppError> {
    get_attachment_in(&data_dir(), id, seq)
}

pub(super) fn get_attachment_in(
    data_dir: &Path,
    id: &str,
    seq: u32,
) -> Result<DebugSessionAttachmentData, AppError> {
    let png = read_attachment_in(data_dir, id, seq)?;
    Ok(DebugSessionAttachmentData {
        seq,
        media_type: PNG_MEDIA_TYPE.into(),
        base64: base64::engine::general_purpose::STANDARD.encode(png),
    })
}

/// The UI hierarchy of attachment event `seq` of session `id` (recorded or
/// imported), for display.
pub fn get_hierarchy(id: &str, seq: u32) -> Result<DebugSessionHierarchy, AppError> {
    read_hierarchy_in(&data_dir(), id, seq)
}

#[cfg(test)]
mod tests {
    use super::super::export::build_bundle_in;
    use super::super::import::{import_bytes_in, list_imported_in};
    use super::*;
    use crate::models::build::InstalledBuild;
    use std::io::Cursor;
    use tempfile::TempDir;

    const RETAIN: Retention = Retention {
        days: 14,
        max_folder_mb: 200,
    };

    fn open(dir: &Path) -> DebugSession {
        open_on(dir, "R5CT1234ABC")
    }

    fn open_on(dir: &Path, serial: &str) -> DebugSession {
        let target = InstallTarget {
            serial: serial.into(),
            avd_name: None,
            model: Some("Pixel 8".into()),
        };
        let entry = InstalledBuild {
            serial: target.serial.clone(),
            avd_name: None,
            model: target.model.clone(),
            package: "com.example".into(),
            apk_sha256: "a".repeat(64),
            build_id: None,
            version_code: Some(7),
            mappings: vec![],
            installed_at: "2026-09-25T10:32:00+00:00".into(),
        };
        open_in(dir, &target, &entry, BuildActor::App, RETAIN, Utc::now()).expect("opened")
    }

    /// A `width` x `height` RGBA PNG; `noise` makes it barely compressible.
    fn png_of(width: u32, height: u32, noise: bool) -> Vec<u8> {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let pixels: Vec<u8> = (0..width as usize * height as usize * 4)
            .map(|i| {
                if noise {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                } else {
                    (i % 7) as u8
                }
            })
            .collect();
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
        out
    }

    fn attach(dir: &Path, id: &str, png: &[u8]) -> Result<DebugSessionEvent, AppError> {
        attach_png_in(
            dir,
            id,
            png,
            "R5CT1234ABC",
            Some(BuildActor::App),
            Utc::now(),
        )
    }

    fn refusal(result: Result<DebugSessionEvent, AppError>) -> String {
        match result {
            Err(AppError::InvalidInput(m)) => m,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_screenshot_is_stored_with_its_event_and_counts_toward_the_session_size() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let before = read_manifest(dir.path(), &session.id).unwrap();
        let png = png_of(24, 48, false);
        let event = attach(dir.path(), &session.id, &png).unwrap();

        let DebugSessionEventData::Attachment(a) = &event.event else {
            panic!("{event:?}")
        };
        assert_eq!(a.kind, DebugSessionAttachmentKind::Screenshot);
        assert_eq!(a.name, format!("screenshot-{}.png", event.seq));
        assert_eq!(
            (a.bytes, a.width, a.height, a.node_count),
            (png.len() as u64, Some(24), Some(48), None)
        );
        let stored = session_dir(dir.path(), &session.id)
            .join(ATTACHMENTS_DIR)
            .join(&a.name);
        assert_eq!(std::fs::read(stored).unwrap(), png);

        let after = read_manifest(dir.path(), &session.id).unwrap();
        assert_eq!(after.counts.attachments, 1);
        let line = serde_json::to_string(&event).unwrap().len() as u64 + 1;
        assert_eq!(after.bytes, before.bytes + png.len() as u64 + line);
        let listed = load_index_from(dir.path());
        assert_eq!(listed[0].counts.attachments, 1);

        let detail = get_session_in(dir.path(), &session.id, Utc::now()).unwrap();
        assert_eq!(detail.attachments, vec![event.clone()]);
        let data = get_attachment_in(dir.path(), &session.id, event.seq).unwrap();
        assert_eq!(data.media_type, "image/png");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(data.base64)
                .unwrap(),
            png
        );
        let err = get_attachment_in(dir.path(), &session.id, 999).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
    }

    #[test]
    fn attachments_are_capped_and_only_valid_pngs_go_in() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let png = png_of(8, 8, false);

        let message = refusal(attach(dir.path(), &session.id, b"GIF89a not a png"));
        assert!(message.contains("did not return a PNG"), "{message}");
        let mut too_big = png.clone();
        too_big.resize(MAX_ATTACHMENT_BYTES as usize + 1, 0);
        let message = refusal(attach(dir.path(), &session.id, &too_big));
        assert!(message.contains("larger than 4 MiB"), "{message}");

        for _ in 0..MAX_ATTACHMENTS_PER_SESSION {
            attach(dir.path(), &session.id, &png).unwrap();
        }
        let message = refusal(attach(dir.path(), &session.id, &png));
        assert!(message.contains("MAX_ATTACHMENTS_PER_SESSION"), "{message}");
        let files = std::fs::read_dir(session_dir(dir.path(), &session.id).join(ATTACHMENTS_DIR))
            .unwrap()
            .count();
        assert_eq!(files, MAX_ATTACHMENTS_PER_SESSION as usize);
    }

    #[test]
    fn a_full_or_closed_or_imported_session_takes_no_attachment() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let png = png_of(8, 8, false);

        let mut full = read_manifest(dir.path(), &session.id).unwrap();
        full.bytes = MAX_SESSION_BYTES - png.len() as u64;
        save_manifest(dir.path(), &full).unwrap();
        let message = refusal(attach(dir.path(), &session.id, &png));
        assert!(message.contains("MAX_SESSION_BYTES"), "{message}");
        assert!(!session_dir(dir.path(), &session.id)
            .join(ATTACHMENTS_DIR)
            .join(screenshot_name(full.event_count + 1))
            .exists());

        full.bytes = 0;
        save_manifest(dir.path(), &full).unwrap();
        let exported = build_bundle_in(
            dir.path(),
            &session.id,
            &SessionExportOptions::default(),
            None,
            Utc::now(),
        )
        .unwrap();
        let imported = import_bytes_in(dir.path(), &exported.bytes, "x.zip", Utc::now())
            .unwrap()
            .id;
        let message = refusal(attach(dir.path(), &imported, &png));
        assert!(message.contains("read-only"), "{message}");

        end_session_in(dir.path(), &session.id, RETAIN, Utc::now()).unwrap();
        let message = refusal(attach(dir.path(), &session.id, &png));
        assert!(message.contains("is closed"), "{message}");
    }

    #[test]
    fn attachments_count_toward_pruning_by_folder_size() {
        let dir = TempDir::new().unwrap();
        let one_mb = Retention {
            days: 14,
            max_folder_mb: 1,
        };
        let session = open(dir.path());
        prune_persisted_in(dir.path(), one_mb, Utc::now()).unwrap();
        assert_eq!(
            load_index_from(dir.path()).len(),
            1,
            "a small session stays"
        );

        let big = png_of(700, 700, true);
        assert!(big.len() > 1024 * 1024 && big.len() as u64 <= MAX_ATTACHMENT_BYTES);
        attach(dir.path(), &session.id, &big).unwrap();
        prune_persisted_in(dir.path(), one_mb, Utc::now()).unwrap();
        assert!(load_index_from(dir.path()).is_empty());
        assert!(!session_dir(dir.path(), &session.id).exists());
    }

    fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut file = archive.by_index(i).unwrap();
                let mut data = Vec::new();
                file.read_to_end(&mut data).unwrap();
                (file.name().to_string(), data)
            })
            .collect()
    }

    fn rezip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in files {
            zip.start_file(name.as_str(), options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn export(dir: &Path, id: &str, include_attachments: bool) -> Vec<(String, Vec<u8>)> {
        let options = SessionExportOptions {
            include_attachments,
            ..SessionExportOptions::default()
        };
        let bundle = build_bundle_in(dir, id, &options, None, Utc::now()).unwrap();
        unzip(&bundle.bytes)
    }

    #[test]
    fn export_includes_attachments_unless_left_out_and_says_they_are_not_redacted() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let png = png_of(16, 16, false);
        let event = attach(dir.path(), &session.id, &png).unwrap();
        let entry = format!("attachments/screenshot-{}.png", event.seq);

        let files = export(dir.path(), &session.id, true);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&entry.as_str()), "{names:?}");
        assert_eq!(files.iter().find(|(n, _)| *n == entry).unwrap().1, png);
        let report = &files.iter().find(|(n, _)| n == "redaction.json").unwrap().1;
        assert!(String::from_utf8_lossy(report).contains("screenshots are not redacted"));

        let files = export(dir.path(), &session.id, false);
        assert!(files.iter().all(|(n, _)| !n.starts_with("attachments/")));
        let manifest = &files.iter().find(|(n, _)| n == "manifest.json").unwrap().1;
        let manifest: serde_json::Value = serde_json::from_slice(manifest).unwrap();
        assert!(manifest["omitted"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["item"] == "attachments" && o["reason"] == "not selected"));
    }

    #[test]
    fn import_keeps_valid_screenshots_and_refuses_what_is_not_a_png() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let png = png_of(16, 16, false);
        let event = attach(dir.path(), &session.id, &png).unwrap();
        let entry = format!("attachments/screenshot-{}.png", event.seq);
        let files = export(dir.path(), &session.id, true);

        let imported = import_bytes_in(dir.path(), &rezip(&files), "x.zip", Utc::now()).unwrap();
        assert_eq!(imported.counts.attachments, 1);
        let data = get_attachment_in(dir.path(), &imported.id, event.seq).unwrap();
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(data.base64)
                .unwrap(),
            png
        );
        let events = std::fs::metadata(session_dir(dir.path(), &imported.id).join(EVENTS_FILE))
            .unwrap()
            .len();
        assert_eq!(imported.bytes, events + png.len() as u64);

        let refused = |data: Vec<u8>| {
            let mut changed = files.clone();
            changed.iter_mut().find(|(n, _)| *n == entry).unwrap().1 = data;
            match import_bytes_in(dir.path(), &rezip(&changed), "x.zip", Utc::now()) {
                Err(AppError::InvalidInput(m)) => m,
                other => panic!("{other:?}"),
            }
        };
        let message = refused(b"\xff\xd8\xff\xe0 a jpeg".to_vec());
        assert!(message.contains("did not return a PNG"), "{message}");
        let mut truncated = png.clone();
        truncated.truncate(20);
        let message = refused(truncated);
        assert!(message.contains("not a valid PNG"), "{message}");
        // Past the size cap, with bytes that do not compress.
        let mut big = png.clone();
        while big.len() <= MAX_ATTACHMENT_BYTES as usize {
            big.extend(png_of(800, 800, true));
        }
        big.truncate(MAX_ATTACHMENT_BYTES as usize + 1);
        let message = refused(big);
        assert!(message.contains("larger than 4 MiB"), "{message}");

        // A screenshot no attachment event names is left out.
        let mut orphan = files.clone();
        orphan.push(("attachments/screenshot-999.png".into(), png.clone()));
        let names: Vec<String> = orphan.iter().map(|(n, _)| n.clone()).collect();
        let manifest = &mut orphan
            .iter_mut()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1;
        let mut value: serde_json::Value = serde_json::from_slice(manifest).unwrap();
        value["entries"] = serde_json::json!(names);
        *manifest = serde_json::to_vec(&value).unwrap();
        let kept = import_bytes_in(dir.path(), &rezip(&orphan), "x.zip", Utc::now()).unwrap();
        let session = read_manifest(dir.path(), &kept.id).unwrap();
        assert!(session
            .imported
            .unwrap()
            .omitted
            .iter()
            .any(|o| o.item == "attachments/screenshot-999.png"));
        assert_eq!(list_imported_in(dir.path()).len(), 2);
    }

    // ── UI hierarchies ────────────────────────────────────────────────────────

    const DUMP: &str = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node text="" resource-id="" class="android.widget.FrameLayout" package="com.example" content-desc="" password="false" bounds="[0,0][1080,2400]">
    <node text="Sign in" resource-id="com.example:id/title" class="android.widget.TextView" package="com.example" content-desc="" password="false" bounds="[48,200][400,280]"/>
    <node text="hunter2" resource-id="com.example:id/password" class="android.widget.EditText" package="com.example" content-desc="" password="true" bounds="[48,300][1032,400]"/>
    <node text="" resource-id="com.example:id/list" class="android.widget.LinearLayout" package="com.example" content-desc="" password="false" bounds="[0,500][1080,900]">
      <node text="Row" resource-id="" class="android.widget.TextView" package="com.example" content-desc="First row" password="false" bounds="[0,500][1080,600]"/>
    </node>
  </node>
</hierarchy>"#;

    /// An `adb` that logs each call to `calls`, dumps `DUMP` for
    /// `uiautomator`, and names a resumed activity.
    fn fake_adb(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(dir.join("dump.xml"), DUMP).unwrap();
        let adb = dir.join("adb");
        let script = format!(
            "#!/bin/sh\nD='{}'\necho \"$*\" >> \"$D/calls\"\ncase \"$*\" in\n\
             *uiautomator*) cat \"$D/dump.xml\" ;;\n\
             *'dumpsys activity'*) echo '  topResumedActivity=ActivityRecord{{1 u0 com.example/.SignIn t9}}' ;;\n\
             *) exit 0 ;;\nesac\n",
            dir.display()
        );
        std::fs::write(&adb, script).unwrap();
        std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
        adb
    }

    fn calls(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn node(depth: u32, class: &str, text: &str) -> DebugSessionHierarchyNode {
        DebugSessionHierarchyNode {
            depth,
            class: class.into(),
            resource_id: String::new(),
            text: text.into(),
            content_desc: String::new(),
            bounds: "[0,0][10,10]".into(),
        }
    }

    fn hierarchy(nodes: Vec<DebugSessionHierarchyNode>) -> DebugSessionHierarchy {
        DebugSessionHierarchy {
            captured_at: "2026-09-26T10:00:00.000000Z".into(),
            foreground_activity: None,
            truncated: false,
            nodes,
        }
    }

    fn attach_tree(
        dir: &Path,
        id: &str,
        tree: DebugSessionHierarchy,
    ) -> Result<DebugSessionEvent, AppError> {
        attach_hierarchy_in(
            dir,
            id,
            tree,
            "R5CT1234ABC",
            Some(BuildActor::App),
            Utc::now(),
        )
    }

    /// `count` nodes of `chars` characters each that do not compress well.
    fn noisy_nodes(count: usize, chars: usize) -> Vec<DebugSessionHierarchyNode> {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        (0..count)
            .map(|i| {
                let text: String = (0..chars)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        char::from(b'a' + (state % 26) as u8)
                    })
                    .collect();
                node(u32::from(i > 0), "android.widget.TextView", &text)
            })
            .collect()
    }

    #[tokio::test]
    async fn a_hierarchy_is_captured_from_the_device_and_stored_with_its_event() {
        let dir = TempDir::new().unwrap();
        let adb_dir = TempDir::new().unwrap();
        let adb = fake_adb(adb_dir.path());
        let session = open_on(dir.path(), "HIERCAP01");
        let before = read_manifest(dir.path(), &session.id).unwrap();

        let event =
            attach_hierarchy_with(dir.path().to_path_buf(), &session.id, &adb, BuildActor::App)
                .await
                .unwrap();

        let DebugSessionEventData::Attachment(a) = &event.event else {
            panic!("{event:?}")
        };
        assert_eq!(a.kind, DebugSessionAttachmentKind::Hierarchy);
        assert_eq!(a.name, format!("hierarchy-{}.json", event.seq));
        assert_eq!((a.node_count, a.width, a.height), (Some(5), None, None));
        assert_eq!(a.serial, "HIERCAP01");
        assert!(well_formed(a, event.seq));

        let stored = read_hierarchy_in(dir.path(), &session.id, event.seq).unwrap();
        let depths: Vec<u32> = stored.nodes.iter().map(|n| n.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 1, 2]);
        assert_eq!(stored.nodes[1].text, "Sign in");
        assert_eq!(stored.nodes[1].resource_id, "com.example:id/title");
        assert_eq!(stored.nodes[1].bounds, "[48,200][400,280]");
        assert_eq!(stored.nodes[4].content_desc, "First row");
        assert_eq!(
            stored.nodes[2].text, "",
            "a password field's text is left out"
        );
        assert!(stored
            .foreground_activity
            .as_deref()
            .is_some_and(|a| a.contains("com.example/.SignIn")));
        assert!(!stored.truncated);
        let file = session_dir(dir.path(), &session.id)
            .join(ATTACHMENTS_DIR)
            .join(&a.name);
        assert_eq!(std::fs::metadata(&file).unwrap().len(), a.bytes);

        let after = read_manifest(dir.path(), &session.id).unwrap();
        assert_eq!(after.counts.attachments, 1);
        let line = serde_json::to_string(&event).unwrap().len() as u64 + 1;
        assert_eq!(after.bytes, before.bytes + a.bytes + line);
        let detail = get_session_in(dir.path(), &session.id, Utc::now()).unwrap();
        assert_eq!(detail.attachments, vec![event.clone()]);

        // Only the tree: no screenshot and no layout probes.
        let calls = calls(adb_dir.path());
        assert!(calls.iter().any(|c| c.contains("uiautomator dump")));
        assert!(
            calls
                .iter()
                .all(|c| !c.contains("screencap") && !c.contains("wm size")),
            "{calls:?}"
        );
        // A screenshot read of a hierarchy event finds none.
        let err = get_attachment_in(dir.path(), &session.id, event.seq).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "{err}");
    }

    #[tokio::test]
    async fn a_busy_device_is_not_touched_and_its_lock_is_waited_for() {
        use crate::services::ui_automator_lock::test_support::begin_instrumentation_on;
        let dir = TempDir::new().unwrap();
        let adb_dir = TempDir::new().unwrap();
        let adb = fake_adb(adb_dir.path());
        let session = open_on(dir.path(), "HIERBUSY01");

        let run = begin_instrumentation_on("HIERBUSY01");
        let err =
            attach_hierarchy_with(dir.path().to_path_buf(), &session.id, &adb, BuildActor::App)
                .await
                .unwrap_err();
        assert!(err.to_string().contains("busy"), "{err}");
        assert!(
            calls(adb_dir.path()).is_empty(),
            "the device was sent a command"
        );
        let after = read_manifest(dir.path(), &session.id).unwrap();
        assert_eq!(after.counts.attachments, 0);

        drop(run);
        attach_hierarchy_with(dir.path().to_path_buf(), &session.id, &adb, BuildActor::App)
            .await
            .expect("the device is free once the run ends");
    }

    #[tokio::test]
    async fn a_closed_or_imported_session_takes_no_hierarchy_and_nothing_is_captured() {
        let dir = TempDir::new().unwrap();
        let adb_dir = TempDir::new().unwrap();
        let adb = fake_adb(adb_dir.path());
        let session = open_on(dir.path(), "HIERCLOSED1");
        end_session_in(dir.path(), &session.id, RETAIN, Utc::now()).unwrap();
        let err =
            attach_hierarchy_with(dir.path().to_path_buf(), &session.id, &adb, BuildActor::App)
                .await
                .unwrap_err();
        assert!(err.to_string().contains("is closed"), "{err}");
        let err = attach_hierarchy_with(
            dir.path().to_path_buf(),
            "i-20260926T100000Z-0123456789ab",
            &adb,
            BuildActor::App,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"), "{err}");
        assert!(calls(adb_dir.path()).is_empty());
    }

    #[test]
    fn a_hierarchy_is_fitted_to_its_caps() {
        // Text past the capture's cap is cut.
        let long = "x".repeat(MAX_HIERARCHY_TEXT_CHARS + 10);
        let (json, count) = fitted_hierarchy(hierarchy(vec![node(0, "a.B", &long)])).unwrap();
        let fitted = checked_hierarchy(&json).unwrap();
        assert_eq!(count, 1);
        assert!(fitted.truncated);
        assert_eq!(
            fitted.nodes[0].text.chars().count(),
            MAX_HIERARCHY_TEXT_CHARS
        );

        // Nodes past the depth cap go with their subtree; the tree stays whole.
        let mut deep: Vec<_> = (0..MAX_HIERARCHY_DEPTH + 3)
            .map(|d| node(d, "a.B", ""))
            .collect();
        deep.push(node(1, "a.C", "after"));
        let (json, count) = fitted_hierarchy(hierarchy(deep)).unwrap();
        let fitted = checked_hierarchy(&json).unwrap();
        assert_eq!(count, MAX_HIERARCHY_DEPTH + 1);
        assert_eq!(fitted.nodes.last().unwrap().text, "after");
        assert!(fitted.truncated);

        // The last nodes are left out to fit the file cap.
        let (json, count) = fitted_hierarchy(hierarchy(noisy_nodes(1_500, 2_000))).unwrap();
        assert!(json.len() as u64 <= MAX_HIERARCHY_ATTACHMENT_BYTES);
        assert!(json.len() as u64 > MAX_HIERARCHY_ATTACHMENT_BYTES - 3_000);
        assert!((count as usize) < 1_500);
        let fitted = checked_hierarchy(&json).unwrap();
        assert!(fitted.truncated);
        assert_eq!(fitted.nodes.len(), count as usize);

        // A tree within the caps is kept as it is.
        let (json, _) = fitted_hierarchy(hierarchy(vec![node(0, "a.B", "hi")])).unwrap();
        assert!(!checked_hierarchy(&json).unwrap().truncated);
    }

    #[test]
    fn hierarchies_count_toward_the_attachment_and_session_caps() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let png = png_of(8, 8, false);
        for _ in 0..MAX_ATTACHMENTS_PER_SESSION - 1 {
            attach(dir.path(), &session.id, &png).unwrap();
        }
        attach_tree(dir.path(), &session.id, hierarchy(vec![node(0, "a.B", "")])).unwrap();
        let message = refusal(attach_tree(
            dir.path(),
            &session.id,
            hierarchy(vec![node(0, "a.B", "")]),
        ));
        assert!(message.contains("MAX_ATTACHMENTS_PER_SESSION"), "{message}");
        let message = refusal(attach(dir.path(), &session.id, &png));
        assert!(message.contains("MAX_ATTACHMENTS_PER_SESSION"), "{message}");

        let other = open_on(dir.path(), "HIERFULL01");
        let mut full = read_manifest(dir.path(), &other.id).unwrap();
        full.bytes = MAX_SESSION_BYTES - 10;
        save_manifest(dir.path(), &full).unwrap();
        let message = refusal(attach_tree(
            dir.path(),
            &other.id,
            hierarchy(vec![node(0, "a.B", "")]),
        ));
        assert!(message.contains("MAX_SESSION_BYTES"), "{message}");
        assert!(!session_dir(dir.path(), &other.id)
            .join(ATTACHMENTS_DIR)
            .join(hierarchy_name(full.event_count + 1))
            .exists());
    }

    #[test]
    fn hierarchies_count_toward_pruning_by_folder_size() {
        let dir = TempDir::new().unwrap();
        let one_mb = Retention {
            days: 14,
            max_folder_mb: 1,
        };
        let session = open(dir.path());
        prune_persisted_in(dir.path(), one_mb, Utc::now()).unwrap();
        assert_eq!(load_index_from(dir.path()).len(), 1);

        let event =
            attach_tree(dir.path(), &session.id, hierarchy(noisy_nodes(600, 2_000))).unwrap();
        let DebugSessionEventData::Attachment(a) = &event.event else {
            panic!("{event:?}")
        };
        assert!(a.bytes > 1024 * 1024);
        prune_persisted_in(dir.path(), one_mb, Utc::now()).unwrap();
        assert!(load_index_from(dir.path()).is_empty());
        assert!(!session_dir(dir.path(), &session.id).exists());
    }

    fn entry<'a>(files: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
        &files.iter().find(|(n, _)| n == name).unwrap().1
    }

    #[test]
    fn export_redacts_hierarchy_text_and_leaves_it_out_with_the_attachments() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let mut tree = hierarchy(vec![
            node(0, "android.widget.FrameLayout", ""),
            node(
                1,
                "android.widget.TextView",
                "Signed in as jane.doe@example.com",
            ),
            node(1, "android.widget.TextView", ""),
            node(1, "android.widget.TextView", ""),
        ]);
        tree.nodes[2].content_desc = "api_key=sk-live0123456789abcdefghijkl".into();
        tree.nodes[3].resource_id = "com.example:id/jane.doe@example.com".into();
        tree.nodes[3].text = "Saved to /Users/jane/Downloads/report.pdf".into();
        let event = attach_tree(dir.path(), &session.id, tree).unwrap();
        let name = format!("attachments/hierarchy-{}.json", event.seq);

        let bundle = build_bundle_in(
            dir.path(),
            &session.id,
            &SessionExportOptions::default(),
            Some("/Users/jane".into()),
            Utc::now(),
        )
        .unwrap();
        let files = unzip(&bundle.bytes);
        let exported = String::from_utf8(entry(&files, &name).to_vec()).unwrap();
        for secret in ["jane.doe@example.com", "sk-live0123456789", "/Users/jane"] {
            assert!(!exported.contains(secret), "{secret} in {exported}");
        }
        let redacted = checked_hierarchy(entry(&files, &name)).unwrap();
        assert_eq!(redacted.nodes.len(), 4);
        assert_eq!(redacted.nodes[1].text, "Signed in as <email-1>");
        assert_eq!(redacted.nodes[3].resource_id, "com.example:id/<email-1>");
        assert_eq!(redacted.nodes[3].text, "Saved to ~/Downloads/report.pdf");
        assert!(
            !redacted.nodes[2].content_desc.contains("sk-live"),
            "{}",
            redacted.nodes[2].content_desc
        );
        let emails = bundle
            .redactions
            .iter()
            .find(|r| r.rule == crate::models::redaction::RedactionRule::Emails)
            .unwrap();
        assert!(emails.count >= 2, "{emails:?}");
        let report = String::from_utf8_lossy(entry(&files, "redaction.json")).into_owned();
        assert!(!report.contains("not redacted"), "{report}");

        // The stored file is untouched.
        let stored = read_hierarchy_in(dir.path(), &session.id, event.seq).unwrap();
        assert!(stored.nodes[1].text.contains("jane.doe@example.com"));

        let files = export(dir.path(), &session.id, false);
        assert!(files.iter().all(|(n, _)| !n.starts_with("attachments/")));
    }

    /// `files` with `name` set to `data` (added when missing) and the
    /// manifest listing what is there, zipped.
    fn with_file(files: &[(String, Vec<u8>)], name: &str, data: Vec<u8>) -> Vec<u8> {
        let mut changed = files.to_vec();
        match changed.iter_mut().find(|(n, _)| n == name) {
            Some(file) => file.1 = data,
            None => changed.push((name.to_string(), data)),
        }
        let names: Vec<String> = changed.iter().map(|(n, _)| n.clone()).collect();
        let manifest = &mut changed
            .iter_mut()
            .find(|(n, _)| n == "manifest.json")
            .unwrap()
            .1;
        let mut value: serde_json::Value = serde_json::from_slice(manifest).unwrap();
        value["entries"] = serde_json::json!(names);
        *manifest = serde_json::to_vec(&value).unwrap();
        rezip(&changed)
    }

    fn import_refusal(dir: &Path, bundle: &[u8]) -> String {
        match import_bytes_in(dir, bundle, "x.zip", Utc::now()) {
            Err(AppError::InvalidInput(m)) => m,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn import_keeps_valid_hierarchies_and_refuses_what_this_version_does_not_write() {
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let tree = hierarchy(vec![
            node(0, "android.widget.FrameLayout", ""),
            node(1, "android.widget.Button", "OK"),
        ]);
        let event = attach_tree(dir.path(), &session.id, tree.clone()).unwrap();
        let name = format!("attachments/hierarchy-{}.json", event.seq);
        let files = export(dir.path(), &session.id, true);

        let imported = import_bytes_in(dir.path(), &rezip(&files), "x.zip", Utc::now()).unwrap();
        assert_eq!(imported.counts.attachments, 1);
        let read = read_hierarchy_in(dir.path(), &imported.id, event.seq).unwrap();
        assert_eq!(read.nodes, tree.nodes);
        let folder = session_dir(dir.path(), &imported.id);
        let events = std::fs::metadata(folder.join(EVENTS_FILE)).unwrap().len();
        let stored =
            std::fs::metadata(folder.join(ATTACHMENTS_DIR).join(hierarchy_name(event.seq)))
                .unwrap()
                .len();
        assert_eq!(imported.bytes, events + stored);

        let valid: serde_json::Value = serde_json::from_slice(entry(&files, &name)).unwrap();
        let refused_with = |value: serde_json::Value| {
            let bundle = with_file(&files, &name, serde_json::to_vec(&value).unwrap());
            import_refusal(dir.path(), &bundle)
        };

        // A node deeper than its parent allows, and a chain past the depth cap.
        let mut skip = valid.clone();
        skip["nodes"][1]["depth"] = serde_json::json!(5);
        let message = refused_with(skip);
        assert!(message.contains("do not form a tree"), "{message}");
        let mut chain = valid.clone();
        chain["nodes"] = serde_json::json!((0..=MAX_HIERARCHY_DEPTH)
            .map(|d| serde_json::to_value(node(d, "a.B", "")).unwrap())
            .collect::<Vec<_>>());
        let message = refused_with(chain);
        assert!(message.contains("do not form a tree"), "{message}");

        // Nesting as deep as the file allows is refused without recursing.
        let bomb = format!(
            "{{\"capturedAt\":\"2026-09-26T10:00:00Z\",\"foregroundActivity\":null,\
             \"truncated\":false,\"nodes\":{}{}}}",
            "[".repeat(200_000),
            "]".repeat(200_000)
        );
        let message = import_refusal(dir.path(), &with_file(&files, &name, bomb.into_bytes()));
        assert!(
            message.contains("not a UI hierarchy Keynobi writes"),
            "{message}"
        );

        // A field this version does not know, in a node and at the top.
        let mut unknown = valid.clone();
        unknown["nodes"][0]["password"] = serde_json::json!("hunter2");
        let message = refused_with(unknown);
        assert!(message.contains("unknown field"), "{message}");
        let mut unknown = valid.clone();
        unknown["screenshot"] = serde_json::json!("iVBOR");
        let message = refused_with(unknown);
        assert!(message.contains("unknown field"), "{message}");

        // Too many nodes, text past the cap, and a file past the size cap.
        let mut many = valid.clone();
        many["nodes"] = serde_json::json!((0..=MAX_HIERARCHY_NODES)
            .map(|i| serde_json::to_value(node(u32::from(i > 0), "a", "")).unwrap())
            .collect::<Vec<_>>());
        let message = refused_with(many);
        assert!(message.contains("more than 8000 nodes"), "{message}");
        let mut long = valid.clone();
        long["nodes"][1]["text"] = serde_json::json!("x".repeat(MAX_HIERARCHY_TEXT_CHARS + 1));
        let message = refused_with(long);
        assert!(message.contains("longer than"), "{message}");
        let mut big = valid.clone();
        big["nodes"] = serde_json::to_value(noisy_nodes(1_200, 2_000)).unwrap();
        let message = refused_with(big);
        assert!(message.contains("larger than 2 MiB"), "{message}");

        // An attachment event shaped unlike this version's is refused.
        let timeline = String::from_utf8(entry(&files, "timeline.jsonl").to_vec())
            .unwrap()
            .replace("\"nodeCount\":2", "\"nodeCount\":2,\"width\":1");
        let message = import_refusal(
            dir.path(),
            &with_file(&files, "timeline.jsonl", timeline.into_bytes()),
        );
        assert!(message.contains("an attachment this version"), "{message}");

        // A hierarchy no event names is left out.
        let orphan = "attachments/hierarchy-999.json";
        let kept = import_bytes_in(
            dir.path(),
            &with_file(&files, orphan, entry(&files, &name).to_vec()),
            "x.zip",
            Utc::now(),
        )
        .unwrap();
        let omitted = read_manifest(dir.path(), &kept.id)
            .unwrap()
            .imported
            .unwrap()
            .omitted;
        assert!(omitted
            .iter()
            .any(|o| o.item == orphan && o.reason == "no attachment of the timeline names it"));
        assert!(read_hierarchy_in(dir.path(), &kept.id, 999).is_err());
        assert_eq!(kept.counts.attachments, 1);
    }

    #[test]
    fn get_debug_session_lists_screenshots_and_hierarchies() {
        use super::super::agent::{session_for_agent_in, AgentSessionRequest};
        let dir = TempDir::new().unwrap();
        let session = open(dir.path());
        let shot = attach(dir.path(), &session.id, &png_of(8, 4, false)).unwrap();
        let tree = attach_tree(
            dir.path(),
            &session.id,
            hierarchy(vec![node(0, "a.B", ""), node(1, "a.C", "")]),
        )
        .unwrap();
        let request = AgentSessionRequest {
            id: session.id.clone(),
            before_seq: None,
            max_events: 10,
            capture_seq: None,
            log_lines: 1,
        };
        let out = session_for_agent_in(dir.path(), &request, Utc::now()).unwrap();
        assert_eq!(out["session"]["counts"]["attachments"], 2);
        assert_eq!(out["attachments_truncated"], false);
        let listed = out["attachments"].as_array().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0]["seq"], shot.seq);
        assert_eq!(listed[0]["kind"], "screenshot");
        assert_eq!(
            (listed[0]["width"].clone(), listed[0]["height"].clone()),
            (8.into(), 4.into())
        );
        assert_eq!(listed[1]["seq"], tree.seq);
        assert_eq!(listed[1]["kind"], "hierarchy");
        assert_eq!(listed[1]["name"], format!("hierarchy-{}.json", tree.seq));
        assert_eq!(listed[1]["node_count"], 2);
        assert!(listed[1]["width"].is_null());
        let summary = out["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["seq"] == tree.seq)
            .unwrap()["summary"]
            .clone();
        assert!(
            summary.as_str().unwrap().starts_with(&format!(
                "UI hierarchy attached: hierarchy-{}.json (2 nodes, ",
                tree.seq
            )),
            "{summary}"
        );
    }
}
