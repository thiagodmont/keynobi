//! Screenshots attached to a debug session: `<id>/attachments/screenshot-<seq>.png`,
//! each with an `attachment` event named by its `seq`.
//!
//! A screenshot is taken on request from the session's device through the
//! screenshot service, fitted to [`ATTACHMENT_SCREENSHOT_MAX_DIMENSION`], and
//! stored while the session is open, within [`MAX_ATTACHMENTS_PER_SESSION`],
//! [`MAX_ATTACHMENT_BYTES`] each, and [`MAX_SESSION_BYTES`] together with
//! the event log (the session's `bytes`). The folder is pruned with its session.

use super::*;
use crate::services::adb_manager;
use crate::services::device_inspector::{
    fit_screenshot, take_screenshot_scaled, DEFAULT_SCREENSHOT_MAX_DIMENSION,
};
use crate::utils::validation::validate_device_serial;
use base64::Engine as _;

/// Most attachments in one session.
pub const MAX_ATTACHMENTS_PER_SESSION: u32 = 10;
/// Largest attachment.
pub const MAX_ATTACHMENT_BYTES: u64 = 4 * 1024 * 1024;
/// Long edge a screenshot is fitted to before it is attached.
pub const ATTACHMENT_SCREENSHOT_MAX_DIMENSION: u32 = DEFAULT_SCREENSHOT_MAX_DIMENSION;

pub(super) const ATTACHMENTS_DIR: &str = "attachments";
const PNG_MEDIA_TYPE: &str = "image/png";

/// The file of screenshot event `seq`.
pub(super) fn screenshot_name(seq: u32) -> String {
    format!("screenshot-{seq}.png")
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

/// Take a screenshot of session `id`'s device and attach it. The session
/// must be open and recorded here, and its device online (an emulator must
/// still run the session's AVD).
pub async fn attach_screenshot(
    id: &str,
    adb: &PathBuf,
    by: BuildActor,
) -> Result<DebugSessionEvent, AppError> {
    let dir = data_dir();
    checked_recorded_id(id)?;
    let session = read_session_in(&dir, id, Utc::now())?;
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
                "{avd} is not running on {serial}; start it to take a screenshot"
            )));
        }
    }
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
        let size = png.len() as u64;
        if session.bytes + size > MAX_SESSION_BYTES {
            return Err(full("MAX_SESSION_BYTES"));
        }
        let seq = session.event_count + 1;
        let name = screenshot_name(seq);
        let folder = session_dir(data_dir, id).join(ATTACHMENTS_DIR);
        std::fs::create_dir_all(&folder).map_err(|e| AppError::io(folder.display(), e))?;
        let path = folder.join(&name);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(png))
            .map_err(|e| {
                let _ = std::fs::remove_file(&path);
                AppError::io(path.display(), e)
            })?;
        let event = DebugSessionEventData::Attachment(DebugSessionAttachment {
            kind: DebugSessionAttachmentKind::Screenshot,
            name,
            bytes: size,
            width,
            height,
            serial: serial.to_string(),
        });
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

/// The file of attachment event `seq` of session `id`, within the caps.
pub(super) fn read_attachment_in(data_dir: &Path, id: &str, seq: u32) -> Result<Vec<u8>, AppError> {
    checked_id(id)?;
    read_manifest(data_dir, id).map_err(|_| not_found(id))?;
    let path = session_dir(data_dir, id)
        .join(ATTACHMENTS_DIR)
        .join(screenshot_name(seq));
    let file = std::fs::File::open(&path).map_err(|_| {
        AppError::NotFound(format!(
            "Debug session {id} has no attachment for event {seq}"
        ))
    })?;
    let mut png = Vec::new();
    file.take(MAX_ATTACHMENT_BYTES + 1)
        .read_to_end(&mut png)
        .map_err(|e| AppError::io(path.display(), e))?;
    checked_png(&png).map_err(|e| {
        AppError::InvalidInput(format!("Attachment {seq} of {id} cannot be shown: {e}"))
    })?;
    Ok(png)
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
        let target = InstallTarget {
            serial: "R5CT1234ABC".into(),
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
        assert_eq!((a.bytes, a.width, a.height), (png.len() as u64, 24, 48));
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
}
