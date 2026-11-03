//! Streaming an episode: launching ani-cli/mpv, following the player's
//! next/previous requests, and reporting how far the user got.

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::history::{read_in_progress, resume_position};
use crate::shell::{shell_command, spawn_bash_command};
use crate::state::AppState;

/// What the player's next/previous-episode button asked for (see
/// `lua/anigui-controls.lua`, which writes it to `nav.json` before quitting mpv).
#[derive(Debug, Clone, Copy, PartialEq)]
enum NavAction {
    Next,
    Prev,
}

fn parse_nav_action(raw: &str) -> Option<NavAction> {
    let json: Value = serde_json::from_str(raw).ok()?;
    match json["action"].as_str()? {
        "next" => Some(NavAction::Next),
        "prev" => Some(NavAction::Prev),
        _ => None,
    }
}

/// The episode to launch after `action`, or `None` when it would fall off
/// either end. `total <= 0` means the episode count is unknown (airing show),
/// so "next" is left unbounded.
fn navigation_target(action: NavAction, current: i64, total: i64) -> Option<i64> {
    match action {
        NavAction::Prev => (current > 1).then(|| current - 1),
        NavAction::Next => (total <= 0 || current < total).then(|| current + 1),
    }
}

fn anigui_data_dir() -> Option<std::path::PathBuf> {
    dirs_next::data_dir().map(|d| d.join("AniGUI"))
}

#[tauri::command]
pub fn play_episode(
    state: State<AppState>,
    app: AppHandle,
    title: String,
    ep_num: i64,
    mal_id: Option<i64>,
    total_eps: Option<i64>,
    anime_id: Option<i64>,
) -> Value {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash();

    let Some(bash_path) = bash else {
        return serde_json::json!({ "error": "bash not found. Set it in Settings." });
    };

    let quality = cfg.quality();
    let player_active = state.player_active.clone();
    {
        let mut active = player_active.lock().unwrap();
        if *active {
            return serde_json::json!({ "error": "A video player is already running." });
        }
        *active = true;
    }

    let total_eps = total_eps.unwrap_or(0);
    let autoplay_next = cfg.autoplay_next();
    let fullscreen = cfg.fullscreen();

    std::thread::spawn(move || {
        let dub_flag = if cfg.dub() { " --dub" } else { "" };
        let data_dir = anigui_data_dir();
        let stats_file = data_dir.as_ref().map(|d| d.join("last_watched.json"));
        let nav_file = data_dir.as_ref().map(|d| d.join("nav.json"));

        let window_clone = app.get_webview_window("main");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(6));
            if let Some(window) = window_clone {
                let _ = window.minimize();
            }
        });

        // One iteration per episode: the player's next/previous buttons quit
        // mpv with a request in nav.json, and we relaunch on the target.
        let mut current_ep = ep_num;
        let mut is_first_launch = true;
        loop {
            // The title and quality reach bash as variables, never as script
            // text, so quotes, `$` or backticks in a title can't break out.
            let cmd = format!(
                r#"ani-cli "$ANIGUI_TITLE" -S 1 -e {} -q "$ANIGUI_QUALITY"{} --exit-after-play"#,
                current_ep, dub_flag
            );
            let start = std::time::Instant::now();

            // Stale files from a previous episode must not be mistaken for this one's.
            for stale in [&stats_file, &nav_file].into_iter().flatten() {
                let _ = std::fs::remove_file(stale);
            }

            let mut command = spawn_bash_command(&bash_path);
            command
                .args(["-lc", &shell_command(&cmd)])
                .env("ANIGUI_TITLE", &title)
                .env("ANIGUI_QUALITY", &quality)
                .env("ANIGUI_EP", current_ep.to_string())
                .env("ANIGUI_EP_TOTAL", total_eps.to_string())
                .env("ANIGUI_AUTOPLAY", if autoplay_next { "1" } else { "0" })
                .env("ANIGUI_FULLSCREEN", if fullscreen { "1" } else { "0" });
            if let Some(id) = mal_id {
                command.env("ANIGUI_MAL_ID", id.to_string());
            }
            if let Some(nav) = &nav_file {
                command.env("ANIGUI_NAV_FILE", nav);
            }
            // Picks up an episode left part-way through (see `save_in_progress`).
            if let Some(t) = anime_id.and_then(|id| resume_position(&read_in_progress(), id, current_ep)) {
                command.env("ANIGUI_RESUME", format!("{:.1}", t));
            }
            let _ = command.status();

            let elapsed = start.elapsed().as_secs_f64();
            let mut percent = 0.0;
            let mut time_pos = 0.0;
            let mut duration = 0.0;
            let mut played = false;

            if let Some(content) = stats_file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    played = true;
                    if let Some(p) = json["percent"].as_f64() {
                        percent = p;
                    }
                    if let Some(t) = json["time"].as_f64() {
                        time_pos = t;
                    }
                    if let Some(d) = json["duration"].as_f64() {
                        duration = d;
                    }
                }
            }

            let nav_action = nav_file
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .and_then(|raw| parse_nav_action(&raw));
            let next_ep = nav_action.and_then(|action| navigation_target(action, current_ep, total_eps));
            // Asking for the next episode (button, key, or autoplay) means this one is done.
            let advanced = nav_action == Some(NavAction::Next) && next_ep.is_some();

            if next_ep.is_none() {
                *player_active.lock().unwrap() = false;

                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }

                let _ = app.emit("player_closed", ());
            }

            // A relaunched episode that never got as far as a tracker tick
            // (ani-cli failed, or it was closed within seconds) wasn't watched.
            // Emitted logged-out too: local history and in-progress don't need AniList.
            if is_first_launch || played {
                let _ = app.emit("playback_finished", serde_json::json!({
                    "animeId": anime_id,
                    "epNum": current_ep,
                    "elapsed": elapsed,
                    "percent": percent,
                    "timePos": time_pos,
                    "duration": duration,
                    "advanced": advanced
                }));
            }

            match next_ep {
                Some(next) => {
                    current_ep = next;
                    is_first_launch = false;
                    let _ = app.emit("playback_episode_changed", serde_json::json!({ "epNum": next }));
                }
                None => break,
            }
        }
    });

    serde_json::json!({ "success": true })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_player_navigation_requests() {
        assert_eq!(parse_nav_action(r#"{"action":"next","from":3}"#), Some(NavAction::Next));
        assert_eq!(parse_nav_action(r#"{"action":"prev","from":3}"#), Some(NavAction::Prev));
        assert_eq!(parse_nav_action(r#"{"action":"quit"}"#), None);
        assert_eq!(parse_nav_action("not json"), None);
    }

    #[test]
    fn navigation_stays_within_the_episode_range() {
        assert_eq!(navigation_target(NavAction::Next, 3, 12), Some(4));
        assert_eq!(navigation_target(NavAction::Next, 12, 12), None);
        assert_eq!(navigation_target(NavAction::Prev, 3, 12), Some(2));
        assert_eq!(navigation_target(NavAction::Prev, 1, 12), None);
    }

    #[test]
    fn next_episode_is_unbounded_when_the_count_is_unknown() {
        assert_eq!(navigation_target(NavAction::Next, 30, 0), Some(31));
    }
}
