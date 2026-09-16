//! Best-effort text capture of the focused (foreground) window via Windows UI
//! Automation.
//!
//! This is the fallback for when the clipboard route is unavailable - proctored
//! coding sites that disable copy/select, for example. Accessibility can't be turned
//! off (it would break screen readers), and the document Text pattern returns the
//! whole document including content scrolled off-screen. It's triggered by a GLOBAL
//! hotkey so the browser - not Lodestar - is the focused window when it runs.
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
// Lodestar when clicked, so the in-app/phone read targets THIS window (the page the user was
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

/// Start tracking the last non-Lodestar foreground window. A WinEvent OUT_OF_CONTEXT hook is
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
fn find_documents(
    walker: &uiautomation::UITreeWalker,
    element: &uiautomation::UIElement,
    depth: usize,
    out: &mut Vec<uiautomation::UIElement>,
) {
    use uiautomation::controls::ControlType;
    // Bounded so a pathological tree can't hang the read.
    if depth > 60 || out.len() >= 32 {
        return;
    }
    if matches!(element.get_control_type(), Ok(ControlType::Document)) {
        out.push(element.clone());
    }
    if let Ok(child) = walker.get_first_child(element) {
        let mut cur = child;
        loop {
            find_documents(walker, &cur, depth + 1, out);
            match walker.get_next_sibling(&cur) {
                Ok(next) => cur = next,
                Err(_) => break,
            }
        }
    }
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
    // A browser window holds SEVERAL Document elements, not one: Edge renders parts of its
    // own UI as internal web views, and those can come first in tree order and be empty.
    // Taking the first Document (as this used to) read one of those and returned nothing,
    // so "Read full page" failed on Edge. Consider every Document and keep the one that
    // actually carries the most text - the page the user is looking at.
    let mut documents = Vec::new();
    find_documents(&walker, &root, 0, &mut documents);
    let mut best: Option<String> = None;
    for doc in &documents {
        let Ok(pattern) = doc.get_pattern::<UITextPattern>() else {
            continue;
        };
        let Ok(range) = pattern.get_document_range() else {
            continue;
        };
        let Ok(text) = range.get_text(-1) else {
            continue;
        };
        let text = text.trim();
        if !text.is_empty() && best.as_ref().is_none_or(|b| text.len() > b.len()) {
            best = Some(text.to_string());
        }
    }
    if let Some(text) = best {
        return Ok(text);
    }
    // No document text anywhere: fall back to element names from the WINDOW root. Walking
    // an empty internal document here would also come back empty.
    let mut out = String::new();
    collect_text(&walker, &root, &mut out, 0);
    let out = out.trim().to_string();
    if out.is_empty() {
        Err(anyhow!("Couldn't read any text from that window."))
    } else {
        Ok(out)
    }
}

/// Read the page the user is on, even when triggered by a button/phone that focuses Lodestar:
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

/// Live end-to-end check of the page reader against a REAL browser page, including content
/// scrolled far below the fold ("Read full page" promises the whole page, not the viewport).
///
/// Ignored by default because it opens a visible Edge window. It uses a throwaway profile
/// directory, so it runs as a separate Edge instance and never touches the user's own
/// browser. Run locally with:
///   cargo test --lib live_reader -- --ignored --nocapture
#[cfg(all(test, windows))]
mod live_reader_tests {
    use super::read_hwnd_text;
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, Instant};

    fn powershell(script: &str) -> String {
        Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    /// Kill every process of the throwaway instance, matched on its unique profile path.
    /// NOT by the spawned PID: Edge relaunches its browser process under a new PID, so
    /// killing the original PID leaves the real window and its helper processes running.
    fn kill_probe_instance(profile_marker: &str) {
        powershell(&format!(
            "Get-CimInstance Win32_Process -Filter \"Name='msedge.exe'\" | \
             Where-Object {{ $_.CommandLine -like '*{profile_marker}*' }} | \
             ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}"
        ));
    }

    /// Poll until the probe window exposes both markers through UI Automation. Found by page
    /// title rather than PID, for the same relaunch reason as above.
    fn read_until_both(tag: &str, top: &str, bottom: &str) -> Result<String, String> {
        let deadline = Instant::now() + Duration::from_secs(40);
        let mut last = String::from("window never appeared");
        while Instant::now() < deadline {
            let hwnd: isize = powershell(&format!(
                "(Get-Process msedge -ErrorAction SilentlyContinue | \
                 Where-Object {{ $_.MainWindowTitle -like '*{tag}*' }} | \
                 Select-Object -First 1).MainWindowHandle"
            ))
            .parse()
            .unwrap_or(0);
            if hwnd != 0 {
                match read_hwnd_text(hwnd) {
                    Ok(text) if text.contains(top) && text.contains(bottom) => return Ok(text),
                    Ok(text) => {
                        last = format!("read {} chars, markers not all present yet", text.len())
                    }
                    Err(e) => last = e.to_string(),
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(last)
    }

    /// Baseline for the Edge test: a plain Win32 form with a labelled control. If this fails
    /// too, the top-down reader is broken in general rather than only on Chromium pages.
    #[test]
    #[ignore = "opens a small window; run locally with --ignored"]
    fn live_reader_reads_a_plain_win32_window() {
        let tag = format!("LODESTARFORM{}", std::process::id());
        let label = format!("{tag}LABEL");
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms;              $f = New-Object System.Windows.Forms.Form; $f.Text = '{tag}';              $l = New-Object System.Windows.Forms.Label; $l.Text = '{label}'; $l.AutoSize = $true;              $f.Controls.Add($l); [void]$f.ShowDialog()"
        );
        let mut child = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .spawn()
            .expect("launch form");

        let deadline = Instant::now() + Duration::from_secs(25);
        let mut result: Result<String, String> = Err("form never appeared".into());
        while Instant::now() < deadline {
            let hwnd: isize = powershell(&format!(
                "(Get-Process powershell -ErrorAction SilentlyContinue |                  Where-Object {{ $_.MainWindowTitle -eq '{tag}' }} |                  Select-Object -First 1).MainWindowHandle"
            ))
            .parse()
            .unwrap_or(0);
            if hwnd != 0 {
                result = read_hwnd_text(hwnd).map_err(|e| e.to_string());
                if matches!(&result, Ok(t) if t.contains(&label)) {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        let _ = child.kill();
        let _ = child.wait();

        let text = result.unwrap_or_else(|e| panic!("plain window not read: {e}"));
        println!("plain window read: {text:?}");
        assert!(
            text.contains(&label),
            "label text missing from a plain Win32 window"
        );
    }

    #[test]
    #[ignore = "opens a real Edge window; run locally with --ignored"]
    fn live_reader_reads_on_screen_and_off_screen_page_text() {
        let tag = format!("LODESTARPROBE{}", std::process::id());
        let top = format!("{tag}TOP");
        let bottom = format!("{tag}BOTTOM");
        let profile_marker = format!("lodestar-reader-{tag}");
        let dir = std::env::temp_dir().join(&profile_marker);
        std::fs::create_dir_all(&dir).expect("temp dir");
        // ~400 filler paragraphs push the bottom marker well outside the visible viewport.
        let filler =
            "<p>filler paragraph that pushes the next marker below the fold</p>\n".repeat(400);
        let page = dir.join("page.html");
        std::fs::write(
            &page,
            format!("<!doctype html><title>{tag}</title><body><p>{top}</p>{filler}<p>{bottom}</p></body>"),
        )
        .expect("write page");

        let edge = [
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .expect("Microsoft Edge is not installed");

        let mut cmd = Command::new(edge);
        // Diagnostic switch: Chromium only builds its full accessibility tree once it detects
        // assistive tech. Forcing it isolates "is the reader code correct" from "had Chromium
        // exposed the page yet". The default run stays honest and does NOT force it.
        if std::env::var_os("LODESTAR_FORCE_A11Y").is_some() {
            cmd.arg("--force-renderer-accessibility");
        }
        let mut child = cmd
            .arg(format!("--user-data-dir={}", dir.join("profile").display()))
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "--new-window",
                "--window-size=900,700",
            ])
            .arg(&page)
            .spawn()
            .expect("launch Edge");

        let result = read_until_both(&tag, &top, &bottom);

        // Always clean up the throwaway instance, even when the read failed.
        kill_probe_instance(&profile_marker);
        let _ = child.kill();
        let _ = child.wait();
        std::thread::sleep(Duration::from_millis(1000));
        let _ = std::fs::remove_dir_all(&dir);

        let text = result.unwrap_or_else(|e| panic!("reader did not return the page: {e}"));
        println!("read {} chars from the live page", text.len());
        assert!(text.contains(&top), "on-screen text was not read");
        assert!(
            text.contains(&bottom),
            "off-screen text was not read; Read full page must read the whole page"
        );
    }
}
