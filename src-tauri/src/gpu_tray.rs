//! Live GPU-temperature readout baked into the system-tray icon.
//!
//! Reads the NVIDIA GPU temperature via NVML (`nvml.dll`, shipped with the driver)
//! every couple of seconds and paints it as a small colour-coded number into the
//! tray icon, so the current temp is glanceable without opening anything. Degrades
//! gracefully: with no NVIDIA GPU / NVML unavailable, the tray keeps its default
//! icon and this thread simply exits. The Intel Arc iGPU has no clean pure-Rust
//! temperature path, so we monitor the discrete NVIDIA GPU only.

use std::time::Duration;

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use tauri::image::Image;

/// How often to re-read the temperature.
const POLL: Duration = Duration::from_secs(2);

const W: usize = 32;
const H: usize = 32;

/// A 5x7 pixel font for the digits 0-9. Each entry is 7 rows; within a row the low
/// 5 bits are the pixels, bit 4 (`0b10000`) leftmost.
const FONT: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ], // 0
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ], // 1
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ], // 2
    [
        0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
    ], // 3
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ], // 4
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ], // 5
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ], // 6
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ], // 7
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ], // 8
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ], // 9
];

fn set_pixel(lit: &mut [bool], x: usize, y: usize) {
    if x < W && y < H {
        lit[y * W + x] = true;
    }
}

/// Rasterise one digit at `scale` into the `lit` coverage grid, top-left at (ox, oy).
fn draw_digit(lit: &mut [bool], d: u8, ox: usize, oy: usize, scale: usize) {
    let glyph = &FONT[(d % 10) as usize];
    for (ry, row) in glyph.iter().enumerate() {
        for rx in 0..5 {
            if (row >> (4 - rx)) & 1 == 1 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        set_pixel(lit, ox + rx * scale + sx, oy + ry * scale + sy);
                    }
                }
            }
        }
    }
}

fn has_lit_neighbour(lit: &[bool], x: usize, y: usize) -> bool {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx >= 0
                && nx < W as i32
                && ny >= 0
                && ny < H as i32
                && lit[ny as usize * W + nx as usize]
            {
                return true;
            }
        }
    }
    false
}

/// Render `temp` (degrees C) as a colour-coded 32x32 RGBA tray icon: purple below
/// 60, amber 60-80, red above 80, with a 1px dark outline so the digits stay
/// legible on both light and dark taskbars.
fn render_temp_icon(temp: u32) -> Image<'static> {
    let mut lit = vec![false; W * H];

    let scale = 3;
    let glyph_w = 5 * scale;
    let glyph_h = 7 * scale;
    let gap = 2;
    let y = (H - glyph_h) / 2;

    // GPU temps are two digits in practice; clamp so we never overflow the icon.
    let shown = temp.min(99);
    if shown < 10 {
        draw_digit(&mut lit, shown as u8, (W - glyph_w) / 2, y, scale);
    } else {
        let total = glyph_w * 2 + gap;
        let x = (W - total) / 2;
        draw_digit(&mut lit, (shown / 10) as u8, x, y, scale);
        draw_digit(&mut lit, (shown % 10) as u8, x + glyph_w + gap, y, scale);
    }

    let (r, g, b) = if temp < 60 {
        (180u8, 130, 255) // purple
    } else if temp <= 80 {
        (250, 190, 60) // amber
    } else {
        (240, 80, 80) // red
    };

    let mut rgba = vec![0u8; W * H * 4];
    for py in 0..H {
        for px in 0..W {
            let i = (py * W + px) * 4;
            if lit[py * W + px] {
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            } else if has_lit_neighbour(&lit, px, py) {
                rgba[i + 3] = 220; // black (0,0,0) outline, mostly opaque
            }
        }
    }

    Image::new_owned(rgba, W as u32, H as u32)
}

/// Spawn the background poller. Reads the NVIDIA GPU temperature every couple of
/// seconds and repaints the tray icon + tooltip. No-op (the thread exits) when NVML
/// or an NVIDIA GPU is unavailable, leaving the default tray icon in place.
pub fn spawn_updater(tray: tauri::tray::TrayIcon) {
    let _ = std::thread::Builder::new()
        .name("gpu-tray".into())
        .spawn(move || {
            let nvml = match Nvml::init() {
                Ok(n) => n,
                Err(e) => {
                    tracing::info!("GPU tray disabled (NVML unavailable): {e}");
                    return;
                }
            };
            // Cache the device name once for the tooltip.
            let name = nvml
                .device_by_index(0)
                .and_then(|d| d.name())
                .unwrap_or_else(|_| "GPU".to_string());

            loop {
                let reading = nvml
                    .device_by_index(0)
                    .and_then(|d| d.temperature(TemperatureSensor::Gpu));

                match reading {
                    Ok(temp) => {
                        let _ = tray.set_icon(Some(render_temp_icon(temp)));
                        let tip = format!("{name} - {temp}\u{00b0}C");
                        let _ = tray.set_tooltip(Some(tip.as_str()));
                    }
                    Err(e) => {
                        tracing::debug!("GPU temp read failed: {e}");
                        let _ = tray.set_tooltip(Some("GPU temp unavailable"));
                    }
                }

                std::thread::sleep(POLL);
            }
        });
}
