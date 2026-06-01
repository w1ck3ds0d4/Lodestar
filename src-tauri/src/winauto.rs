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

#[cfg(not(windows))]
pub fn read_focused_window_text() -> anyhow::Result<String> {
    Err(anyhow::anyhow!(
        "Reading the focused window is only supported on Windows."
    ))
}
