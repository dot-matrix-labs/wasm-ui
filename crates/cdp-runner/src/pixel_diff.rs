// Visual regression testing via pixel-level image comparison.
//
// Workflow:
//   1. Run tests normally: each screenshot is compared against its baseline
//      in `tests/screenshots/baseline/`. If a baseline is missing, the test
//      fails with a message explaining how to generate it.
//
//   2. Generate/update baselines:
//        CDP_UPDATE_BASELINES=1 cargo test -p cdp-runner -- --nocapture
//      This copies captured screenshots into the baseline directory.
//
//   3. On mismatch, three images are written to the screenshot output dir:
//        <name>_actual.png   — what the test produced
//        <name>_baseline.png — the committed reference
//        <name>_diff.png     — changed pixels highlighted in magenta
//
// Environment variables:
//   CDP_BASELINE_DIR     — baseline directory (default: tests/screenshots/baseline)
//   CDP_UPDATE_BASELINES — set to "1" to overwrite baselines with current captures
//   CDP_DIFF_THRESHOLD   — max fraction of differing pixels (default: 0.001 = 0.1%)

use std::path::{Path, PathBuf};

/// Per-channel tolerance for perceptual comparison. Pixels whose R, G, B, and
/// A channels all differ by at most this value are considered identical.
const CHANNEL_TOLERANCE: u8 = 3;

/// Result of comparing two images.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Total number of pixels in the image.
    pub total_pixels: u64,
    /// Number of pixels that differ beyond the perceptual tolerance.
    pub diff_pixels: u64,
    /// Fraction of differing pixels (diff_pixels / total_pixels).
    pub diff_ratio: f64,
    /// Path to the generated diff image (if any).
    pub diff_image_path: Option<PathBuf>,
}

/// Returns the baseline directory, resolved relative to the crate manifest dir.
pub fn baseline_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CDP_BASELINE_DIR") {
        return PathBuf::from(dir);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("..");
    dir.push("..");
    dir.push("tests");
    dir.push("screenshots");
    dir.push("baseline");
    // Normalize the path to remove the ".." components.
    match dir.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => dir,
    }
}

/// Returns true if the user has requested baseline updates.
pub fn should_update_baselines() -> bool {
    std::env::var("CDP_UPDATE_BASELINES")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Returns the diff threshold (max fraction of differing pixels before failure).
pub fn diff_threshold() -> f64 {
    std::env::var("CDP_DIFF_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.001)
}

/// Update the baseline image for the given screenshot name.
///
/// Copies `actual_path` to `baseline_dir/<name>.png`.
pub fn update_baseline(actual_path: &Path, name: &str) -> Result<(), String> {
    let dir = baseline_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create baseline dir: {}", e))?;
    let dest = dir.join(format!("{}.png", name));
    std::fs::copy(actual_path, &dest).map_err(|e| format!("copy baseline: {}", e))?;
    println!("[baseline] updated {}", dest.display());
    Ok(())
}

/// Compare a captured screenshot against its baseline.
///
/// On mismatch (exceeding `diff_threshold()`), writes `<name>_baseline.png`,
/// `<name>_actual.png`, and `<name>_diff.png` to `output_dir` for debugging,
/// then returns an error.
///
/// Returns `Ok(None)` when no baseline exists (with a warning printed).
/// Returns `Ok(Some(result))` on successful comparison.
pub fn compare_screenshot(
    actual_png_bytes: &[u8],
    name: &str,
    output_dir: &Path,
) -> Result<Option<DiffResult>, String> {
    let baseline_path = baseline_dir().join(format!("{}.png", name));
    if !baseline_path.exists() {
        println!(
            "[visual-regression] WARNING: no baseline for '{}' at {}",
            name,
            baseline_path.display()
        );
        println!(
            "[visual-regression] Run with CDP_UPDATE_BASELINES=1 to generate baselines."
        );
        return Ok(None);
    }

    // Load baseline image.
    let baseline_img = image::open(&baseline_path)
        .map_err(|e| format!("load baseline '{}': {}", baseline_path.display(), e))?
        .to_rgba8();

    // Load actual image from raw PNG bytes.
    let actual_img = image::load_from_memory_with_format(actual_png_bytes, image::ImageFormat::Png)
        .map_err(|e| format!("decode actual screenshot '{}': {}", name, e))?
        .to_rgba8();

    let (bw, bh) = (baseline_img.width(), baseline_img.height());
    let (aw, ah) = (actual_img.width(), actual_img.height());

    if bw != aw || bh != ah {
        // Size mismatch: write debug images and fail.
        let actual_debug = output_dir.join(format!("{}_actual.png", name));
        let baseline_debug = output_dir.join(format!("{}_baseline.png", name));
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("create output dir: {}", e))?;
        std::fs::write(&actual_debug, actual_png_bytes)
            .map_err(|e| format!("write actual: {}", e))?;
        std::fs::copy(&baseline_path, &baseline_debug)
            .map_err(|e| format!("copy baseline: {}", e))?;
        return Err(format!(
            "screenshot '{}' size mismatch: baseline {}x{}, actual {}x{}",
            name, bw, bh, aw, ah
        ));
    }

    // Pixel-by-pixel comparison with perceptual tolerance.
    let total_pixels = (bw as u64) * (bh as u64);
    let mut diff_pixels: u64 = 0;
    let mut diff_img = image::RgbaImage::new(bw, bh);

    for y in 0..bh {
        for x in 0..bw {
            let bp = baseline_img.get_pixel(x, y);
            let ap = actual_img.get_pixel(x, y);
            if pixels_differ(bp, ap) {
                diff_pixels += 1;
                // Highlight differing pixel in magenta.
                diff_img.put_pixel(x, y, image::Rgba([255, 0, 255, 255]));
            } else {
                // Dim copy of the actual pixel for context.
                diff_img.put_pixel(
                    x,
                    y,
                    image::Rgba([ap[0] / 3, ap[1] / 3, ap[2] / 3, 255]),
                );
            }
        }
    }

    let diff_ratio = if total_pixels > 0 {
        diff_pixels as f64 / total_pixels as f64
    } else {
        0.0
    };

    let threshold = diff_threshold();

    if diff_ratio > threshold {
        // Write debug artifacts.
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("create output dir: {}", e))?;

        let actual_debug = output_dir.join(format!("{}_actual.png", name));
        let baseline_debug = output_dir.join(format!("{}_baseline.png", name));
        let diff_debug = output_dir.join(format!("{}_diff.png", name));

        std::fs::write(&actual_debug, actual_png_bytes)
            .map_err(|e| format!("write actual: {}", e))?;
        std::fs::copy(&baseline_path, &baseline_debug)
            .map_err(|e| format!("copy baseline: {}", e))?;
        diff_img
            .save(&diff_debug)
            .map_err(|e| format!("write diff: {}", e))?;

        println!("[visual-regression] FAIL '{}': {:.4}% pixels differ (threshold: {:.4}%)",
            name, diff_ratio * 100.0, threshold * 100.0);
        println!("[visual-regression]   baseline: {}", baseline_debug.display());
        println!("[visual-regression]   actual:   {}", actual_debug.display());
        println!("[visual-regression]   diff:     {}", diff_debug.display());

        return Err(format!(
            "visual regression in '{}': {:.4}% of pixels differ ({} / {}), threshold {:.4}%",
            name,
            diff_ratio * 100.0,
            diff_pixels,
            total_pixels,
            threshold * 100.0,
        ));
    }

    let diff_image_path = if diff_pixels > 0 {
        // Even if below threshold, save the diff for informational purposes.
        let diff_debug = output_dir.join(format!("{}_diff.png", name));
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("create output dir: {}", e))?;
        diff_img
            .save(&diff_debug)
            .map_err(|e| format!("write diff: {}", e))?;
        Some(diff_debug)
    } else {
        None
    };

    println!(
        "[visual-regression] PASS '{}': {:.4}% pixels differ ({} / {})",
        name,
        diff_ratio * 100.0,
        diff_pixels,
        total_pixels,
    );

    Ok(Some(DiffResult {
        total_pixels,
        diff_pixels,
        diff_ratio,
        diff_image_path,
    }))
}

/// Returns true if two RGBA pixels differ beyond the perceptual tolerance.
fn pixels_differ(a: &image::Rgba<u8>, b: &image::Rgba<u8>) -> bool {
    // Check if any channel exceeds the tolerance.
    for i in 0..4 {
        let diff = (a[i] as i16 - b[i] as i16).unsigned_abs() as u8;
        if diff > CHANNEL_TOLERANCE {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_pixels_do_not_differ() {
        let a = image::Rgba([100, 150, 200, 255]);
        assert!(!pixels_differ(&a, &a));
    }

    #[test]
    fn within_tolerance_does_not_differ() {
        let a = image::Rgba([100, 150, 200, 255]);
        let b = image::Rgba([103, 147, 200, 255]); // within 3
        assert!(!pixels_differ(&a, &b));
    }

    #[test]
    fn beyond_tolerance_differs() {
        let a = image::Rgba([100, 150, 200, 255]);
        let b = image::Rgba([104, 150, 200, 255]); // 4 > 3
        assert!(pixels_differ(&a, &b));
    }

    #[test]
    fn alpha_channel_matters() {
        let a = image::Rgba([100, 150, 200, 255]);
        let b = image::Rgba([100, 150, 200, 250]); // alpha diff = 5
        assert!(pixels_differ(&a, &b));
    }
}
