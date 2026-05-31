mod assistant;
mod career;
mod commands;
mod db;
mod ingest;
mod listen;
mod local_stt;
mod model;
mod sources;
mod util;

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

            let http = reqwest::Client::builder()
                .user_agent("JobRadar/0.1 (+personal job search)")
                .gzip(true)
                .build()?;

            app.manage(AppState {
                db: db.clone(),
                http: http.clone(),
                listening: Arc::new(AtomicBool::new(false)),
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
            commands::add_milestone,
            commands::set_milestone,
            commands::remove_milestone,
            commands::list_skills,
            commands::add_skill,
            commands::update_skill,
            commands::log_study,
            commands::remove_skill,
            commands::set_screen_privacy,
            commands::get_assistant_settings,
            commands::set_api_key,
            commands::set_assistant_model,
            commands::ask_assistant,
            commands::set_deepgram_key,
            commands::set_stt_engine,
            commands::start_listening,
            commands::stop_listening,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
