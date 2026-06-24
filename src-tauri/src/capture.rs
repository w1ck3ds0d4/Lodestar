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
];

/// Shared window flags for every capture overlay: borderless, always-on-top, hidden
/// from the taskbar, and (when `protected`) excluded from screen capture/sharing.
/// `protected` is normally true so the panels stay invisible to a shared/recorded
/// screen; the user can turn it off to screenshot the panels themselves.
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
        .build()
        .map_err(|e| format!("could not open the {label} window: {e}"))?;
    Ok(())
}

/// Geometry of one capture window, captured so it can be recreated in place.
pub type PanelGeom = (&'static str, tauri::PhysicalPosition<i32>, tauri::PhysicalSize<u32>);

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

/// Open (or re-focus) the control bar and overlay panels. Idempotent. `protected`
/// sets whether the windows are hidden from screen capture (the saved default is true).
/// `bounds` is the chosen monitor's logical (x, y, width, height): every window is
/// placed relative to it, so the overlay opens on the display the user picked rather
/// than always on the primary one.
pub fn open_panels(app: &AppHandle, protected: bool, bounds: (f64, f64, f64, f64)) -> Result<(), String> {
    let (ox, oy, ow, oh) = bounds;
    // Keep a window's top-left corner inside the chosen monitor. The default panel
    // layout is sized for ~1280x800; on a smaller or high-DPI (logically smaller)
    // display the lower/right panels would otherwise open off-screen. Clamp to the
    // monitor box, preferring the top-left when a window is larger than the display.
    let place = |start: f64, len: f64, origin: f64, span: f64| {
        (origin + start).min(origin + (span - len).max(0.0)).max(origin)
    };
    // Control bar: centered horizontally near the top of the chosen monitor.
    if let Some(win) = app.get_webview_window(BAR) {
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        let center_x = place((ow - BAR_W) / 2.0, BAR_W, ox, ow);
        let bar_y = place(8.0, BAR_H, oy, oh);
        build_overlay(app, BAR, BAR_W, BAR_H, center_x, bar_y, false, protected)?;
    }

    for &(label, _title, x, y, w, h) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.show();
            let _ = win.set_focus();
            continue;
        }
        let px = place(x, w, ox, ow);
        let py = place(y, h, oy, oh);
        build_overlay(app, label, w, h, px, py, true, protected)?;
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
