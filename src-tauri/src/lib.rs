mod assistant;
mod capture;
mod career;
mod commands;
mod db;
mod gpu_tray;
mod listen;
mod local_stt;
mod migrate;
mod model;
mod mouse_hook;
mod netcheck;
mod phone;
mod screen;
mod secret;
mod usage;
mod util;
mod winauto;
mod window_fx;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::Manager;

/// Shared application state: the database handle, a reusable HTTP client, and a
/// flag coordinating the live audio-capture thread.
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub http: reqwest::Client,
    pub listening: Arc<AtomicBool>,
    /// Hash of the last screenshot answered, so the watcher skips unchanged screens.
    pub screen_hash: Arc<Mutex<Option<u64>>>,
    /// The running phone-companion server, if started.
    pub phone: Arc<Mutex<Option<phone::PhoneServer>>>,
}

/// Throttle for geometry saves (resize/move fire continuously during a drag).
static LAST_GEO_SAVE: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Write the current window size + position to the `window_state` setting, so the window
/// reopens where the user left it. Skips minimized / bogus geometry.
fn persist_window_state(win: &tauri::WebviewWindow, db: &Arc<Mutex<Connection>>) {
    if let (Ok(sz), Ok(pos)) = (win.outer_size(), win.outer_position()) {
        let sf = win.scale_factor().unwrap_or(1.0);
        let ls = sz.to_logical::<f64>(sf);
        let lp = pos.to_logical::<f64>(sf);
        if ls.width < 200.0 || ls.height < 150.0 {
            return;
        }
        let json = format!(
            "{{\"w\":{},\"h\":{},\"x\":{},\"y\":{}}}",
            ls.width, ls.height, lp.x, lp.y
        );
        if let Ok(conn) = db.lock() {
            let _ = db::set_setting(&conn, "window_state", &json);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("lodestar_lib=info,warn")),
        )
        .try_init();

    // Copy data from the old app identifier once, before any window or the database exists.
    let context = tauri::generate_context!();
    migrate::run_startup(&context.config().identifier);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // Global hotkeys (all Alt+Shift, so they never collide with browser shortcuts):
        //  - C: show/hide the capture overlay panels (instant, focus-free).
        //  - H: panic-hide - instantly toggle the main window's visibility.
        //  - R: read the focused window's text via UI Automation (pages that block copy/paste;
        //    runs on a fresh thread, COM + UIA can be slow).
        //  - L / A / W: listen / answer screen / watch - relayed to the Assistant view.
        //  - Up / Down: raise / lower opacity - relayed to the Assistant view.
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri::{Emitter, Manager};
                    use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
                    if shortcut.matches(alt_shift, Code::KeyC) {
                        capture::toggle_visibility(app);
                        return;
                    }
                    if shortcut.matches(alt_shift, Code::KeyH) {
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(true) {
                                let _ = win.hide();
                            } else {
                                window_fx::recall_overlay(&win);
                            }
                        }
                        return;
                    }
                    // Capture-mode actions: emit a command the Assistant view dispatches
                    // through the same handler the phone uses (listen/answer/watch/opacity).
                    if shortcut.matches(alt_shift, Code::KeyL) {
                        let _ = app.emit("hotkey", serde_json::json!({ "cmd": "listen" }));
                        return;
                    }
                    if shortcut.matches(alt_shift, Code::KeyA) {
                        let _ = app.emit("hotkey", serde_json::json!({ "cmd": "answer_screen" }));
                        return;
                    }
                    if shortcut.matches(alt_shift, Code::KeyW) {
                        let _ = app.emit("hotkey", serde_json::json!({ "cmd": "watch" }));
                        return;
                    }
                    if shortcut.matches(alt_shift, Code::ArrowUp) {
                        let _ = app.emit(
                            "hotkey",
                            serde_json::json!({ "cmd": "opacity", "dir": "up" }),
                        );
                        return;
                    }
                    if shortcut.matches(alt_shift, Code::ArrowDown) {
                        let _ = app.emit(
                            "hotkey",
                            serde_json::json!({ "cmd": "opacity", "dir": "down" }),
                        );
                        return;
                    }
                    if shortcut.matches(alt_shift, Code::KeyR) {
                        let app = app.clone();
                        std::thread::spawn(move || match winauto::read_focused_window_text() {
                            Ok(text) => {
                                let _ = app.emit("window-text", text);
                            }
                            Err(e) => {
                                let _ = app.emit("window-text-error", e.to_string());
                            }
                        });
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("lodestar.db");
            let conn = db::init(&db_path).expect("failed to initialize database");
            let db = Arc::new(Mutex::new(conn));
            // Make the DB reachable from the deep Claude call site for token tallying.
            usage::init(db.clone());

            let http = reqwest::Client::builder()
                .user_agent("Lodestar/0.1 (+personal job search)")
                .gzip(true)
                .build()?;

            app.manage(AppState {
                db: db.clone(),
                http: http.clone(),
                listening: Arc::new(AtomicBool::new(false)),
                screen_hash: Arc::new(Mutex::new(None)),
                phone: Arc::new(Mutex::new(None)),
            });

            // Register the global Alt+Shift+<key> hotkeys, by Code so they match the handler
            // exactly. Alt+Shift combos don't collide with browser shortcuts, and these are
            // OS-level anyway. Each failure (e.g. already taken) is non-fatal so the app still
            // starts. R: read page, H: panic-hide, C: show/hide panels, L: listen, A: answer
            // screen, W: watch, Up/Down: opacity.
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
                for code in [
                    Code::KeyR,
                    Code::KeyH,
                    Code::KeyC,
                    Code::KeyL,
                    Code::KeyA,
                    Code::KeyW,
                    Code::ArrowUp,
                    Code::ArrowDown,
                ] {
                    if let Err(e) = app
                        .global_shortcut()
                        .register(Shortcut::new(Some(alt_shift), code))
                    {
                        tracing::warn!("could not register hotkey {code:?}: {e}");
                    }
                }
            }

            // Closing the main window must not leave the always-on-top capture
            // panels floating; tear them down so the app can exit cleanly.
            if let Some(main) = app.get_webview_window("main") {
                // Disable Windows Aero Snap so dragging the borderless overlay to a screen
                // edge/corner no longer auto-tiles it; the user positions it freely.
                let _ = window_fx::disable_aero_snap(&main);

                // Permanently exclude the main window from BOTH the taskbar and Alt+Tab by making
                // it an OWNED top-level window (a hidden owner + no WS_EX_APPWINDOW). Done here
                // while the window is still visible:false, so its very first show is already
                // owned and no button/entry ever flashes. This is structural (stored in
                // GWLP_HWNDPARENT, which tao never rewrites), so it survives show/hide,
                // always-on-top, activation, focus, and elapsed time with no per-event re-commits.
                #[cfg(windows)]
                if let Err(e) = window_fx::make_owned_stealth(&main) {
                    tracing::warn!("could not make main window owned-stealth: {e}");
                }

                // Install the global side-button (MB4/MB5) mouse hook: MB4 peeks behind,
                // MB5 drags the overlay, MB5 double-click toggles it. Internal; Windows-only.
                mouse_hook::start(&main);

                // Restore the saved window size + position (best effort; skip if unset, too
                // small, or the saved spot is off every current monitor, e.g. a monitor was
                // unplugged since last run).
                if let Ok(conn) = db.lock() {
                    match db::get_setting(&conn, "window_state") {
                        Ok(Some(raw)) => {
                            tracing::info!("window_state: restoring {raw}");
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                                let num = |k: &str| v.get(k).and_then(|x| x.as_f64());
                                if let (Some(w), Some(hgt)) = (num("w"), num("h")) {
                                    if w >= 480.0 && hgt >= 360.0 {
                                        let _ = main.set_size(tauri::LogicalSize::new(w, hgt));
                                    }
                                }
                                if let (Some(x), Some(y)) = (num("x"), num("y")) {
                                    if window_fx::point_on_any_monitor(&main, x, y) {
                                        let _ =
                                            main.set_position(tauri::LogicalPosition::new(x, y));
                                    }
                                }
                            }
                        }
                        other => tracing::info!("window_state: nothing to restore ({other:?})"),
                    }
                }

                let h = app.handle().clone();
                let save_db = db.clone();
                let save_win = main.clone();
                main.on_window_event(move |e| match e {
                    // Persist geometry on resize/move so the last size/position survives HOWEVER
                    // the app is later closed (X button, tray Quit, Alt+F4, or a crash). Throttled
                    // because a resize/drag fires these continuously.
                    tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_) => {
                        // Re-assert the taskbar / Alt+Tab / background-process stealth here too:
                        // tao drops the stealth ex-style on the window's first show, and Resized
                        // fires right after that show (size/DPI restore). This is tab-independent,
                        // so the stealth holds on EVERY view - not only while Assistant is mounted.
                        // Cheap no-op once the style is already in place.
                        window_fx::reassert_stealth(&save_win);
                        if let Ok(mut last) = LAST_GEO_SAVE.lock() {
                            let now = std::time::Instant::now();
                            let due = last.is_none_or(|t| now.duration_since(t).as_millis() >= 400);
                            if due {
                                *last = Some(now);
                                persist_window_state(&save_win, &save_db);
                            }
                        }
                    }
                    // Every time the window takes focus (user reveals/clicks it), re-assert stealth,
                    // so a reveal that re-shows the window can never leave a stale taskbar button.
                    tauri::WindowEvent::Focused(true) => {
                        window_fx::reassert_stealth(&save_win);
                    }
                    tauri::WindowEvent::CloseRequested { .. } => {
                        persist_window_state(&save_win, &save_db);
                        capture::close_panels(&h);
                    }
                    _ => {}
                });
            }

            // Track the last external foreground window so a "Read full page" button/phone tap
            // can read the page the user was on (clicking a button focuses Lodestar itself).
            winauto::start_foreground_tracker();

            // System-tray icon: a discreet, always-available access point - especially when
            // the window is hidden from the taskbar and Alt+Tab. Left-click (or the Show item)
            // reveals and focuses the window; right-click opens a small menu.
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let reveal = |app: &tauri::AppHandle| {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                };
                let show_i =
                    MenuItem::with_id(app, "tray_show", "Show Lodestar", true, None::<&str>)?;
                let hide_i = MenuItem::with_id(app, "tray_hide", "Hide", true, None::<&str>)?;
                let quit_i = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_i, &hide_i, &quit_i])?;

                let mut builder = TrayIconBuilder::new()
                    .tooltip("Lodestar")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app, event| match event.id.as_ref() {
                        "tray_show" => reveal(app),
                        "tray_hide" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.hide();
                            }
                        }
                        "tray_quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                    });
                if let Some(icon) = app.default_window_icon() {
                    builder = builder.icon(icon.clone());
                }
                let tray = builder.build(app)?;
                // Repaint the tray icon with the live GPU temperature (no-op without
                // an NVIDIA GPU; the default icon stays put).
                gpu_tray::spawn_updater(tray);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_stories,
            commands::add_story,
            commands::update_story,
            commands::remove_story,
            commands::draft_story,
            commands::list_profiles,
            commands::active_profile,
            commands::set_active_profile,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::application_doc,
            commands::score_ats,
            commands::list_sessions,
            commands::save_session,
            commands::remove_session,
            commands::export_session,
            commands::export_all_sessions,
            commands::session_review,
            commands::set_screen_privacy,
            commands::get_assistant_settings,
            commands::set_api_key,
            commands::set_assistant_model,
            commands::set_answer_engine,
            commands::set_answer_style,
            commands::set_interview_context,
            commands::set_ollama_model,
            commands::set_ollama_url,
            commands::list_ollama_models,
            commands::ask_assistant,
            commands::ask_image,
            commands::auto_answer,
            commands::read_clipboard,
            commands::set_deepgram_key,
            commands::get_usage,
            commands::reset_usage,
            commands::set_usage_credit,
            commands::set_usage_spent,
            commands::deepgram_balance,
            commands::set_stt_engine,
            commands::set_stt_language,
            commands::set_stt_diarize,
            commands::set_fast_mode,
            commands::check_claims,
            commands::debrief,
            commands::set_overclaim_guard,
            commands::set_predict_followups,
            commands::set_capture_source,
            commands::list_monitors,
            commands::answer_screen,
            commands::answer_screen_hybrid,
            commands::capture_screen_png,
            commands::explain_region,
            commands::list_audio_devices,
            commands::set_input_device,
            commands::set_output_device,
            commands::start_listening,
            commands::stop_listening,
            commands::start_phone_server,
            commands::stop_phone_server,
            commands::phone_broadcast,
            commands::phone_network_status,
            commands::phone_allow_firewall,
            commands::open_capture_panels,
            commands::close_capture_panels,
            commands::set_capture_opacity,
            commands::toggle_capture_panels,
            commands::move_capture_panel,
            commands::read_active_page,
            commands::set_capture_protection,
            window_fx::set_window_opacity,
            window_fx::set_window_always_on_top,
            window_fx::set_window_skip_taskbar,
            window_fx::apply_window_stealth,
        ])
        .run(context)
        .expect("error while running tauri application");
}
