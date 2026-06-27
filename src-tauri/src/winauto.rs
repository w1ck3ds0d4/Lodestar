//! Best-effort text capture of the focused (foreground) window via Windows UI
//! Automation.
//!
//! This is the fallback for when the clipboard route is unavailable - proctored
//! coding sites that disable copy/select, for example. Accessibility can't be turned
//! off (it would break screen readers), and the document Text pattern returns the
//! whole document including content scrolled off-screen. It's triggered by a GLOBAL
//! hotkey so the browser - not JobRadar - is the focused window when it runs.
//!
//! Best-effort by nature: canvas/image content exposes no text, and some apps publish
//! only a partial accessibility tree.

#[cfg(windows)]
pub fn read_focused_window_text() -> anyhow::Result<String> {
    use anyhow::anyhow;
    use uiautomation::controls::ControlType;
    use uiautomation::patterns::UITextPattern;
    use uiautomation::UIAutomation;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    // UI Automation is a COM API; ensure this thread has COM initialized (MTA). Called
    // from a fresh thread per invocation, so a repeat init just returns S_FALSE.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let automation = UIAutomation::new().map_err(|e| anyhow!("UI Automation unavailable: {e}"))?;
    let focused = automation
        .get_focused_element()
        .map_err(|e| anyhow!("No focused window element: {e}"))?;

    // Climb to the nearest Document (the web page content); fall back to the focused
    // element itself if there isn't one.
    let walker = automation
        .create_tree_walker()
        .map_err(|e| anyhow!("UI Automation walker failed: {e}"))?;
    let mut node = focused.clone();
    let mut target = focused.clone();
    for _ in 0..50 {
        if matches!(node.get_control_type(), Ok(ControlType::Document)) {
            target = node.clone();
            break;
        }
        match walker.get_parent(&node) {
            Ok(parent) => node = parent,
            Err(_) => break,
        }
    }

    // The Text pattern yields the entire document text in one call, including
    // off-screen content - exactly what we want.
    if let Ok(pattern) = target.get_pattern::<UITextPattern>() {
        if let Ok(range) = pattern.get_document_range() {
            if let Ok(text) = range.get_text(-1) {
                let text = text.trim();
                if !text.is_empty() {
                    return Ok(text.to_string());
                }
            }
        }
    }

    // Fallback: walk the subtree and collect element names (bounded so a huge tree
    // can't hang or blow up memory).
    let mut out = String::new();
    collect_text(&walker, &target, &mut out, 0);
    let out = out.trim().to_string();
    if out.is_empty() {
        Err(anyhow!(
            "Couldn't read any text from the focused window (it may expose no accessible text)."
        ))
    } else {
        Ok(out)
    }
}

#[cfg(windows)]
fn collect_text(
    walker: &uiautomation::UITreeWalker,
    element: &uiautomation::UIElement,
    out: &mut String,
    depth: usize,
) {
    const MAX_CHARS: usize = 60_000;
    if depth > 60 || out.len() > MAX_CHARS {
        return;
    }
    if let Ok(name) = element.get_name() {
        let name = name.trim();
        if !name.is_empty() && !out.ends_with(&format!("{name}\n")) {
            out.push_str(name);
            out.push('\n');
        }
    }
    if let Ok(child) = walker.get_first_child(element) {
        let mut cur = child;
        loop {
            collect_text(walker, &cur, out, depth + 1);
            if out.len() > MAX_CHARS {
                return;
            }
            match walker.get_next_sibling(&cur) {
                Ok(next) => cur = next,
                Err(_) => break,
            }
        }
    }
}

// The last FOREGROUND window that wasn't ours. A "Read full page" button necessarily focuses
// JobRadar when clicked, so the in-app/phone read targets THIS window (the page the user was
// on) instead of the app itself.
#[cfg(windows)]
static LAST_EXTERNAL_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(windows)]
unsafe extern "system" fn win_event_proc(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    if hwnd.0.is_null() {
        return;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != 0 && pid != GetCurrentProcessId() {
        LAST_EXTERNAL_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Start tracking the last non-JobRadar foreground window. A WinEvent OUT_OF_CONTEXT hook is
/// delivered to its installing thread's message queue, so this spawns a tiny message-pump
/// thread for the app's lifetime. No-op off Windows.
#[cfg(windows)]
pub fn start_foreground_tracker() {
    use windows::Win32::UI::Accessibility::SetWinEventHook;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG,
        WINEVENT_OUTOFCONTEXT,
    };
    std::thread::spawn(|| unsafe {
        let _hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
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

#[cfg(not(windows))]
pub fn start_foreground_tracker() {}

// Depth-first search for the page's Document node under `element` (the window root), so the
// Text pattern reads the whole document; falls back to the root if there isn't one.
#[cfg(windows)]
fn find_document(
    walker: &uiautomation::UITreeWalker,
    element: &uiautomation::UIElement,
    depth: usize,
) -> Option<uiautomation::UIElement> {
    use uiautomation::controls::ControlType;
    if depth > 60 {
        return None;
    }
    if matches!(element.get_control_type(), Ok(ControlType::Document)) {
        return Some(element.clone());
    }
    if let Ok(child) = walker.get_first_child(element) {
        let mut cur = child;
        loop {
            if let Some(found) = find_document(walker, &cur, depth + 1) {
                return Some(found);
            }
            match walker.get_next_sibling(&cur) {
                Ok(next) => cur = next,
                Err(_) => break,
            }
        }
    }
    None
}

// Read the full text of a specific window (by raw HWND value) via UI Automation.
#[cfg(windows)]
fn read_hwnd_text(hwnd_val: isize) -> anyhow::Result<String> {
    use anyhow::anyhow;
    use uiautomation::patterns::UITextPattern;
    use uiautomation::UIAutomation;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let automation = UIAutomation::new().map_err(|e| anyhow!("UI Automation unavailable: {e}"))?;
    // `Handle: From<isize>` takes the raw HWND value (no windows-version coupling).
    let root = automation
        .element_from_handle(hwnd_val.into())
        .map_err(|e| anyhow!("element_from_handle: {e}"))?;
    let walker = automation
        .create_tree_walker()
        .map_err(|e| anyhow!("UI Automation walker failed: {e}"))?;
    let target = find_document(&walker, &root, 0).unwrap_or_else(|| root.clone());

    if let Ok(pattern) = target.get_pattern::<UITextPattern>() {
        if let Ok(range) = pattern.get_document_range() {
            if let Ok(text) = range.get_text(-1) {
                let text = text.trim();
                if !text.is_empty() {
                    return Ok(text.to_string());
                }
            }
        }
    }
    let mut out = String::new();
    collect_text(&walker, &target, &mut out, 0);
    let out = out.trim().to_string();
    if out.is_empty() {
        Err(anyhow!("Couldn't read any text from that window."))
    } else {
        Ok(out)
    }
}

/// Read the page the user is on, even when triggered by a button/phone that focuses JobRadar:
/// prefer the last external foreground window (tracked), falling back to the focused element.
#[cfg(windows)]
pub fn read_active_page() -> anyhow::Result<String> {
    use std::sync::atomic::Ordering;
    let stored = LAST_EXTERNAL_HWND.load(Ordering::Relaxed);
    if stored != 0 {
        if let Ok(text) = read_hwnd_text(stored) {
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
    }
    read_focused_window_text()
}

#[cfg(not(windows))]
pub fn read_active_page() -> anyhow::Result<String> {
    read_focused_window_text()
}

#[cfg(not(windows))]
pub fn read_focused_window_text() -> anyhow::Result<String> {
    Err(anyhow::anyhow!(
        "Reading the focused window is only supported on Windows."
    ))
}
