//! Screen capture for the visual assistant: enumerate monitors and grab a PNG of
//! a chosen one. Uses xcap (pure Rust, Windows API under the hood) - no external
//! tools. The captured image is sent to Claude's vision to find and answer any
//! question/problem shown on screen.

use std::hash::{Hash, Hasher};
use std::io::Cursor;

use anyhow::{anyhow, Result};
use xcap::image::{DynamicImage, ImageFormat};
use xcap::Monitor;

/// A captured frame: PNG bytes plus a cheap content hash for change detection.
pub struct Capture {
    pub png: Vec<u8>,
    pub hash: u64,
}

/// Number of connected monitors (each is addressed by its index in this list).
pub fn count() -> usize {
    Monitor::all().map(|m| m.len()).unwrap_or(0)
}

/// Capture monitor `index` as PNG, with a hash of its raw pixels so callers can
/// skip re-answering an unchanged screen.
pub fn capture(index: usize) -> Result<Capture> {
    let monitors = Monitor::all().map_err(|e| anyhow!("Could not list monitors: {e}"))?;
    let monitor = monitors
        .get(index)
        .ok_or_else(|| anyhow!("That monitor is no longer connected."))?;
    let img = monitor
        .capture_image()
        .map_err(|e| anyhow!("Could not capture the screen: {e}"))?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    img.as_raw().hash(&mut hasher);
    let hash = hasher.finish();

    let mut png = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|e| anyhow!("Could not encode the screenshot: {e}"))?;
    Ok(Capture { png, hash })
}
