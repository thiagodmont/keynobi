use crate::utils::device_shell::quote_device_shell_arg;
use crate::utils::process::{
    describe_failure, output_with_timeout, ADB_QUERY_TIMEOUT, ADB_SCREENSHOT_TIMEOUT,
    ADB_UNRESPONSIVE_HINT,
};
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub build_version_sdk: Option<String>,
    pub build_version_release: Option<String>,
    pub product_manufacturer: Option<String>,
    pub product_model: Option<String>,
    pub product_name: Option<String>,
    pub build_fingerprint: Option<String>,
    pub build_id: Option<String>,
    pub display_size: Option<String>,
    pub battery: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct DumpedAppInfo {
    pub package: String,
    pub install_path: Option<String>,
    pub data_dir: Option<String>,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub first_install: Option<String>,
    pub raw_dump_excerpt: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryInfo {
    pub package: String,
    pub total_pss: Option<String>,
    pub java_heap_pss: Option<String>,
    pub java_heap_rss: Option<String>,
    pub native_heap_pss: Option<String>,
    pub native_heap_rss: Option<String>,
    pub graphics_pss: Option<String>,
    pub graphics_rss: Option<String>,
    pub raw: String,
}

#[allow(clippy::ptr_arg)]
pub async fn get_device_info(adb: &PathBuf, serial: &str) -> Result<DeviceInfo, String> {
    let shell = |args: &'static [&'static str]| async move {
        output_with_timeout(
            Command::new(adb).args(["-s", serial, "shell"]).args(args),
            ADB_QUERY_TIMEOUT,
        )
        .await
    };
    let mk_getprop = |prop: &'static str| async move {
        output_with_timeout(
            Command::new(adb).args(["-s", serial, "shell", "getprop", prop]),
            ADB_QUERY_TIMEOUT,
        )
        .await
    };

    let (sdk, release, manufacturer, model, name, fingerprint, build_id, wm_size, battery) = tokio::join!(
        mk_getprop("ro.build.version.sdk"),
        mk_getprop("ro.build.version.release"),
        mk_getprop("ro.product.manufacturer"),
        mk_getprop("ro.product.model"),
        mk_getprop("ro.product.name"),
        mk_getprop("ro.build.fingerprint"),
        mk_getprop("ro.build.id"),
        shell(&["wm", "size"]),
        shell(&["dumpsys", "battery"]),
    );

    // The probes run concurrently, so an unresponsive device times out all of
    // them; report that instead of an all-empty result.
    if let Err(e) = &sdk {
        if e.kind() == std::io::ErrorKind::TimedOut {
            return Err(describe_failure("adb getprop", e, ADB_UNRESPONSIVE_HINT));
        }
    }

    let prop_val = |res: Result<std::process::Output, _>| -> Option<String> {
        let s = res
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    };

    let battery_str = battery
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let battery_level = battery_str
        .lines()
        .find(|l| l.trim_start().starts_with("level:"))
        .map(|l| l.trim().to_string());

    let size_str = wm_size
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty());

    Ok(DeviceInfo {
        build_version_sdk: prop_val(sdk),
        build_version_release: prop_val(release),
        product_manufacturer: prop_val(manufacturer),
        product_model: prop_val(model),
        product_name: prop_val(name),
        build_fingerprint: prop_val(fingerprint),
        build_id: prop_val(build_id),
        display_size: size_str,
        battery: battery_level,
    })
}

#[allow(clippy::ptr_arg)]
pub async fn dump_app_info(
    adb: &PathBuf,
    serial: &str,
    package: &str,
) -> Result<DumpedAppInfo, String> {
    let (path_res, dump_res) = tokio::join!(
        async {
            output_with_timeout(
                Command::new(adb).args([
                    "-s",
                    serial,
                    "shell",
                    "pm",
                    "path",
                    &quote_device_shell_arg(package),
                ]),
                ADB_QUERY_TIMEOUT,
            )
            .await
        },
        async {
            output_with_timeout(
                Command::new(adb).args([
                    "-s",
                    serial,
                    "shell",
                    "dumpsys",
                    "package",
                    &quote_device_shell_arg(package),
                ]),
                ADB_QUERY_TIMEOUT,
            )
            .await
        },
    );

    if let Err(e) = &dump_res {
        if e.kind() == std::io::ErrorKind::TimedOut {
            return Err(describe_failure(
                "adb dumpsys package",
                e,
                ADB_UNRESPONSIVE_HINT,
            ));
        }
    }

    let path_out = path_res
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();
    let dump_out = dump_res
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if dump_out.is_empty() && path_out.is_empty() {
        return Err(format!(
            "Package '{package}' not found on device {serial}. Is it installed?"
        ));
    }

    let raw = path_out
        .strip_prefix("package:")
        .unwrap_or(&path_out)
        .trim();
    let install_path = if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    };

    Ok(DumpedAppInfo {
        package: package.to_string(),
        install_path,
        data_dir: extract_dump_value(&dump_out, "dataDir="),
        version_name: extract_dump_value(&dump_out, "versionName="),
        version_code: extract_dump_value(&dump_out, "versionCode="),
        first_install: extract_dump_value(&dump_out, "firstInstallTime="),
        raw_dump_excerpt: dump_out.lines().take(40).collect::<Vec<_>>().join("\n"),
    })
}

pub async fn get_memory_info(
    adb: &PathBuf,
    serial: &str,
    package: &str,
) -> Result<MemoryInfo, String> {
    let output = output_with_timeout(
        Command::new(adb).args([
            "-s",
            serial,
            "shell",
            "dumpsys",
            "meminfo",
            &quote_device_shell_arg(package),
        ]),
        ADB_QUERY_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("adb dumpsys meminfo", &e, ADB_UNRESPONSIVE_HINT))?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();

    if text.trim().is_empty() || text.contains("No process found") {
        return Err(format!(
            "No memory info for '{package}' — is the app running?"
        ));
    }

    let (java_heap_pss, java_heap_rss) = extract_dump_two_values(&text, "Java Heap:");
    let (native_heap_pss, native_heap_rss) = extract_dump_two_values(&text, "Native Heap:");
    let (graphics_pss, graphics_rss) = extract_dump_two_values(&text, "Graphics:");

    Ok(MemoryInfo {
        package: package.to_string(),
        total_pss: extract_dump_value(&text, "TOTAL PSS:"),
        java_heap_pss,
        java_heap_rss,
        native_heap_pss,
        native_heap_rss,
        graphics_pss,
        graphics_rss,
        raw: text.lines().take(50).collect::<Vec<_>>().join("\n"),
    })
}

pub async fn take_screenshot(adb: &PathBuf, serial: &str) -> Result<Vec<u8>, String> {
    let output = output_with_timeout(
        Command::new(adb).args(["-s", serial, "exec-out", "screencap", "-p"]),
        ADB_SCREENSHOT_TIMEOUT,
    )
    .await
    .map_err(|e| describe_failure("adb exec-out screencap", &e, ADB_UNRESPONSIVE_HINT))?;

    if !output.status.success() || output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = if stderr.trim().is_empty() {
            "Screenshot failed (no error output from adb)".to_string()
        } else {
            stderr.to_string()
        };
        return Err(msg);
    }
    Ok(output.stdout)
}

/// Long edge, in image pixels, of a screenshot sent to an agent by default.
pub const DEFAULT_SCREENSHOT_MAX_DIMENSION: u32 = 1280;
/// Smallest accepted long edge; below this, text on screen is unreadable.
pub const MIN_SCREENSHOT_MAX_DIMENSION: u32 = 256;
/// Largest accepted long edge.
pub const MAX_SCREENSHOT_MAX_DIMENSION: u32 = 8192;
/// Largest screencap PNG accepted (encoded bytes).
pub const MAX_SCREENSHOT_PNG_BYTES: usize = 32 * 1024 * 1024;
/// Largest screen accepted (pixels); bounds the decoded RGBA buffer to 64 MiB.
pub const MAX_SCREENSHOT_PIXELS: u64 = 16 * 1024 * 1024;
/// Decoder allocation limit: the RGBA frame plus working buffers.
const SCREENSHOT_DECODE_LIMIT_BYTES: usize = 2 * MAX_SCREENSHOT_PIXELS as usize * 4;
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Where a screenshot's pixels sit on the device.
///
/// `device_*` is the capture's own size: the screen in its current rotation,
/// the same space as UI hierarchy bounds and `input tap`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenshotGeometry {
    pub device_width: u32,
    pub device_height: u32,
    pub image_width: u32,
    pub image_height: u32,
    /// Device pixels per image pixel (long edge ratio, 4 decimals).
    pub scale: f64,
}

#[derive(Debug)]
pub struct Screenshot {
    pub png: Vec<u8>,
    pub geometry: ScreenshotGeometry,
}

/// Resolve the requested long edge: `None` keeps the capture at full size.
pub fn screenshot_max_dimension(
    max_dimension: Option<u32>,
    full_size: bool,
) -> Result<Option<u32>, String> {
    if full_size {
        return match max_dimension {
            Some(_) => Err("Pass either max_dimension or full_size, not both.".to_string()),
            None => Ok(None),
        };
    }
    let value = max_dimension.unwrap_or(DEFAULT_SCREENSHOT_MAX_DIMENSION);
    if !(MIN_SCREENSHOT_MAX_DIMENSION..=MAX_SCREENSHOT_MAX_DIMENSION).contains(&value) {
        return Err(format!(
            "max_dimension must be between {MIN_SCREENSHOT_MAX_DIMENSION} and \
             {MAX_SCREENSHOT_MAX_DIMENSION} (got {value}). Pass full_size: true for the \
             original resolution."
        ));
    }
    Ok(Some(value))
}

/// Capture a screenshot and fit its long edge within `max_dimension`
/// (`None` returns the capture unchanged), reporting its geometry.
pub async fn take_screenshot_scaled(
    adb: &PathBuf,
    serial: &str,
    max_dimension: Option<u32>,
) -> Result<Screenshot, String> {
    let png = take_screenshot(adb, serial).await?;
    tokio::task::spawn_blocking(move || fit_screenshot(png, max_dimension))
        .await
        .map_err(|e| format!("Screenshot processing failed: {e}"))?
}

/// Downscale `png` so its long edge is at most `max_dimension`. A capture that
/// already fits, or `None`, is returned byte for byte.
pub fn fit_screenshot(png: Vec<u8>, max_dimension: Option<u32>) -> Result<Screenshot, String> {
    if png.len() > MAX_SCREENSHOT_PNG_BYTES {
        return Err(format!(
            "Screenshot is too large ({} bytes, max {MAX_SCREENSHOT_PNG_BYTES}).",
            png.len()
        ));
    }
    if !png.starts_with(PNG_SIGNATURE) {
        let head = String::from_utf8_lossy(&png[..png.len().min(80)]);
        return Err(format!(
            "adb screencap did not return a PNG image. Output starts with: {:?}",
            head.trim()
        ));
    }
    let (width, height) = png_size(&png)?;
    let long_edge = width.max(height);
    let max = match max_dimension {
        Some(max) if long_edge > max => max,
        _ => {
            return Ok(Screenshot {
                png,
                geometry: ScreenshotGeometry {
                    device_width: width,
                    device_height: height,
                    image_width: width,
                    image_height: height,
                    scale: 1.0,
                },
            })
        }
    };

    let factor = f64::from(long_edge) / f64::from(max);
    let image_width = scaled_edge(width, factor, max);
    let image_height = scaled_edge(height, factor, max);
    let (pixels, color, stride) = decode_png(&png)?;
    let channels = color.samples();
    let resized = box_downscale(
        &pixels,
        stride,
        (width, height),
        channels,
        (image_width, image_height),
    )?;
    let encoded = encode_png(&resized, image_width, image_height, color)?;
    Ok(Screenshot {
        png: encoded,
        geometry: ScreenshotGeometry {
            device_width: width,
            device_height: height,
            image_width,
            image_height,
            scale: (factor * 10_000.0).round() / 10_000.0,
        },
    })
}

fn invalid_png(e: impl std::fmt::Display) -> String {
    format!("Screenshot is not a valid PNG: {e}")
}

fn screenshot_decoder(png: &[u8]) -> png::Decoder<std::io::Cursor<&[u8]>> {
    let limits = png::Limits {
        bytes: SCREENSHOT_DECODE_LIMIT_BYTES,
    };
    png::Decoder::new_with_limits(std::io::Cursor::new(png), limits)
}

/// Read the image size from the header, rejecting screens too large to decode.
fn png_size(png: &[u8]) -> Result<(u32, u32), String> {
    let mut decoder = screenshot_decoder(png);
    let (width, height) = decoder.read_header_info().map_err(invalid_png)?.size();
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || pixels > MAX_SCREENSHOT_PIXELS {
        return Err(format!(
            "Screenshot size {width}x{height} is outside the supported range \
             (max {MAX_SCREENSHOT_PIXELS} pixels)."
        ));
    }
    Ok((width, height))
}

fn scaled_edge(edge: u32, factor: f64, max: u32) -> u32 {
    ((f64::from(edge) / factor).round() as u32).clamp(1, max)
}

/// Decode to 8-bit samples; returns the pixels, their color type, and the row stride.
fn decode_png(png: &[u8]) -> Result<(Vec<u8>, png::ColorType, usize), String> {
    let mut decoder = screenshot_decoder(png);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(invalid_png)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| invalid_png("frame does not fit in memory"))?;
    let mut pixels = vec![0; size];
    let info = reader.next_frame(&mut pixels).map_err(invalid_png)?;
    pixels.truncate(info.buffer_size());
    Ok((pixels, info.color_type, info.line_size))
}

/// Split `src` pixels into `dst` contiguous bins of at least one pixel each.
fn bin_edges(src: u32, dst: u32) -> Vec<usize> {
    (0..=u64::from(dst))
        .map(|i| (i * u64::from(src) / u64::from(dst)) as usize)
        .collect()
}

/// Area-average downscale: each output pixel is the mean of the source pixels
/// in its bin.
fn box_downscale(
    src: &[u8],
    stride: usize,
    (width, height): (u32, u32),
    channels: usize,
    (out_width, out_height): (u32, u32),
) -> Result<Vec<u8>, String> {
    let row_len = width as usize * channels;
    if out_width > width || out_height > height || stride < row_len {
        return Err("Screenshot downscale received inconsistent sizes.".to_string());
    }
    let rows: Vec<&[u8]> = src
        .chunks(stride)
        .map(|row| row.get(..row_len).unwrap_or_default())
        .collect();
    if rows.len() < height as usize || rows.iter().any(|r| r.len() != row_len) {
        return Err(invalid_png("decoded image is shorter than its header"));
    }
    let xs = bin_edges(width, out_width);
    let ys = bin_edges(height, out_height);
    let mut out = Vec::with_capacity(out_width as usize * out_height as usize * channels);
    let mut column_sums = vec![0u64; row_len];
    for band in ys.windows(2) {
        column_sums.fill(0);
        for row in &rows[band[0]..band[1]] {
            for (sum, &value) in column_sums.iter_mut().zip(row.iter()) {
                *sum += u64::from(value);
            }
        }
        let band_height = (band[1] - band[0]) as u64;
        for bin in xs.windows(2) {
            let area = (bin[1] - bin[0]) as u64 * band_height;
            for channel in 0..channels {
                let sum: u64 = (bin[0]..bin[1])
                    .map(|x| column_sums[x * channels + channel])
                    .sum();
                out.push(((sum + area / 2) / area) as u8);
            }
        }
    }
    Ok(out)
}

fn encode_png(
    pixels: &[u8],
    width: u32,
    height: u32,
    color: png::ColorType,
) -> Result<Vec<u8>, String> {
    let encode_error = |e: png::EncodingError| format!("Screenshot re-encoding failed: {e}");
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(encode_error)?;
    writer.write_image_data(pixels).map_err(encode_error)?;
    writer.finish().map_err(encode_error)?;
    Ok(out)
}

fn extract_dump_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find(|l| l.contains(key))
        .and_then(|l| l.split(key).nth(1))
        .map(|v| v.split_whitespace().next().unwrap_or(v).trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn extract_dump_two_values(text: &str, key: &str) -> (Option<String>, Option<String>) {
    let after = text
        .lines()
        .find(|l| l.contains(key))
        .and_then(|l| l.split(key).nth(1));
    match after {
        None => (None, None),
        Some(s) => {
            let mut parts = s.split_whitespace();
            let first = parts.next().map(str::to_owned).filter(|v| !v.is_empty());
            let second = parts.next().map(str::to_owned).filter(|v| !v.is_empty());
            (first, second)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_dump_value_finds_version() {
        let dump = "    versionName=1.2.3\n    versionCode=42\n";
        assert_eq!(
            extract_dump_value(dump, "versionName="),
            Some("1.2.3".into())
        );
        assert_eq!(extract_dump_value(dump, "versionCode="), Some("42".into()));
    }

    #[test]
    fn extract_dump_value_returns_none_for_missing() {
        assert!(extract_dump_value("some text", "nothere=").is_none());
    }

    #[test]
    fn extract_dump_two_values_extracts_pss_and_rss() {
        let meminfo = "App Summary\n   Java Heap:        0                          13832\n   Native Heap:        4                            764\n";
        let (pss, rss) = extract_dump_two_values(meminfo, "Java Heap:");
        assert_eq!(pss.as_deref(), Some("0"));
        assert_eq!(rss.as_deref(), Some("13832"));
    }

    #[test]
    fn extract_dump_two_values_returns_none_none_for_missing_key() {
        let meminfo = "App Summary\n   Java Heap:  0   1234\n";
        let (pss, rss) = extract_dump_two_values(meminfo, "Graphics:");
        assert!(pss.is_none());
        assert!(rss.is_none());
    }

    #[test]
    fn extract_dump_two_values_handles_single_column() {
        let text = "   Graphics:        8\n";
        let (pss, rss) = extract_dump_two_values(text, "Graphics:");
        assert_eq!(pss.as_deref(), Some("8"));
        assert!(rss.is_none());
    }

    #[test]
    fn extract_dump_two_values_both_non_zero() {
        let meminfo = "   Native Heap:      512                          2048\n";
        let (pss, rss) = extract_dump_two_values(meminfo, "Native Heap:");
        assert_eq!(pss.as_deref(), Some("512"));
        assert_eq!(rss.as_deref(), Some("2048"));
    }

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const RED: [u8; 4] = [220, 20, 30, 255];

    /// A white screen with one red element at device bounds `[left, top, right, bottom)`.
    fn screen_png(width: u32, height: u32, color: png::ColorType, element: [u32; 4]) -> Vec<u8> {
        let channels = color.samples();
        let mut pixels = Vec::with_capacity(width as usize * height as usize * channels);
        for y in 0..height {
            for x in 0..width {
                let inside = x >= element[0] && x < element[2] && y >= element[1] && y < element[3];
                let px = if inside { RED } else { WHITE };
                pixels.extend_from_slice(&px[..channels]);
            }
        }
        // Encoded differently from `encode_png`, so a re-encoded capture never
        // matches the original bytes by accident.
        let mut png_bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::High);
        let mut writer = encoder.write_header().expect("fixture header");
        writer.write_image_data(&pixels).expect("fixture pixels");
        writer.finish().expect("fixture finish");
        png_bytes
    }

    /// Centroid, in image coordinates, of the red element's coverage. Area
    /// averaging blends edge pixels linearly, so the green channel gives each
    /// pixel's share of the element.
    fn element_centroid(png: &[u8]) -> (f64, f64) {
        let (width, _) = png_size(png).expect("size");
        let (pixels, color, stride) = decode_png(png).expect("decode");
        let channels = color.samples();
        let (mut total, mut sum_x, mut sum_y) = (0.0, 0.0, 0.0);
        for (y, row) in pixels.chunks(stride).enumerate() {
            for x in 0..width as usize {
                let green = f64::from(row[x * channels + 1]);
                let coverage = (255.0 - green) / (255.0 - f64::from(RED[1]));
                total += coverage;
                sum_x += coverage * (x as f64 + 0.5);
                sum_y += coverage * (y as f64 + 0.5);
            }
        }
        assert!(total > 0.0, "element not visible in the screenshot");
        (sum_x / total, sum_y / total)
    }

    #[test]
    fn a_downscaled_screenshot_reports_the_size_it_was_encoded_at() {
        for (device, image, scale) in [
            ((1080, 2400), (576, 1280), 1.875),
            ((2400, 1080), (1280, 576), 1.875),
            ((1440, 3120), (591, 1280), 2.4375),
        ] {
            let png = screen_png(device.0, device.1, png::ColorType::Rgba, [10, 10, 50, 50]);

            let shot = fit_screenshot(png, Some(DEFAULT_SCREENSHOT_MAX_DIMENSION)).expect("fit");

            assert_eq!(
                shot.geometry,
                ScreenshotGeometry {
                    device_width: device.0,
                    device_height: device.1,
                    image_width: image.0,
                    image_height: image.1,
                    scale,
                }
            );
            assert_eq!(png_size(&shot.png).expect("decode output"), image);
        }
    }

    #[test]
    fn a_tap_read_from_a_downscaled_screenshot_hits_the_intended_element() {
        for element in [
            [1200, 2900, 1400, 3050],
            [40, 60, 200, 180],
            [613, 1501, 829, 1623],
        ] {
            let png = screen_png(1440, 3120, png::ColorType::Rgb, element);
            let shot = fit_screenshot(png, Some(DEFAULT_SCREENSHOT_MAX_DIMENSION)).expect("fit");
            assert!(shot.geometry.scale > 2.0, "{:?}", shot.geometry);

            let (image_x, image_y) = element_centroid(&shot.png);
            let tap_x = image_x * shot.geometry.scale;
            let tap_y = image_y * shot.geometry.scale;

            let center_x = f64::from(element[0] + element[2]) / 2.0;
            let center_y = f64::from(element[1] + element[3]) / 2.0;
            assert!(
                (tap_x - center_x).abs() <= 1.0 && (tap_y - center_y).abs() <= 1.0,
                "tap ({tap_x:.2}, {tap_y:.2}) is not within 1px of the element center \
                 ({center_x}, {center_y}); geometry {:?}",
                shot.geometry
            );
        }
    }

    #[test]
    fn max_dimension_defaults_to_1280_and_is_bounded() {
        assert_eq!(screenshot_max_dimension(None, false), Ok(Some(1280)));
        assert_eq!(
            screenshot_max_dimension(Some(MIN_SCREENSHOT_MAX_DIMENSION), false),
            Ok(Some(256))
        );
        assert_eq!(
            screenshot_max_dimension(Some(MAX_SCREENSHOT_MAX_DIMENSION), false),
            Ok(Some(8192))
        );
        for out_of_range in [
            0,
            MIN_SCREENSHOT_MAX_DIMENSION - 1,
            MAX_SCREENSHOT_MAX_DIMENSION + 1,
        ] {
            let err = screenshot_max_dimension(Some(out_of_range), false).unwrap_err();
            assert!(err.contains("between 256 and 8192"), "{err}");
        }
        assert_eq!(screenshot_max_dimension(None, true), Ok(None));
        assert!(screenshot_max_dimension(Some(1280), true).is_err());
    }

    #[test]
    fn full_size_or_a_limit_at_the_screen_size_returns_the_original_bytes() {
        let png = screen_png(1080, 2400, png::ColorType::Rgba, [10, 10, 50, 50]);
        for max_dimension in [None, Some(2400), Some(MAX_SCREENSHOT_MAX_DIMENSION)] {
            let shot = fit_screenshot(png.clone(), max_dimension).expect("fit");

            assert_eq!(shot.png, png, "{max_dimension:?}");
            assert_eq!(
                shot.geometry,
                ScreenshotGeometry {
                    device_width: 1080,
                    device_height: 2400,
                    image_width: 1080,
                    image_height: 2400,
                    scale: 1.0,
                }
            );
        }
    }

    #[test]
    fn malformed_screenshots_fail_with_a_clear_error() {
        let err = fit_screenshot(b"error: device offline\n".to_vec(), Some(1280)).unwrap_err();
        assert!(err.contains("did not return a PNG"), "{err}");
        assert!(err.contains("device offline"), "{err}");

        let mut garbage = PNG_SIGNATURE.to_vec();
        garbage.extend_from_slice(b"not a png chunk stream");
        let err = fit_screenshot(garbage, None).unwrap_err();
        assert!(err.contains("not a valid PNG"), "{err}");

        let png = screen_png(1080, 2400, png::ColorType::Rgba, [10, 10, 50, 50]);
        let truncated = png[..png.len() / 2].to_vec();
        let err = fit_screenshot(truncated, Some(1280)).unwrap_err();
        assert!(err.contains("not a valid PNG"), "{err}");
    }

    #[test]
    fn oversized_screenshots_are_refused_before_decoding() {
        let mut huge_header = Vec::new();
        {
            let encoder = png::Encoder::new(&mut huge_header, 5000, 5000);
            let _writer = encoder.write_header().expect("header");
        }
        let err = fit_screenshot(huge_header, Some(1280)).unwrap_err();
        assert!(err.contains("5000x5000"), "{err}");

        let mut too_many_bytes = PNG_SIGNATURE.to_vec();
        too_many_bytes.resize(MAX_SCREENSHOT_PNG_BYTES + 1, 0);
        let err = fit_screenshot(too_many_bytes, None).unwrap_err();
        assert!(err.contains("too large"), "{err}");
    }
}
