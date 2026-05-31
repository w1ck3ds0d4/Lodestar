mod assistant;
mod career;
mod commands;
mod db;
mod ingest;
mod listen;
mod local_stt;
mod model;
mod screen;
mod sources;
mod usage;
mod util;
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
            });

            // Background scheduler: sync due sources, then poll on an interval.
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
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
            commands::study_plan,
            commands::set_screen_privacy,
            commands::get_assistant_settings,
            commands::set_api_key,
            commands::set_assistant_model,
            commands::set_answer_engine,
            commands::set_interview_context,
            commands::set_ollama_model,
            commands::set_ollama_url,
            commands::list_ollama_models,
            commands::ask_assistant,
            commands::auto_answer,
            commands::set_deepgram_key,
            commands::get_claude_usage,
            commands::reset_claude_usage,
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
            window_fx::set_window_opacity,
            window_fx::set_window_always_on_top,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
