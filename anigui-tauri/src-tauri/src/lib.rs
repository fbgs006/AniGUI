//! AniGUI backend entry point. Each module owns one responsibility; this file
//! only builds the shared state and registers the Tauri commands.

use std::sync::{Arc, Mutex};
use tauri::Manager;

mod anicli;
mod anilist;
mod bootstrap;
mod config;
mod downloads;
mod history;
mod playback;
mod shell;
mod state;
mod updates;

use anilist::commands as al;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    bootstrap::install_mpv_script();
    let config_path = config::get_config_path();
    let mut cfg = config::load_config_from_disk(&config_path);
    if config::clear_autosaved_bash_path(&mut cfg) {
        config::save_config_to_disk(&config_path, &cfg);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            config: Mutex::new(cfg),
            config_path,
            player_active: Arc::new(Mutex::new(false)),
            download_pid: Arc::new(Mutex::new(None)),
            viewer: Mutex::new(None),
            http: reqwest::Client::builder()
                .user_agent(concat!("AniGUI/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        })
        .setup(|app| {
            // Existing installs predate the player interface: fetch it in the
            // background so the next playback picks it up. Best effort only.
            let client = app.state::<AppState>().http.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = bootstrap::ensure_player_ui(&client, false, |_, _| {}).await {
                    eprintln!("[anigui] player interface install failed: {e}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_config,
            config::save_config,
            al::open_anilist_login,
            al::get_viewer_info,
            al::advanced_search,
            al::search_anime,
            al::get_trending,
            al::get_popular_this_season,
            al::get_upcoming_season,
            al::get_all_time_popular,
            al::get_continue_watching,
            al::get_planning,
            al::get_all_lists,
            al::bulk_update_entries,
            al::sync_progress,
            al::update_status,
            al::get_airing_schedule,
            al::get_user_stats,
            al::get_media_genres,
            playback::play_episode,
            downloads::start_download,
            downloads::cancel_download,
            downloads::get_downloads,
            downloads::play_local_file,
            downloads::delete_local_file,
            history::append_history,
            history::get_history,
            history::get_in_progress,
            history::save_in_progress,
            history::remove_in_progress,
            history::clear_history,
            history::export_history_csv,
            updates::get_app_info,
            updates::check_for_update,
            updates::install_update,
            updates::check_anicli_version,
            updates::update_anicli,
            bootstrap::check_runtime_status,
            bootstrap::install_runtime,
            bootstrap::skip_runtime_setup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
