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
/// the panel view on the frontend (`cap-answers` -> the answers panel).
const PANELS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("cap-answers", "Answers", 60.0, 80.0, 380.0, 440.0),
    ("cap-transcript", "Transcript", 460.0, 80.0, 380.0, 440.0),
    ("cap-controls", "Controls", 60.0, 540.0, 380.0, 150.0),
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

/// Toggle screen-capture protection on every open capture window (bar + panels) at
/// runtime. With `protected` false the panels become visible to screenshots and screen
/// shares; with it true they are hidden again. Best-effort per window.
pub fn set_panels_protected(app: &AppHandle, protected: bool) {
    for label in std::iter::once(BAR).chain(PANELS.iter().map(|p| p.0)) {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_content_protected(protected);
        }
    }
}

/// Open (or re-focus) the control bar and overlay panels. Idempotent. `protected`
/// sets whether the windows are hidden from screen capture (the saved default is true).
pub fn open_panels(app: &AppHandle, protected: bool) -> Result<(), String> {
    // Control bar: centered horizontally near the top of the primary monitor.
    if let Some(win) = app.get_webview_window(BAR) {
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        let center_x = app
            .get_webview_window("main")
            .and_then(|w| w.primary_monitor().ok().flatten())
            .map(|m| (((m.size().width as f64 / m.scale_factor()) - BAR_W) / 2.0).max(0.0))
            .unwrap_or(540.0);
        build_overlay(app, BAR, BAR_W, BAR_H, center_x, 8.0, false, protected)?;
    }

    for &(label, _title, x, y, w, h) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.show();
            let _ = win.set_focus();
            continue;
        }
        build_overlay(app, label, w, h, x, y, true, protected)?;
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
