//! Screen capture for the visual assistant: enumerate monitors and grab a PNG of
//! a chosen one. Uses xcap (pure Rust, Windows API under the hood) - no external
//! tools. The captured image is sent to Claude's vision to find and answer any
//! question/problem shown on screen.

use std::io::Cursor;

use anyhow::{anyhow, Result};
use xcap::image::imageops::FilterType;
use xcap::image::{DynamicImage, ImageFormat};
use xcap::Monitor;

/// A captured frame: PNG bytes plus a perceptual content hash for change detection.
pub struct Capture {
    pub png: Vec<u8>,
    /// 64-bit average-hash (aHash) of the frame. Compare two with Hamming distance
    /// (count of differing bits): a small distance means perceptually the same screen.
    pub hash: u64,
}

/// Perceptual difference-hash (dHash): downscale to 9x8 grayscale and, for each of the
/// 8 rows, set one bit per adjacent column pair where the left pixel is brighter than
/// the right (8 comparisons x 8 rows = 64 bits). Compared with an average-hash, a
/// gradient hash reacts to the text/code that changes between two exercises while a
/// blinking caret, a ticking clock, or a moved cursor barely shifts the row gradients.
/// So the watcher stops re-answering the same screen without silently dropping a
/// genuinely new question that happens to share the same page chrome (which an aHash,
/// dominated by the unchanging background, would collide with).
fn difference_hash(img: &DynamicImage) -> u64 {
    let thumb = img.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let mut hash = 0u64;
    let mut bit = 0u32;
    for row in 0..8u32 {
        for col in 0..8u32 {
            let left = thumb.get_pixel(col, row)[0];
            let right = thumb.get_pixel(col + 1, row)[0];
            if left > right {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// Number of connected monitors (each is addressed by its index in this list).
pub fn count() -> usize {
    Monitor::all().map(|m| m.len()).unwrap_or(0)
}

/// Capture monitor `index` as PNG, with a perceptual hash of its content so callers
/// can skip re-answering a screen that has not meaningfully changed.
pub fn capture(index: usize) -> Result<Capture> {
    let monitors = Monitor::all().map_err(|e| anyhow!("Could not list monitors: {e}"))?;
    let monitor = monitors
        .get(index)
        .ok_or_else(|| anyhow!("That monitor is no longer connected."))?;
    let img = monitor
        .capture_image()
        .map_err(|e| anyhow!("Could not capture the screen: {e}"))?;

    let dynimg = DynamicImage::ImageRgba8(img);
    let hash = difference_hash(&dynimg);

    let mut png = Vec::new();
    dynimg
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|e| anyhow!("Could not encode the screenshot: {e}"))?;
    Ok(Capture { png, hash })
}

/// Physical-pixel bounds of monitor `index` plus its scale factor:
/// `(x, y, width, height, scale)`. The capture overlay is positioned in PHYSICAL
/// coordinates (the global virtual-desktop space, which is what xcap reports and what
/// Tauri's `Position::Physical` uses) so it lands on the chosen display regardless of
/// per-monitor DPI - logical positioning was converted with the primary monitor's scale
/// and so landed on the wrong screen in a mixed-DPI / multi-monitor setup.
pub fn bounds_physical(index: usize) -> Result<(f64, f64, f64, f64, f64)> {
    let monitors = Monitor::all().map_err(|e| anyhow!("Could not list monitors: {e}"))?;
    let m = monitors
        .get(index)
        .ok_or_else(|| anyhow!("That monitor is no longer connected."))?;
    Ok((
        m.x() as f64,
        m.y() as f64,
        m.width() as f64,
        m.height() as f64,
        (m.scale_factor() as f64).max(0.1),
    ))
}
