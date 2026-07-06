//! Global side-button (MB4 / MB5) mouse gestures for the overlay (main) window, via a
//! Windows low-level mouse hook (`WH_MOUSE_LL`). Lets the user peek behind, drag, and
//! toggle the always-on-top overlay from anywhere without touching its title bar:
//!
//!  - MB4 (XBUTTON1) held  -> momentary "peek behind": hide while down, show on release
//!    (shown WITHOUT stealing focus, so whatever is underneath keeps input).
//!  - MB5 (XBUTTON2) drag  -> move the window (follows the cursor once it passes ~5px).
//!  - MB5 double-click     -> toggle the window hidden / shown.
//!
//! MB4/MB5 button events are swallowed so they never also fire in other apps while Lodestar
//! runs; mouse-move and every other button pass through untouched.
//!
//! SAFETY: a `WH_MOUSE_LL` proc runs on the installing thread's message queue, and if it
//! blocks or panics it can freeze the WHOLE system cursor (Windows drops a hook that exceeds
//! LowLevelHooksTimeout, ~300ms). So the proc never makes a BLOCKING cross-thread window call.
//! Hide / show (peek, toggle) CAN block, so they are handed to the app's MAIN thread via
//! `run_on_main_thread` (which merely queues and returns) and run on the window's own thread.
//! The high-frequency drag move never touches a window from the hook: the hook only PUBLISHES the
//! desired position into atomics (plus one non-blocking `GetWindowRect` read at drag start), and a
//! dedicated drag thread applies it with a blocking `SetWindowPos`. Blocking is fine on that thread
//! (it is not the hook) and it actively drives the window's own thread to apply each move at once;
//! because it always reads the NEWEST target, fast moves coalesce instead of piling into a laggy
//! backlog - so the window stays tight to the cursor even when the cursor is far outside it. The
//! proc is lock-free and wrapped in `catch_unwind` so a panic can't unwind across FFI. No-op off
//! Windows.

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, Ordering};
    use std::sync::OnceLock;
    use std::time::Duration;

    use tauri::Manager;
    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, GetWindowRect, IsWindowVisible,
        SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage, HC_ACTION, MSG,
        MSLLHOOKSTRUCT, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, WH_MOUSE_LL,
        WM_MOUSEMOVE, WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    // The pressed X button is in the high word of MSLLHOOKSTRUCT.mouseData.
    const XBUTTON1: u16 = 0x0001; // MB4
    const XBUTTON2: u16 = 0x0002; // MB5

    // MB5 must travel this far (physical px, compared squared) before a hold becomes a drag
    // rather than a click. Max gap (ms) between two MB5 clicks to count as a double-click.
    const DRAG_THRESHOLD_SQ: i64 = 5 * 5;
    const DOUBLE_CLICK_MS: u32 = 350;

    // Set once in start(); used only to queue actions onto the main thread. AppHandle is Sync.
    static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
    // The overlay (main) window's raw HWND value, or 0 before start().
    static HOOK_HWND: AtomicIsize = AtomicIsize::new(0);

    // Gesture state, touched only by the single hook thread, so Relaxed is enough.
    static MB5_DOWN: AtomicBool = AtomicBool::new(false);
    static DRAGGING: AtomicBool = AtomicBool::new(false);
    static MB5_DOWN_X: AtomicI64 = AtomicI64::new(0);
    static MB5_DOWN_Y: AtomicI64 = AtomicI64::new(0);
    // Captured once when a drag begins: the window's top-left and the cursor point at that moment.
    // Each move then positions the window ABSOLUTELY (origin + how far the cursor has since moved),
    // so it tracks the cursor 1:1 with no jump on the first frame and no accumulated drift.
    static WIN_ORIGIN_X: AtomicI64 = AtomicI64::new(0);
    static WIN_ORIGIN_Y: AtomicI64 = AtomicI64::new(0);
    static DRAG_ANCHOR_X: AtomicI64 = AtomicI64::new(0);
    static DRAG_ANCHOR_Y: AtomicI64 = AtomicI64::new(0);
    // The latest window top-left the drag wants: published by the hook on each move, consumed by the
    // dedicated drag thread. Decoupling them COALESCES - the drag thread always jumps to the newest
    // target rather than replaying a backlog - so the window keeps up with fast moves even when the
    // cursor has run well outside it.
    static DRAG_TARGET_X: AtomicI64 = AtomicI64::new(0);
    static DRAG_TARGET_Y: AtomicI64 = AtomicI64::new(0);
    // Timestamp (u32 tick, stored in i64) of a pending first MB5 click, or -1 for none.
    static LAST_MB5_CLICK_TIME: AtomicI64 = AtomicI64::new(-1);
    // Whether the current MB4 peek actually hid the window (main thread only), so releasing
    // MB4 over an already-hidden (MB5-toggled) window is a no-op instead of revealing it.
    static PEEK_HID: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, Copy)]
    enum Action {
        PeekHide,
        PeekShow,
        Toggle,
    }

    fn stored_hwnd() -> Option<HWND> {
        let raw = HOOK_HWND.load(Ordering::Relaxed);
        if raw == 0 {
            None
        } else {
            Some(HWND(raw as *mut core::ffi::c_void))
        }
    }

    /// Runs on the MAIN thread (queued via run_on_main_thread), so these window calls are on
    /// the window's OWNING thread: direct, non-blocking, and never stall the hook thread.
    fn perform(action: Action) {
        let Some(hwnd) = stored_hwnd() else {
            return;
        };
        unsafe {
            match action {
                Action::PeekHide => {
                    if IsWindowVisible(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                        PEEK_HID.store(true, Ordering::Relaxed);
                    }
                }
                Action::PeekShow => {
                    if PEEK_HID.swap(false, Ordering::Relaxed) {
                        // Reveal without focus steal, keeping the taskbar/Alt+Tab stealth.
                        crate::window_fx::show_noactivate_keep_stealth(hwnd);
                    }
                }
                Action::Toggle => {
                    if IsWindowVisible(hwnd).as_bool() {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    } else {
                        crate::window_fx::show_noactivate_keep_stealth(hwnd);
                    }
                }
            }
        }
    }

    /// Non-blocking: queue the action onto the main thread and return immediately.
    fn dispatch(action: Action) {
        if let Some(app) = APP.get() {
            let _ = app.run_on_main_thread(move || perform(action));
        }
    }

    /// Interpret one hook event; returns true if it should be swallowed. Does NO window work
    /// on this (hook) thread - only atomics and a non-blocking dispatch to the main thread.
    fn handle(wparam: WPARAM, lparam: LPARAM) -> bool {
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let px = info.pt.x as i64;
        let py = info.pt.y as i64;
        let xbtn = ((info.mouseData >> 16) & 0xFFFF) as u16;
        let time = info.time; // u32 tick

        match msg {
            WM_XBUTTONDOWN => {
                if xbtn == XBUTTON1 {
                    dispatch(Action::PeekHide);
                    return true;
                }
                if xbtn == XBUTTON2 {
                    MB5_DOWN.store(true, Ordering::Relaxed);
                    DRAGGING.store(false, Ordering::Relaxed);
                    MB5_DOWN_X.store(px, Ordering::Relaxed);
                    MB5_DOWN_Y.store(py, Ordering::Relaxed);
                    return true;
                }
                false
            }
            WM_XBUTTONUP => {
                if xbtn == XBUTTON1 {
                    dispatch(Action::PeekShow);
                    return true;
                }
                if xbtn == XBUTTON2 {
                    let was_dragging = DRAGGING.swap(false, Ordering::Relaxed);
                    MB5_DOWN.store(false, Ordering::Relaxed);
                    if was_dragging {
                        // A drag ended: not a click, and it breaks any pending double-click.
                        LAST_MB5_CLICK_TIME.store(-1, Ordering::Relaxed);
                        return true;
                    }
                    // A click. Double-click if within the window of the previous click. Compare
                    // in u32 so the subtraction wraps correctly at the ~49-day tick rollover.
                    let last = LAST_MB5_CLICK_TIME.load(Ordering::Relaxed);
                    if last >= 0 && time.wrapping_sub(last as u32) <= DOUBLE_CLICK_MS {
                        dispatch(Action::Toggle);
                        LAST_MB5_CLICK_TIME.store(-1, Ordering::Relaxed);
                    } else {
                        LAST_MB5_CLICK_TIME.store(time as i64, Ordering::Relaxed);
                    }
                    return true;
                }
                false
            }
            WM_MOUSEMOVE => {
                // Never swallow moves (the cursor must keep travelling); while MB5 is held and past
                // the threshold, move the window to follow the cursor. The reposition is done RIGHT
                // HERE on the hook thread with a NON-blocking async SetWindowPos, skipping the
                // run_on_main_thread hop that made the window trail the cursor.
                if MB5_DOWN.load(Ordering::Relaxed) {
                    if !DRAGGING.load(Ordering::Relaxed) {
                        let dx = px - MB5_DOWN_X.load(Ordering::Relaxed);
                        let dy = py - MB5_DOWN_Y.load(Ordering::Relaxed);
                        if dx * dx + dy * dy > DRAG_THRESHOLD_SQ {
                            // Drag begins: anchor to the window's current top-left and this cursor
                            // point (one GetWindowRect read - non-blocking), so the window follows
                            // the cursor 1:1 from here with no jump and no per-frame drift.
                            if let Some(hwnd) = stored_hwnd() {
                                let mut r = RECT::default();
                                if unsafe { GetWindowRect(hwnd, &mut r) }.is_ok() {
                                    WIN_ORIGIN_X.store(r.left as i64, Ordering::Relaxed);
                                    WIN_ORIGIN_Y.store(r.top as i64, Ordering::Relaxed);
                                    DRAG_TARGET_X.store(r.left as i64, Ordering::Relaxed);
                                    DRAG_TARGET_Y.store(r.top as i64, Ordering::Relaxed);
                                    DRAG_ANCHOR_X.store(px, Ordering::Relaxed);
                                    DRAG_ANCHOR_Y.store(py, Ordering::Relaxed);
                                    // A drag breaks any pending double-click sequence.
                                    LAST_MB5_CLICK_TIME.store(-1, Ordering::Relaxed);
                                    DRAGGING.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    if DRAGGING.load(Ordering::Relaxed) {
                        // Publish the latest desired top-left (origin + how far the cursor has moved
                        // since the grab); the dedicated drag thread applies it. No window call here,
                        // so the hook can never stall.
                        let nx = WIN_ORIGIN_X.load(Ordering::Relaxed)
                            + (px - DRAG_ANCHOR_X.load(Ordering::Relaxed));
                        let ny = WIN_ORIGIN_Y.load(Ordering::Relaxed)
                            + (py - DRAG_ANCHOR_Y.load(Ordering::Relaxed));
                        DRAG_TARGET_X.store(nx, Ordering::Relaxed);
                        DRAG_TARGET_Y.store(ny, Ordering::Relaxed);
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Dedicated drag thread. While a drag is active, keep the window at the latest published
    /// target. It reads DRAG_TARGET (updated by the hook on every mouse move) and applies it with a
    /// BLOCKING SetWindowPos - safe here because this is NOT the hook thread, and blocking actively
    /// drives the window's own thread to apply each move immediately, so tracking stays tight even
    /// when the cursor is far outside the window. Because it always reads the NEWEST target, fast
    /// moves coalesce (stale intermediate positions are skipped) instead of piling into a backlog.
    fn drag_loop() {
        let mut last: (i32, i32) = (i32::MIN, i32::MIN);
        loop {
            if DRAGGING.load(Ordering::Relaxed) {
                let x = DRAG_TARGET_X.load(Ordering::Relaxed) as i32;
                let y = DRAG_TARGET_Y.load(Ordering::Relaxed) as i32;
                if (x, y) != last {
                    if let Some(hwnd) = stored_hwnd() {
                        unsafe {
                            let _ = SetWindowPos(
                                hwnd,
                                None,
                                x,
                                y,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        }
                        last = (x, y);
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            } else {
                last = (i32::MIN, i32::MIN);
                std::thread::sleep(Duration::from_millis(12));
            }
        }
    }

    unsafe extern "system" fn low_level_mouse_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < HC_ACTION as i32 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        // A panic must never unwind across this FFI boundary - swallow only on a clean `true`.
        let consumed =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(wparam, lparam)))
                .unwrap_or(false);
        if consumed {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, wparam, lparam)
        }
    }

    pub fn start(window: &tauri::WebviewWindow) {
        let Ok(handle) = window.hwnd() else {
            tracing::warn!("mouse_hook: could not get window HWND; side-button gestures disabled");
            return;
        };
        HOOK_HWND.store(handle.0 as isize, Ordering::Relaxed);
        let _ = APP.set(window.app_handle().clone());

        // Off-hook thread that applies drag moves to the window (see drag_loop); runs for the
        // app's lifetime, idle-polling cheaply when no drag is in progress.
        std::thread::spawn(drag_loop);

        // A WH_MOUSE_LL hook only fires while its installing thread pumps messages, so give it
        // a dedicated thread for the app's lifetime.
        std::thread::spawn(|| unsafe {
            let _hook = match SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(low_level_mouse_proc),
                HINSTANCE::default(), // ignored for a low-level hook
                0,
            ) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("mouse_hook: SetWindowsHookExW failed: {e}");
                    return;
                }
            };
            let mut msg = MSG::default();
            loop {
                let r = GetMessageW(&mut msg, None, 0, 0);
                if r.0 <= 0 {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    }
}

#[cfg(windows)]
pub fn start(window: &tauri::WebviewWindow) {
    imp::start(window);
}

#[cfg(not(windows))]
pub fn start(_window: &tauri::WebviewWindow) {}
