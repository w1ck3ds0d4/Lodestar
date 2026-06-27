//! Capture mode: floating, always-on-top overlay windows for live interviews.
//!
//! Pressing Capture opens a small control bar docked at the top of the screen plus
//! three frameless panels (answers, transcript, controls) that the user drags
//! anywhere over their video call. The bar shows/hides the panels and holds the
//! quick controls (listen, screenshot). Everything is content-protected (excluded
//! from screen capture/sharing) and skips the taskbar. Each window loads the same
//! bundle and reads its own label to decide which view to render.
//!
//! The main window is the source of truth: it relays state to the windows via
//! `capture-state` events and receives `capture-command` events back, over the
//! in-process event bus (no socket, unlike the phone companion).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The top control bar's window label.
const BAR: &str = "cap-bar";
const BAR_W: f64 = 232.0;
const BAR_H: f64 = 46.0;

/// (window label, title, x, y, width, height). The label's `cap-` suffix selects
/// the panel view on the frontend (`cap-answers` -> the answers panel). Compact
/// defaults clustered in a left column so the overlay does not blanket the screen;
/// every panel is freely draggable and resizable from there.
const PANELS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("cap-answers", "Answers", 24.0, 64.0, 320.0, 300.0),
    ("cap-transcript", "Transcript", 24.0, 372.0, 320.0, 200.0),
    ("cap-controls", "Controls", 24.0, 580.0, 320.0, 132.0),
    ("cap-shortcuts", "Shortcuts", 360.0, 64.0, 280.0, 250.0),
    ("cap-log", "Log", 360.0, 324.0, 300.0, 240.0),
];

/// Shared window flags for every capture overlay: borderless, always-on-top, hidden
/// from the taskbar, and (when `protected`) excluded from screen capture/sharing.
/// `protected` is normally true so the panels stay invisible to a shared/recorded
/// screen; the user can turn it off to screenshot the panels themselves.
#[allow(clippy::too_many_arguments)]
fn build_overlay(
    app: &AppHandle,
    label: &str,
    w: f64,
    h: f64,
    x: f64,
    y: f64,
    resizable: bool,
    protected: bool,
) -> Result<(), String> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(label)
        .inner_size(w, h)
        .position(x, y)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .content_protected(protected)
        .resizable(resizable)
        // Build hidden so the caller can place it on the chosen monitor before it shows
        // (otherwise it flashes at the builder position on the primary display first).
        .visible(false)
        .build()
        .map_err(|e| format!("could not open the {label} window: {e}"))?;
    Ok(())
}

/// Geometry of one capture window, captured so it can be recreated in place.
pub type PanelGeom = (
    &'static str,
    tauri::PhysicalPosition<i32>,
    tauri::PhysicalSize<u32>,
);

/// Snapshot each open capture window's geometry and destroy it, returning the geometry.
/// Recreating (rather than toggling `set_content_protected` on a live window) is the
/// only reliable way to change screen-capture protection: toggling it at runtime does
/// not lift Windows' WDA_EXCLUDEFROMCAPTURE, but setting it at window creation does.
/// The caller should wait briefly (so the destroy is processed) before reopening, then
/// call [`restore_geometry`]. Returns empty when no panels are open.
pub fn snapshot_and_destroy(app: &AppHandle) -> Vec<PanelGeom> {
    let mut geo: Vec<PanelGeom> = Vec::new();
    for label in std::iter::once(BAR).chain(PANELS.iter().map(|p| p.0)) {
        if let Some(win) = app.get_webview_window(label) {
            if let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) {
                geo.push((label, pos, size));
            }
            let _ = win.destroy();
        }
    }
    geo
}

/// Set the translucency (0.0..=1.0) of every capture overlay window together, so the
/// whole overlay can be dimmed to sit discreetly over a call.
pub fn set_panels_opacity(app: &AppHandle, opacity: f64) {
    for label in std::iter::once(BAR).chain(PANELS.iter().map(|p| p.0)) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = crate::window_fx::apply_window_opacity(&win, opacity);
        }
    }
}

/// Restore window positions/sizes captured by [`snapshot_and_destroy`] after the panels
/// have been reopened, so the user's layout survives a protection toggle.
pub fn restore_geometry(app: &AppHandle, geo: Vec<PanelGeom>) {
    for (label, pos, size) in geo {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_position(tauri::Position::Physical(pos));
            let _ = win.set_size(tauri::Size::Physical(size));
        }
    }
}

/// Resolve the chosen capture monitor to Tauri's own monitor coordinates. The picker
/// index is xcap's, so read that monitor's physical SIZE from xcap, then find Tauri's
/// matching monitor and return ITS (x, y, width, height, scale). Tauri monitor positions
/// are in the same space `set_position(Physical)` uses; xcap's positions are in the
/// virtual-screen space and don't line up on a mixed-DPI desktop, which is why a position
/// taken straight from xcap put the overlay on the wrong display.
pub fn monitor_bounds(app: &AppHandle, index: usize) -> (f64, f64, f64, f64, f64) {
    let xcap = crate::screen::bounds_physical(index).ok();
    if let Some((_, _, w, h, _)) = xcap {
        if let Some(b) = tauri_bounds(app, w as u32, h as u32) {
            return b;
        }
    }
    xcap.unwrap_or((0.0, 0.0, 1280.0, 800.0, 1.0))
}

/// Tauri's monitor whose physical size matches `(w, h)` -> its (x, y, width, height,
/// scale). Falls back to the first monitor.
fn tauri_bounds(app: &AppHandle, w: u32, h: u32) -> Option<(f64, f64, f64, f64, f64)> {
    let win = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next())?;
    let mons = win.available_monitors().ok()?;
    let m = mons
        .iter()
        .find(|m| m.size().width == w && m.size().height == h)
        .or_else(|| mons.first())?;
    let (p, sz) = (m.position(), m.size());
    Some((
        p.x as f64,
        p.y as f64,
        sz.width as f64,
        sz.height as f64,
        m.scale_factor(),
    ))
}

/// Open (or re-focus) the control bar and overlay panels. Idempotent. `protected` sets
/// whether the windows are hidden from screen capture (saved default true). `mon` is the
/// chosen monitor's physical bounds + scale in Tauri's coordinate space (see
/// [`monitor_bounds`]). New windows are built hidden, positioned on that monitor via
/// `Position::Physical`, then shown - so they never flash on, or stick to, the wrong one.
pub fn open_panels(
    app: &AppHandle,
    protected: bool,
    mon: (f64, f64, f64, f64, f64),
) -> Result<(), String> {
    let (mx, my, mw, mh, scale) = mon;
    // Physical top-left for a window of LOGICAL size (lw,lh) at LOGICAL offset (lx,ly)
    // within the monitor, clamped to the monitor's physical box (so nothing opens
    // off-screen on a smaller display).
    let place = |lx: f64, ly: f64, lw: f64, lh: f64| -> tauri::PhysicalPosition<i32> {
        let (pw, ph) = (lw * scale, lh * scale);
        let px = (mx + lx * scale).min(mx + (mw - pw).max(0.0)).max(mx);
        let py = (my + ly * scale).min(my + (mh - ph).max(0.0)).max(my);
        tauri::PhysicalPosition {
            x: px.round() as i32,
            y: py.round() as i32,
        }
    };
    // Place a freshly-built (hidden) window on the chosen monitor, then reveal it.
    let setup = |label: &str, pos: tauri::PhysicalPosition<i32>| {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_position(tauri::Position::Physical(pos));
            let _ = win.show();
            let _ = win.set_focus();
        }
    };
    // Control bar: centered horizontally near the top of the chosen monitor.
    let bar_lx = (mw / scale - BAR_W) / 2.0;
    if let Some(win) = app.get_webview_window(BAR) {
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        build_overlay(app, BAR, BAR_W, BAR_H, bar_lx, 8.0, false, protected)?;
        setup(BAR, place(bar_lx, 8.0, BAR_W, BAR_H));
    }

    for &(label, _title, x, y, w, h) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.show();
            let _ = win.set_focus();
            continue;
        }
        build_overlay(app, label, w, h, x, y, true, protected)?;
        setup(label, place(x, y, w, h));
    }
    Ok(())
}

/// Close the control bar and every overlay panel (a no-op for any not open).
pub fn close_panels(app: &AppHandle) {
    for label in PANELS.iter().map(|p| p.0).chain(std::iter::once(BAR)) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.close();
        }
    }
}

/// Hide the panels if any is visible, otherwise show them all; the control bar stays
/// put so the panels can be brought back. Returns whether the panels are now visible.
/// Bound to a global hotkey and the bar's show/hide button.
pub fn toggle_visibility(app: &AppHandle) -> bool {
    let any_visible = PANELS.iter().any(|&(label, ..)| {
        app.get_webview_window(label)
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false)
    });
    let show = !any_visible;
    for &(label, ..) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = if show { win.show() } else { win.hide() };
        }
    }
    show
}

/// Reposition one overlay panel - or "all" of them as a group - from the phone remote.
/// `snap` (tl/tr/bl/br/center) moves the panel flush to that anchor of its monitor; `nudge`
/// (up/down/left/right) shifts it a step. With "all", the panels move together so their
/// relative layout is preserved (snap aligns their combined bounding box to the anchor).
pub fn move_panel(
    app: &AppHandle,
    panel: &str,
    snap: Option<&str>,
    nudge: Option<&str>,
) -> Result<(), String> {
    // Resolve the target panel label(s).
    let labels: Vec<&str> = if panel == "all" {
        PANELS.iter().map(|p| p.0).collect()
    } else if PANELS.iter().any(|p| p.0 == panel) {
        vec![panel]
    } else {
        return Err(format!("unknown panel: {panel}"));
    };

    // Collect each open, visible target's window + current physical geometry.
    type WinGeom = (
        tauri::WebviewWindow,
        tauri::PhysicalPosition<i32>,
        tauri::PhysicalSize<u32>,
    );
    let mut wins: Vec<WinGeom> = Vec::new();
    for label in &labels {
        if let Some(w) = app.get_webview_window(label) {
            if !w.is_visible().unwrap_or(false) {
                continue;
            }
            let pos = w.outer_position().map_err(|e| e.to_string())?;
            let size = w.outer_size().map_err(|e| e.to_string())?;
            wins.push((w, pos, size));
        }
    }
    if wins.is_empty() {
        return Ok(()); // nothing open/visible to move
    }

    let mon = wins[0].0.current_monitor().ok().flatten();
    let scale = mon.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);

    let apply = |off_x: i32, off_y: i32| {
        for (w, pos, _) in &wins {
            let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: pos.x + off_x,
                y: pos.y + off_y,
            }));
        }
    };

    if let Some(dir) = nudge {
        let step = (54.0 * scale).round() as i32;
        let (dx, dy) = match dir {
            "left" => (-step, 0),
            "right" => (step, 0),
            "up" => (0, -step),
            "down" => (0, step),
            _ => return Err(format!("unknown nudge: {dir}")),
        };
        apply(dx, dy);
        return Ok(());
    }

    if let Some(corner) = snap {
        let m = mon.ok_or("no monitor for the panel")?;
        let (mx, my) = (m.position().x, m.position().y);
        let (mw, mh) = (m.size().width as i32, m.size().height as i32);
        let margin = (14.0 * scale).round() as i32;
        // Combined bounding box of the targets (a single panel's box is itself).
        let min_x = wins.iter().map(|(_, p, _)| p.x).min().unwrap();
        let min_y = wins.iter().map(|(_, p, _)| p.y).min().unwrap();
        let max_x = wins
            .iter()
            .map(|(_, p, s)| p.x + s.width as i32)
            .max()
            .unwrap();
        let max_y = wins
            .iter()
            .map(|(_, p, s)| p.y + s.height as i32)
            .max()
            .unwrap();
        let (bw, bh) = (max_x - min_x, max_y - min_y);
        let left = mx + margin;
        let right = mx + mw - margin - bw;
        let cx = mx + (mw - bw) / 2;
        let top = my + margin;
        let bottom = my + mh - margin - bh;
        let cy = my + (mh - bh) / 2;
        let (tx, ty) = match corner {
            "tl" => (left, top),
            "tr" => (right, top),
            "bl" => (left, bottom),
            "br" => (right, bottom),
            "center" => (cx, cy),
            _ => return Err(format!("unknown snap: {corner}")),
        };
        apply(tx - min_x, ty - min_y);
    }
    Ok(())
}
