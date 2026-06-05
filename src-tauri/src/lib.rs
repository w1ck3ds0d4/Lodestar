mod assistant;
mod capture;
mod career;
mod commands;
mod db;
mod ingest;
mod listen;
mod local_stt;
mod model;
mod phone;
mod scam;
mod screen;
mod sources;
mod usage;
mod util;
mod winauto;
mod window_fx;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jobradar=info,warn")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // Global hotkeys:
        //  - Alt+Shift+C: show/hide the capture overlay panels (instant, focus-free).
        //  - Alt+Shift+H: panic-hide - instantly toggle the main window's visibility.
        //  - Alt+Shift+R: read the focused window's text via UI Automation, for pages
        //    that block copy/paste (runs on a fresh thread; COM + UIA can be slow).
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
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
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
            let db_path = dir.join("jobradar.db");
            let conn = db::init(&db_path).expect("failed to initialize database");
            let db = Arc::new(Mutex::new(conn));
            // Make the DB reachable from the deep Claude call site for token tallying.
            usage::init(db.clone());

            let http = reqwest::Client::builder()
                .user_agent("JobRadar/0.1 (+personal job search)")
                .gzip(true)
                .build()?;

            app.manage(AppState {
                db: db.clone(),
                http: http.clone(),
                listening: Arc::new(AtomicBool::new(false)),
                screen_hash: Arc::new(Mutex::new(None)),
                phone: Arc::new(Mutex::new(None)),
            });

            // Register the global "read focused window" hotkey. Alt+Shift+R is unlikely
            // to collide with a browser shortcut; failure (e.g. already taken) is
            // non-fatal so the rest of the app still starts.
            {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                for sc in ["Alt+Shift+R", "Alt+Shift+H"] {
                    if let Err(e) = app.global_shortcut().register(sc) {
                        tracing::warn!("could not register hotkey {sc}: {e}");
                    }
                }
                // Alt+Shift+C toggles the capture overlay panels' visibility.
                if let Err(e) = app.global_shortcut().register("Alt+Shift+C") {
                    tracing::warn!("could not register capture-toggle hotkey: {e}");
                }
            }

            // Closing the main window must not leave the always-on-top capture
            // panels floating; tear them down so the app can exit cleanly.
            if let Some(main) = app.get_webview_window("main") {
                let h = app.handle().clone();
                main.on_window_event(move |e| {
                    if matches!(e, tauri::WindowEvent::CloseRequested { .. }) {
                        capture::close_panels(&h);
                    }
                });
            }

            // Background scheduler: sync due sources, then poll on an interval.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // One-off derived-column backfill (re-derives every existing job). This is
                // CPU-heavy, so run it on a blocking thread - never in setup(), which runs
                // on the UI thread and would freeze the launch.
                let bf = db.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(conn) = bf.lock() {
                        if let Err(e) = db::backfill_if_needed(&conn) {
                            tracing::warn!("derived backfill failed: {e}");
                        }
                    }
                })
                .await;
                tokio::time::sleep(Duration::from_secs(2)).await;
                loop {
                    ingest::sync_all(db.clone(), http.clone(), app_handle.clone(), false).await;
                    tokio::time::sleep(Duration::from_secs(120)).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::search_jobs,
            commands::work_country,
            commands::set_work_country,
            commands::list_sources,
            commands::add_source,
            commands::update_source,
            commands::remove_source,
            commands::get_stats,
            commands::list_alerts,
            commands::add_alert,
            commands::remove_alert,
            commands::sync_now,
            commands::run_source,
            commands::list_tracked,
            commands::add_tracked,
            commands::update_tracked,
            commands::remove_tracked,
            commands::set_tracked_next_action,
            commands::prep_application,
            commands::add_milestone,
            commands::set_milestone,
            commands::remove_milestone,
            commands::list_skills,
            commands::add_skill,
            commands::update_skill,
            commands::log_study,
            commands::remove_skill,
            commands::list_stories,
            commands::add_story,
            commands::update_story,
            commands::remove_story,
            commands::draft_story,
            commands::application_doc,
            commands::score_ats,
            commands::practice_question,
            commands::practice_feedback,
            commands::list_sessions,
            commands::save_session,
            commands::remove_session,
            commands::session_review,
            commands::list_cards,
            commands::review_card,
            commands::remove_card,
            commands::generate_cards,
            commands::study_plan,
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
            commands::auto_answer,
            commands::read_clipboard,
            commands::set_deepgram_key,
            commands::get_usage,
            commands::reset_usage,
            commands::set_usage_credit,
            commands::deepgram_balance,
            commands::set_stt_engine,
            commands::set_stt_language,
            commands::set_stt_diarize,
            commands::set_capture_source,
            commands::list_monitors,
            commands::answer_screen,
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
            commands::open_capture_panels,
            commands::close_capture_panels,
            commands::toggle_capture_panels,
            window_fx::set_window_opacity,
            window_fx::set_window_always_on_top,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
