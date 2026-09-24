//! Translucent-overlay + always-on-top window controls, so the assistant can sit
//! discreetly over a call. Commands return `Result<(), String>` to match the rest of
//! Lodestar's Tauri commands. Opacity is Windows-only; a no-op success elsewhere.

/// Overlay ("always on top") state. `WS_EX_TOPMOST` alone is not enough to be above
/// EVERYTHING: the topmost band has its own z-order, so any other app that raises a topmost
/// window (another overlay, a UAC-adjacent prompt, a media/meeting HUD) lands above us and
/// stays there. The stealth ex-style's hide/show cycle can drop us down the band too. While
/// this flag is set, a keeper thread re-asserts the FRONT of the topmost band, so "on" really
/// means on top of everything.
#[cfg(windows)]
static ALWAYS_ON_TOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Raw HWND of the window overlay mode applies to (0 = not yet known). Stored as an isize so
/// it can cross the thread boundary without needing `HWND: Send`.
#[cfg(windows)]
static TOP_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(windows)]
static KEEPER_STARTED: std::sync::Once = std::sync::Once::new();

/// Raise `hwnd` to the FRONT of the topmost band without activating it (so focus stays on
/// whatever the user is actually typing into). Cheap enough to run on a short interval.
#[cfg(windows)]
fn raise_topmost(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// Re-assert the top of the topmost band if overlay mode is on. Call after anything that can
/// restack the window (notably the stealth hide/show cycle).
#[cfg(windows)]
pub(crate) fn reassert_topmost_if_on() {
    use std::sync::atomic::Ordering;
    if !ALWAYS_ON_TOP.load(Ordering::Relaxed) {
        return;
    }
    let raw = TOP_HWND.load(Ordering::Relaxed);
    if raw != 0 {
        raise_topmost(windows::Win32::Foundation::HWND(
            raw as *mut core::ffi::c_void,
        ));
    }
}

/// Start the single keeper thread (idempotent). It idles on an atomic when overlay mode is
/// off, so the cost when the user never enables it is one sleeping thread.
#[cfg(windows)]
fn start_topmost_keeper() {
    KEEPER_STARTED.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_millis(700));
            reassert_topmost_if_on();
        });
    });
}

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

/// Keep the window above other windows (overlay mode).
///
/// On Windows we set the topmost band directly with `SetWindowPos` instead of tao's
/// `set_always_on_top`, because tao rebuilds the window's ex-style from its own flags on that call -
/// re-adding `WS_EX_APPWINDOW` and dropping `WS_EX_TOOLWINDOW` - which would pop the taskbar button
/// / Alt+Tab entry back and bump the process into Task Manager's "Apps". `SetWindowPos` touches only
/// the z-order band, leaving the stealth ex-style (and the owner link) completely untouched.
#[tauri::command]
pub fn set_window_always_on_top(window: tauri::WebviewWindow, on_top: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        let raw = window.hwnd().map_err(|e| format!("hwnd: {e}"))?.0;
        let hwnd = HWND(raw);
        TOP_HWND.store(raw as isize, Ordering::Relaxed);
        ALWAYS_ON_TOP.store(on_top, Ordering::Relaxed);
        let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            SetWindowPos(
                hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .map_err(|e| format!("SetWindowPos(topmost): {e}"))?;
        }
        // Setting the band once only wins until the next app raises its own topmost window,
        // so keep re-asserting the front of the band for as long as overlay mode is on.
        if on_top {
            start_topmost_keeper();
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        window
            .set_always_on_top(on_top)
            .map_err(|e| format!("set_always_on_top: {e}"))
    }
}

/// Show or hide ONLY the taskbar button, via Tauri's native `set_skip_taskbar` (which on Windows
/// drives `ITaskbarList::DeleteTab` / `AddTab`). This is orthogonal to Alt+Tab: the main window
/// is permanently kept out of Alt+Tab by ownership (see [`make_owned_stealth`]), so `skip = false`
/// can bring the taskbar button back WITHOUT ever re-adding an Alt+Tab entry. `skip = true` is the
/// default discreet state (no taskbar button; reachable via the tray icon / Alt+Shift+H).
///
/// Note we deliberately do NOT touch `WS_EX_TOOLWINDOW` / `WS_EX_APPWINDOW` here anymore: setting
/// `WS_EX_APPWINDOW` would re-add the window to Alt+Tab, defeating the whole point.
#[tauri::command]
pub fn set_window_skip_taskbar(window: tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    set_skip_taskbar_impl(&window, skip)
}

fn set_skip_taskbar_impl(window: &tauri::WebviewWindow, skip: bool) -> Result<(), String> {
    window
        .set_skip_taskbar(skip)
        .map_err(|e| format!("set_skip_taskbar: {e}"))
}

/// Reveal a currently-hidden overlay window without stealing focus (`SW_SHOWNOACTIVATE`, so focus
/// stays on whatever is behind it) AND raise it to the front of its z-order band, so a recalled
/// overlay comes back visually ON TOP instead of buried behind the current window. `SW_SHOWNOACTIVATE`
/// alone shows the window but leaves it at its old stacking position, which is why the MB5
/// double-click sometimes brought it back underneath everything.
///
/// None of this disturbs the hidden features: Alt+Tab / taskbar / background-process stealth is
/// structural (hidden owner + tool-window style, set at startup by [`make_owned_stealth`]) and
/// content-protection is a persistent display affinity, so neither the show nor the restack touches
/// them; and because we never activate, the window behind keeps focus (so it never sees a focus-loss
/// event). Given the raw `HWND`; the mouse-hook gestures (MB4 peek, MB5 recall) call this.
#[cfg(windows)]
pub(crate) fn show_noactivate_keep_stealth(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOP, HWND_TOPMOST,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWNOACTIVATE, WS_EX_TOPMOST,
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        // Restack to the front WITHOUT activating (SWP_NOACTIVATE keeps focus where it is). Insert
        // into the correct band: if "on top" is enabled the window is topmost, so re-assert
        // HWND_TOPMOST to keep it above everything; otherwise HWND_TOP raises it above the other
        // normal windows.
        let topmost = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0;
        let after = if topmost { HWND_TOPMOST } else { HWND_TOP };
        let _ = SetWindowPos(
            hwnd,
            Some(after),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
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
        let layered = ex & (WS_EX_LAYERED.0 as isize) != 0;
        if alpha == 255 {
            // Fully opaque: strip WS_EX_LAYERED so the window is a plain, non-layered window.
            // A layered window is composited on its own surface and repaints noticeably slower
            // while being dragged - the window visibly lags the cursor - so we only pay that cost
            // when the overlay is ACTUALLY translucent, not at the default 100% opacity.
            if layered {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
            }
        } else {
            if !layered {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_LAYERED.0 as isize));
            }
            SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)
                .map_err(|e| format!("SetLayeredWindowAttributes: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_opacity_impl(_window: &tauri::WebviewWindow, _alpha: u8) -> Result<(), String> {
    Ok(())
}

/// Owner HWND (as `isize`) of the hidden window that owns the main overlay, stored so we never
/// re-create it and so nothing destroys it before process exit. Windows-only. `HWND` has no
/// `Drop`, so keeping this value is not what keeps the window alive - the window lives until
/// `DestroyWindow` or process teardown - but it documents intent and makes the op idempotent.
#[cfg(windows)]
static OWNER_HWND: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

/// Force `hwnd` into the app's "stealth" extended style: clear `WS_EX_APPWINDOW` and set
/// `WS_EX_TOOLWINDOW`, then commit the frame change. Two problems this solves:
///  - `WS_EX_APPWINDOW` FORCES a taskbar button (and an Alt+Tab entry) even on an owned window, so
///    ownership only actually hides the window once it is gone. tao sets it on every parentless
///    top-level window (its `ON_TASKBAR` flag) and re-adds it on any style re-diff (notably
///    `set_always_on_top`), so this has to be re-run after those.
///  - `WS_EX_TOOLWINDOW` marks the window a utility window, which is what makes Task Manager list
///    the process under "Background processes" rather than "Apps", and gives the shell a second
///    reason to keep it off the taskbar / Alt+Tab. It is invisible on our borderless window - there
///    is no native caption to slim.
///
/// The `SWP_FRAMECHANGED` commit makes the shell re-read the ex-style so both decisions update even
/// while the window is already visible (mirroring how tao's own frame change re-adds the button).
/// Idempotent. Windows-only.
#[cfg(windows)]
fn apply_stealth_exstyle(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindow,
        GWL_EXSTYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
    };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let want = (ex & !(WS_EX_APPWINDOW.0 as isize)) | (WS_EX_TOOLWINDOW.0 as isize);
        // No-op once already applied, so this is safe to re-run on every focus/resize event -
        // only the ONE transition that actually changes the style pays the cost below.
        if want == ex {
            return;
        }
        // Commit the new ex-style FIRST, so any window event fired by the hide/show cycle below
        // re-enters here, sees want == ex, and no-ops (no recursion / no loop).
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
        if IsWindowVisible(hwnd).as_bool() {
            // The shell only DROPS an existing taskbar button when a visible window's app-
            // eligibility is re-read at a hide -> show transition; a bare SetWindowPos/FRAMECHANGED
            // sets the style but leaves a stale button (documented Win32 quirk). Show WITHOUT
            // activating so we never steal focus. This is the one place a brief flicker can occur,
            // and only on the first application after tao's on-show restyle re-adds WS_EX_APPWINDOW.
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        } else {
            // Hidden (e.g. the startup call before the very first show): just commit the frame; the
            // first show then reads the already-stealthed style with no button ever created.
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        tracing::info!(
            "stealth: applied ex-style (was {:#x}, now {:#x})",
            ex as usize,
            want as usize
        );
        // The hide/show above restacks the window, which can drop it down the topmost band.
        reassert_topmost_if_on();
    }
}

/// Re-assert the taskbar / Alt+Tab / background-process stealth on `window`. The owner link set by
/// [`make_owned_stealth`] is permanent, but the `WS_EX_APPWINDOW` / `WS_EX_TOOLWINDOW` ex-style is
/// re-derived by tao on the first show (and on any restyle), which drops it. Call this on a
/// tab-independent trigger AFTER the window is visible - the window's own focus / resize events -
/// so the stealth holds no matter which view is mounted. Cheap no-op once the style is in place.
#[cfg(windows)]
pub(crate) fn reassert_stealth(window: &tauri::WebviewWindow) {
    if let Ok(h) = window.hwnd() {
        apply_stealth_exstyle(windows::Win32::Foundation::HWND(h.0));
    }
}

#[cfg(not(windows))]
pub(crate) fn reassert_stealth(_window: &tauri::WebviewWindow) {}

/// Recall the overlay: show it WITHOUT stealing foreground, restack it to the front of its
/// band, and re-assert stealth + topmost. Both the mouse gestures and the keyboard shortcut
/// go through here so they behave identically.
///
/// Deliberately no `set_focus()`. `SetForegroundWindow` is refused when another process owns
/// the foreground (the foreground lock), which is exactly the case while a fullscreen app is
/// active; Windows then falls back to flashing the taskbar button, and this window has none by
/// design - so the recall looked like it did nothing at all. Restacking with `SWP_NOACTIVATE`
/// needs no foreground rights and works over borderless-fullscreen apps.
pub(crate) fn recall_overlay(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        if let Ok(h) = window.hwnd() {
            show_noactivate_keep_stealth(windows::Win32::Foundation::HWND(h.0));
        }
        reassert_stealth(window);
        reassert_topmost_if_on();
    }
    #[cfg(not(windows))]
    {
        let _ = window.show();
    }
}

/// Permanently exclude `window` from BOTH the taskbar and the Alt+Tab switcher by making it an
/// OWNED top-level window with no `WS_EX_APPWINDOW`. The shell decides taskbar / Alt+Tab membership
/// from ownership + `WS_EX_APPWINDOW`, not from `WS_EX_TOOLWINDOW`: an owned top-level window that
/// lacks `WS_EX_APPWINDOW` is structurally kept out of both lists. Ownership is stored in
/// `GWLP_HWNDPARENT`, which tao/Tauri never read or rewrite, so it survives show/hide, always-on-top
/// toggles, activation, focus, and elapsed time with no re-commits. The one catch is the companion
/// `WS_EX_APPWINDOW` bit: tao sets it at creation (its `ON_TASKBAR` flag) and re-adds it whenever it
/// re-diffs styles, so [`apply_stealth_exstyle`] clears it (and sets `WS_EX_TOOLWINDOW`) here and
/// again after each `set_always_on_top`. Together these replace the old fragile reassert dance.
///
/// Call this at startup while the main window is still `visible: false` (so its first show is
/// already owned) AND again after the window is visible (focus / resize / frontend mount), because
/// tao's on-show restyle drops the ex-style. Idempotent: the owner is created once and cached; every
/// call re-asserts the owner link and re-applies the stealth ex-style.
///
/// The owner is an invisible, zero-size, never-shown top-level `Static` window (a built-in class,
/// so no `RegisterClassW`); `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` keep the owner itself out of
/// everything and unfocusable. IMPORTANT lifecycle: the system destroys an OWNED window when its
/// OWNER is destroyed, never the reverse - so the owner MUST outlive the main window. We therefore
/// never `DestroyWindow` it during runtime; process teardown cleans up both. Returns the owner
/// HWND as `isize`.
#[cfg(windows)]
pub(crate) fn make_owned_stealth(window: &tauri::WebviewWindow) -> Result<isize, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HINSTANCE, HWND};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, SetWindowLongPtrW, GWLP_HWNDPARENT, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_POPUP,
    };

    let main = HWND(window.hwnd().map_err(|e| format!("hwnd: {e}"))?.0);

    // Create the owner once and cache it; on later calls just re-assert the (cheap) owner link so
    // this whole function is safe to call repeatedly (startup, focus/resize events, frontend mount).
    let owner_val = if let Some(&existing) = OWNER_HWND.get() {
        unsafe { SetWindowLongPtrW(main, GWLP_HWNDPARENT, existing) };
        existing
    } else {
        let created = unsafe {
            let hinst: HINSTANCE = GetModuleHandleW(PCWSTR::null())
                .map_err(|e| format!("module handle: {e}"))?
                .into();
            // Built-in "Static" class => no RegisterClassW. Its name goes in as a null-terminated
            // UTF-16 buffer because the `w!` string-literal macro lives in the windows-strings
            // crate, which `windows` 0.58 does not re-export; `class` must outlive the
            // CreateWindowExW call. Invisible, 0-size, top-level (hwndparent = None), never shown;
            // TOOLWINDOW + NOACTIVATE keep the owner itself out of everything and unfocusable.
            let class: Vec<u16> = "Static\0".encode_utf16().collect();
            let owner = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinst),
                None,
            )
            .map_err(|e| format!("create owner: {e}"))?;
            // tao never rewrites GWLP_HWNDPARENT, so this owner link survives show/hide, topmost,
            // activation, focus, and elapsed time with zero re-commits.
            SetWindowLongPtrW(main, GWLP_HWNDPARENT, owner.0 as isize);
            owner.0 as isize
        };
        let _ = OWNER_HWND.set(created);
        created
    };

    // Always (re)apply the stealth ex-style. Ownership alone is not enough - tao created the main
    // window with its ON_TASKBAR flag set, so it carries WS_EX_APPWINDOW (forces a taskbar button /
    // Alt+Tab entry even when owned) and no WS_EX_TOOLWINDOW (Task Manager files it under "Apps"),
    // and tao re-derives that ex-style on the first show and on every restyle. So this must run on
    // EVERY call, not only the first, which is why the startup call, the focus/resize re-assert, and
    // the frontend mount all funnel through here.
    apply_stealth_exstyle(main);
    Ok(owner_val)
}

/// Frontend-callable entry point that (re)applies the full window stealth: ensures the hidden owner
/// exists and re-applies the taskbar / Alt+Tab / background-process ex-style. The webview calls this
/// once on mount - which happens AFTER the window is first shown and is independent of which view /
/// tab is active - so the stealth holds everywhere, not only while the Assistant view is mounted.
#[tauri::command]
pub fn apply_window_stealth(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(windows)]
    {
        make_owned_stealth(&window)?;
    }
    #[cfg(not(windows))]
    {
        let _ = &window;
    }
    Ok(())
}

/// Disable Windows "Aero Snap" for a window: dragging it to a screen edge or corner no longer
/// auto-tiles (half / quarter) it, so the borderless overlay can be positioned freely. Windows
/// only snap-tiles maximizable windows, so this clears WS_MAXIMIZEBOX. The custom title-bar
/// maximize button still works - it maximizes programmatically, a path that does not consult the
/// style bit. No-op off Windows.
#[cfg(windows)]
pub(crate) fn disable_aero_snap(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_STYLE, WS_MAXIMIZEBOX,
    };
    let hwnd = HWND(window.hwnd().map_err(|e| format!("hwnd: {e}"))?.0);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_MAXIMIZEBOX.0 as isize));
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn disable_aero_snap(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// True if the logical point (x, y) lands on (near the top-left of) any currently-connected
/// monitor. Used to avoid restoring a saved window position onto a monitor that is gone, which
/// would leave the window off-screen and unreachable.
pub(crate) fn point_on_any_monitor(window: &tauri::WebviewWindow, x: f64, y: f64) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return false;
    };
    monitors.iter().any(|m| {
        let sf = m.scale_factor();
        let mp = m.position().to_logical::<f64>(sf);
        let ms = m.size().to_logical::<f64>(sf);
        x >= mp.x - 40.0
            && x <= mp.x + ms.width - 120.0
            && y >= mp.y - 5.0
            && y <= mp.y + ms.height - 60.0
    })
}
