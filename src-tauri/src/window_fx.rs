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
/// Windows this gives the window a hidden OWNER window: an owned top-level window gets no
/// taskbar button and is excluded from Alt+Tab, while keeping the FULL native frame (min/max
/// and edge-resize included) - unlike WS_EX_TOOLWINDOW, which slims the title bar. The owner
/// MUST be created on the UI thread for Windows to honor the ownership, so the work hops to the
/// main thread. Elsewhere it falls back to Tauri's taskbar-skip.
#[tauri::command]
pub fn set_window_skip_taskbar(window: tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    set_skip_taskbar_impl(&window, skip)
}

#[cfg(windows)]
fn set_skip_taskbar_impl(window: &tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    let hwnd_val = window.hwnd().map_err(|e| format!("hwnd: {e}"))?.0 as isize;
    window
        .run_on_main_thread(move || apply_skip_taskbar(hwnd_val, skip))
        .map_err(|e| format!("run_on_main_thread: {e}"))
}

/// Lazily create (once, on the calling = UI thread) a hidden zero-size owner window. It is a
/// tool window so it never gets its own taskbar button, and it is never shown.
#[cfg(windows)]
fn stealth_owner() -> isize {
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WS_EX_TOOLWINDOW, WS_POPUP};
    static OWNER: AtomicIsize = AtomicIsize::new(0);
    let cur = OWNER.load(Ordering::Relaxed);
    if cur != 0 {
        return cur;
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
    };
    match hwnd {
        Ok(h) => {
            let v = h.0 as isize;
            OWNER.store(v, Ordering::Relaxed);
            v
        }
        Err(_) => 0,
    }
}

/// Runs on the UI thread: own (skip) or un-own the main window, cycling its visibility so the
/// taskbar and Alt+Tab re-evaluate the change.
#[cfg(windows)]
fn apply_skip_taskbar(hwnd_val: isize, skip: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SetWindowLongPtrW, ShowWindow, GWLP_HWNDPARENT, SW_HIDE, SW_SHOW,
    };
    let hwnd = HWND(hwnd_val as *mut core::ffi::c_void);
    let owner = if skip { stealth_owner() } else { 0 };
    unsafe {
        let was_visible = IsWindowVisible(hwnd).as_bool();
        if was_visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, owner);
        if was_visible {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
    }
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
