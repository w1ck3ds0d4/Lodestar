//! Capture mode: floating, always-on-top overlay panels for live interviews.
//!
//! Pressing Capture opens small frameless windows (answers, transcript, controls)
//! that the user drags anywhere over their video call. They are content-protected
//! (excluded from screen capture/sharing, like the main window) and skip the
//! taskbar. Each loads the same bundle; the panel reads its own window label to
//! decide which view to render.
//!
//! The main window is the source of truth: it relays state to the panels via
//! `capture-state` events and receives `capture-command` events back, over the
//! in-process event bus (no socket, unlike the phone companion).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// (window label, title, x, y, width, height). The label's `cap-` suffix selects
/// the panel view on the frontend (`cap-answers` -> the answers panel).
const PANELS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("cap-answers", "Answers", 60.0, 80.0, 380.0, 440.0),
    ("cap-transcript", "Transcript", 460.0, 80.0, 380.0, 440.0),
    ("cap-controls", "Controls", 60.0, 540.0, 380.0, 150.0),
];

/// Open (or re-focus) the overlay panels. Idempotent: an already-open panel is
/// shown and focused rather than rebuilt.
pub fn open_panels(app: &AppHandle) -> Result<(), String> {
    for &(label, title, x, y, w, h) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.show();
            let _ = win.set_focus();
            continue;
        }
        WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
            .title(title)
            .inner_size(w, h)
            .position(x, y)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .content_protected(true)
            .resizable(true)
            .build()
            .map_err(|e| format!("could not open the {title} panel: {e}"))?;
    }
    Ok(())
}

/// Close every overlay panel (a no-op for any that are not open).
pub fn close_panels(app: &AppHandle) {
    for &(label, ..) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.close();
        }
    }
}

/// Hide the panels if any is visible, otherwise show them all. Bound to a global
/// hotkey so the overlays can be dismissed instantly (e.g. when sharing a screen
/// or someone glances over) without touching the app.
pub fn toggle_visibility(app: &AppHandle) {
    let any_visible = PANELS.iter().any(|&(label, ..)| {
        app.get_webview_window(label)
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false)
    });
    for &(label, ..) in PANELS {
        if let Some(win) = app.get_webview_window(label) {
            let _ = if any_visible { win.hide() } else { win.show() };
        }
    }
}
