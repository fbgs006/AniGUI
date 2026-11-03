//! Streaming an episode: launching ani-cli/mpv, following the player's
//! next/previous requests, and reporting how far the user got.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::anicli::{AniCli, Mode};
use crate::history::{read_in_progress, resume_position};
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

/// The player behaviour chosen in Settings (just these, not the whole Config).
#[derive(Debug, Clone, Copy)]
struct PlayerOptions {
    autoplay_next: bool,
    fullscreen: bool,
}

/// The show being watched, as the frontend described it.
#[derive(Debug, Clone)]
struct Show {
    title: String,
    anime_id: Option<i64>,
    mal_id: Option<i64>,
    /// `0` when unknown (airing show).
    total_eps: i64,
}

/// How far the tracker saw the user get (`last_watched.json`).
#[derive(Debug, Default, PartialEq)]
struct PlaybackStats {
    percent: f64,
    time_pos: f64,
    duration: f64,
}

fn parse_playback_stats(raw: &str) -> Option<PlaybackStats> {
    let json: Value = serde_json::from_str(raw).ok()?;
    Some(PlaybackStats {
        percent: json["percent"].as_f64().unwrap_or(0.0),
        time_pos: json["time"].as_f64().unwrap_or(0.0),
        duration: json["duration"].as_f64().unwrap_or(0.0),
    })
}

/// The files AniGUI's mpv scripts write for us, under the AniGUI data folder.
struct PlayerFiles {
    stats: Option<PathBuf>,
    nav: Option<PathBuf>,
}

impl PlayerFiles {
    fn new() -> Self {
        let dir = dirs_next::data_dir().map(|d| d.join("AniGUI"));
        Self {
            stats: dir.as_ref().map(|d| d.join("last_watched.json")),
            nav: dir.map(|d| d.join("nav.json")),
        }
    }

    /// Stale files from a previous episode must not be mistaken for this one's.
    fn clear(&self) {
        for file in [&self.stats, &self.nav].into_iter().flatten() {
            let _ = std::fs::remove_file(file);
        }
    }

    fn read(file: &Option<PathBuf>) -> Option<String> {
        file.as_ref().and_then(|f| std::fs::read_to_string(f).ok())
    }

    /// `None` when the tracker never wrote anything (episode not really played).
    fn stats(&self) -> Option<PlaybackStats> {
        Self::read(&self.stats).and_then(|raw| parse_playback_stats(&raw))
    }

    fn nav_action(&self) -> Option<NavAction> {
        Self::read(&self.nav).and_then(|raw| parse_nav_action(&raw))
    }
}

/// The variables AniGUI's mpv scripts read (see `lua/anigui-controls.lua`).
fn player_env(
    options: PlayerOptions,
    show: &Show,
    ep: i64,
    nav_file: Option<&PathBuf>,
    resume_at: Option<f64>,
) -> Vec<(&'static str, OsString)> {
    let flag = |on: bool| OsString::from(if on { "1" } else { "0" });
    let mut env = vec![
        ("ANIGUI_EP", ep.to_string().into()),
        ("ANIGUI_EP_TOTAL", show.total_eps.to_string().into()),
        ("ANIGUI_AUTOPLAY", flag(options.autoplay_next)),
        ("ANIGUI_FULLSCREEN", flag(options.fullscreen)),
    ];
    if let Some(id) = show.mal_id {
        env.push(("ANIGUI_MAL_ID", id.to_string().into()));
    }
    if let Some(nav) = nav_file {
        env.push(("ANIGUI_NAV_FILE", nav.into()));
    }
    // Picks up an episode left part-way through (see `save_in_progress`).
    if let Some(t) = resume_at {
        env.push(("ANIGUI_RESUME", format!("{:.1}", t).into()));
    }
    env
}

/// What happened during one episode.
struct EpisodeResult {
    elapsed: f64,
    stats: Option<PlaybackStats>,
    nav: Option<NavAction>,
}

/// Plays one episode and waits for the player to close.
fn play_one(anicli: &AniCli, options: PlayerOptions, show: &Show, ep: i64, files: &PlayerFiles) -> EpisodeResult {
    files.clear();
    let resume_at = show.anime_id.and_then(|id| resume_position(&read_in_progress(), id, ep));

    let start = Instant::now();
    let _ = anicli
        .episode(&show.title, ep, Mode::Stream)
        .envs(player_env(options, show, ep, files.nav.as_ref(), resume_at))
        .status();

    EpisodeResult {
        elapsed: start.elapsed().as_secs_f64(),
        stats: files.stats(),
        nav: files.nav_action(),
    }
}

/// Gets the app out of the way once the player has had time to open.
fn minimize_soon(app: &AppHandle) {
    let window = app.get_webview_window("main");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(6));
        if let Some(window) = window {
            let _ = window.minimize();
        }
    });
}

fn restore_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Plays `first_ep`, then whatever the player's next/previous buttons ask
/// for: each episode is a separate ani-cli run.
fn watch(app: AppHandle, anicli: AniCli, options: PlayerOptions, show: Show, first_ep: i64, player_active: Arc<Mutex<bool>>) {
    minimize_soon(&app);
    let files = PlayerFiles::new();
    let mut ep = first_ep;
    let mut is_first_launch = true;

    loop {
        let result = play_one(&anicli, options, &show, ep, &files);
        let next_ep = result.nav.and_then(|action| navigation_target(action, ep, show.total_eps));
        // Asking for the next episode (button, key, or autoplay) means this one is done.
        let advanced = result.nav == Some(NavAction::Next) && next_ep.is_some();

        if next_ep.is_none() {
            *player_active.lock().unwrap() = false;
            restore_window(&app);
            let _ = app.emit("player_closed", ());
        }

        // A relaunched episode that never got as far as a tracker tick
        // (ani-cli failed, or it was closed within seconds) wasn't watched.
        // Emitted logged-out too: local history and in-progress don't need AniList.
        if is_first_launch || result.stats.is_some() {
            let stats = result.stats.unwrap_or_default();
            let _ = app.emit("playback_finished", json!({
                "animeId": show.anime_id,
                "epNum": ep,
                "elapsed": result.elapsed,
                "percent": stats.percent,
                "timePos": stats.time_pos,
                "duration": stats.duration,
                "advanced": advanced
            }));
        }

        match next_ep {
            Some(next) => {
                ep = next;
                is_first_launch = false;
                let _ = app.emit("playback_episode_changed", json!({ "epNum": next }));
            }
            None => break,
        }
    }
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
    let Some(anicli) = AniCli::from_config(&cfg) else {
        return json!({ "error": "bash not found. Set it in Settings." });
    };
    let options = PlayerOptions { autoplay_next: cfg.autoplay_next(), fullscreen: cfg.fullscreen() };
    let show = Show { title, anime_id, mal_id, total_eps: total_eps.unwrap_or(0) };

    let player_active = state.player_active.clone();
    {
        let mut active = player_active.lock().unwrap();
        if *active {
            return json!({ "error": "A video player is already running." });
        }
        *active = true;
    }

    std::thread::spawn(move || watch(app, anicli, options, show, ep_num, player_active));
    json!({ "success": true })
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

    #[test]
    fn reads_tracker_stats_with_missing_fields_as_zero() {
        let stats = parse_playback_stats(r#"{"percent":91.5,"time":1300}"#).unwrap();
        assert_eq!(stats, PlaybackStats { percent: 91.5, time_pos: 1300.0, duration: 0.0 });
        assert_eq!(parse_playback_stats("garbage"), None);
    }

    #[test]
    fn player_env_carries_only_what_is_known() {
        let show = Show { title: "X".into(), anime_id: None, mal_id: None, total_eps: 12 };
        let options = PlayerOptions { autoplay_next: true, fullscreen: false };
        let env = player_env(options, &show, 4, None, None);
        let get = |k: &str| env.iter().find(|(key, _)| *key == k).map(|(_, v)| v.to_string_lossy().into_owned());

        assert_eq!(get("ANIGUI_EP").as_deref(), Some("4"));
        assert_eq!(get("ANIGUI_EP_TOTAL").as_deref(), Some("12"));
        assert_eq!(get("ANIGUI_AUTOPLAY").as_deref(), Some("1"));
        assert_eq!(get("ANIGUI_FULLSCREEN").as_deref(), Some("0"));
        assert_eq!(get("ANIGUI_MAL_ID"), None);
        assert_eq!(get("ANIGUI_RESUME"), None);

        let show = Show { mal_id: Some(52991), ..show };
        let env = player_env(options, &show, 4, None, Some(312.04));
        assert!(env.contains(&("ANIGUI_MAL_ID", "52991".into())));
        assert!(env.contains(&("ANIGUI_RESUME", "312.0".into())));
    }
}
