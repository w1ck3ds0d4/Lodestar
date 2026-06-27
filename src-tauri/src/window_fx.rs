//! Translucent-overlay + always-on-top window controls, so the assistant can sit
//! discreetly over a call. Commands return `Result<(), String>` to match the rest of
//! JobRadar's Tauri commands. Opacity is Windows-only; a no-op success elsewhere.

/// Make the webview window translucent. `opacity` is 0.0..=1.0 (1.0 = fully opaque).
#[tauri::command]
pub fn set_window_opacity(window: tauri::WebviewWindow, opacity: f64) -> Result<(), String> {
    apply_window_opacity(&window, opacity)
}

/// Apply `opacity` (0.0..=1.0) to a specific window. Reusable from other modules (e.g.
/// the capture overlay sets every panel's opacity together).
pub(crate) fn apply_window_opacity(
    window: &tauri::WebviewWindow,
    opacity: f64,
) -> Result<(), String> {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    set_opacity_impl(window, alpha)
}

/// Keep the window above other windows (overlay mode). Cross-platform via Tauri.
#[tauri::command]
pub fn set_window_always_on_top(window: tauri::WebviewWindow, on_top: bool) -> Result<(), String> {
    window
        .set_always_on_top(on_top)
        .map_err(|e| format!("set_always_on_top: {e}"))
}

/// Remove (or restore) the window from the taskbar AND the Alt+Tab switcher, so the app can
/// sit fully discreetly - reachable only via the tray icon or the Alt+Shift+H hotkey. On
/// Windows this flips the WS_EX_TOOLWINDOW style (a tool window shows in neither the taskbar
/// nor Alt+Tab); elsewhere it falls back to Tauri's taskbar-skip.
#[tauri::command]
pub fn set_window_skip_taskbar(window: tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    set_skip_taskbar_impl(&window, skip)
}

#[cfg(windows)]
fn set_skip_taskbar_impl(window: &tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_HIDE, SW_SHOW,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| format!("hwnd: {e}"))?.0);
    let was_visible = window.is_visible().unwrap_or(true);
    unsafe {
        // Alt+Tab only re-reads WS_EX_TOOLWINDOW after the window is hidden and shown again,
        // so cycle visibility around the style change (skipped when already hidden).
        if was_visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if skip {
            ex = (ex | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
        } else {
            ex = (ex & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        if was_visible {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_skip_taskbar_impl(window: &tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    window
        .set_skip_taskbar(skip)
        .map_err(|e| format!("set_skip_taskbar: {e}"))
}

#[cfg(windows)]
fn set_opacity_impl(window: &tauri::WebviewWindow, alpha: u8) -> Result<(), String> {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA,
        WS_EX_LAYERED,
    };
    // Reconstruct an HWND in our own windows-rs version from the raw pointer, so the
    // version Tauri uses internally is irrelevant.
    let hwnd = HWND(window.hwnd().map_err(|e| format!("hwnd: {e}"))?.0);
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_LAYERED.0 as isize));
        SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)
            .map_err(|e| format!("SetLayeredWindowAttributes: {e}"))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_opacity_impl(_window: &tauri::WebviewWindow, _alpha: u8) -> Result<(), String> {
    Ok(())
}
