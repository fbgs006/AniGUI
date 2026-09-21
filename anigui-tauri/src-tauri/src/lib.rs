use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

mod bootstrap;

const ANILIST_API: &str = "https://graphql.anilist.co";
const ANILIST_CLIENT_ID: &str = "45898";

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct Config {
    bash_path: Option<String>,
    quality: Option<String>,
    confirm_before_sync: Option<bool>,
    anilist_token: Option<String>,
    download_dir: Option<String>,
    theme: Option<String>,
    auto_sync: Option<bool>,
    dub: Option<bool>,
    pub(crate) skip_auto_setup: Option<bool>,
}

pub(crate) struct AppState {
    pub(crate) config: Mutex<Config>,
    pub(crate) config_path: std::path::PathBuf,
    player_active: Arc<Mutex<bool>>,
    pub(crate) http: reqwest::Client,
}

fn get_config_path() -> std::path::PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| dirs_next::home_dir().unwrap_or_default());
    base.join("anicli-gui").join("config.json")
}

fn get_history_path() -> std::path::PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| dirs_next::home_dir().unwrap_or_default());
    base.join("anicli-gui").join("history.json")
}

fn load_config_from_disk(path: &std::path::Path) -> Config {
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(cfg) = serde_json::from_str(&data) {
                return cfg;
            }
        }
    }
    Config::default()
}

fn save_config_to_disk(path: &std::path::Path, cfg: &Config) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, data);
    }
}

fn find_bash() -> Option<String> {
    let bundled = bootstrap::bundled_bash_path();
    if bundled.exists() {
        return Some(bundled.to_string_lossy().to_string());
    }

    let candidates = [
        which::which("bash").ok().map(|p| p.to_string_lossy().to_string()),
        Some(r"C:\Program Files\Git\bin\bash.exe".to_string()),
        Some(r"C:\Program Files (x86)\Git\bin\bash.exe".to_string()),
    ];
    for c in &candidates {
        if let Some(path) = c {
            if std::path::Path::new(path).exists() {
                return Some(path.clone());
            }
        }
    }
    None
}

/// Wraps `Command::new(bash_path)`, augmenting the child process's PATH with
/// the bundled runtime's directories when `bash_path` is our own bundled
/// bash — so `ani-cli` (invoked by bare name in the shell command string)
/// can resolve `mpv`/`fzf`/`curl`/`grep`/`sed` without any of them being on
/// the *system* PATH. A user-set `bash_path` override is left untouched.
fn spawn_bash_command(bash_path: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(bash_path);
    if bootstrap::is_bundled_bash(bash_path) {
        let existing = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", bootstrap::build_augmented_path(&existing));
    }
    // bash.exe is a console-subsystem binary — spawning it from our windowed
    // app would otherwise pop a visible console window for every play/download/
    // version-check. CREATE_NO_WINDOW suppresses that; stdout/stderr piping
    // (where callers use it) is unaffected.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

// ─── AniList HTTP Helper ──────────────────────────────────────────────────────

/// An empty token is not authentication. Treat it as an anonymous request so
/// public discovery remains available for users who have not signed in.
fn usable_anilist_token(token: Option<&str>) -> Option<&str> {
    token.map(str::trim).filter(|token| !token.is_empty())
}

async fn anilist_query(
    client: &reqwest::Client,
    query: &str,
    variables: Value,
    token: Option<&str>,
) -> Result<Value, String> {
    let body = serde_json::json!({
        "query": query,
        "variables": variables
    });

    let max_retries = 2u32;
    let mut last_err = String::new();

    for attempt in 0..=max_retries {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(1000 * attempt as u64)).await;
        }

        let mut req = client
            .post(ANILIST_API)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");

        if let Some(tok) = usable_anilist_token(token) {
            req = req.header("Authorization", format!("Bearer {}", tok));
        }

        let resp = match req.json(&body).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };

        let status = resp.status();

        if status.as_u16() == 429 {
            last_err = "AniList API: rate limited (429)".to_string();
            continue;
        }

        let json: Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => {
                last_err = format!("error decoding response: {}", e);
                continue;
            }
        };

        if let Some(message) = json["errors"]
            .as_array()
            .and_then(|errors| errors.first())
            .and_then(|error| error["message"].as_str())
        {
            if message.contains("temporarily disabled") || message.contains("rate limit") {
                last_err = format!("AniList API: {}", message);
                continue;
            }
            return Err(format!("AniList API: {}", message));
        }
        if !status.is_success() {
            last_err = format!("AniList API returned HTTP {}", status);
            continue;
        }

        return Ok(json);
    }

    Err(last_err)
}

// ─── GraphQL Queries ──────────────────────────────────────────────────────────

const MEDIA_FIELDS: &str = r#"
fragment mediaFields on Media {
  id
  idMal
  title { romaji english }
  episodes
  averageScore
  status
  season
  seasonYear
  genres
  description(asHtml: false)
  coverImage { medium large }
  bannerImage
  nextAiringEpisode { airingAt episode timeUntilAiring }
  relations {
    edges {
      relationType
      node {
        id
        title { romaji english }
        coverImage { medium }
        type
        format
      }
    }
  }
}
"#;

fn advanced_search_query() -> String {
    format!(
        r#"{} query ($search: String, $genres: [String], $year: Int, $season: MediaSeason, $format: MediaFormat, $sort: [MediaSort], $page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(search: $search, genre_in: $genres, seasonYear: $year, season: $season, format: $format, type: ANIME, sort: $sort) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

fn search_query() -> String {
    format!(
        r#"{} query ($search: String, $page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(search: $search, type: ANIME, sort: SEARCH_MATCH) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

fn trending_query() -> String {
    format!(
        r#"{} query ($page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(type: ANIME, sort: TRENDING_DESC) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

const VIEWER_QUERY: &str = "query { Viewer { id } }";

fn current_query() -> String {
    format!(
        r#"{} query ($userId: Int) {{
  MediaListCollection(userId: $userId, type: ANIME, status: CURRENT) {{
    lists {{
      entries {{
        id
        progress
        status
        media {{ ...mediaFields }}
      }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

const UPDATE_MUTATION: &str = r#"
mutation ($mediaId: Int, $progress: Int, $status: MediaListStatus) {
  SaveMediaListEntry(mediaId: $mediaId, progress: $progress, status: $status) {
    id progress status
  }
}"#;

const UPDATE_STATUS_MUTATION: &str = r#"
mutation ($mediaId: Int, $status: MediaListStatus) {
  SaveMediaListEntry(mediaId: $mediaId, status: $status) {
    id progress status
  }
}"#;

fn popular_season_query() -> String {
    format!(
        r#"{} query ($season: MediaSeason, $year: Int, $page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(type: ANIME, season: $season, seasonYear: $year, sort: POPULARITY_DESC) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

fn upcoming_season_query() -> String {
    format!(
        r#"{} query ($season: MediaSeason, $year: Int, $page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(type: ANIME, season: $season, seasonYear: $year, sort: POPULARITY_DESC) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

fn all_time_popular_query() -> String {
    format!(
        r#"{} query ($page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(type: ANIME, sort: POPULARITY_DESC) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

fn planning_query() -> String {
    format!(
        r#"{} query ($userId: Int) {{
  MediaListCollection(userId: $userId, type: ANIME, status: PLANNING) {{
    lists {{
      entries {{
        id
        progress
        status
        media {{ ...mediaFields }}
      }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

// No status filter — returns every list entry (Watching/Planning/Paused/Dropped/
// Completed/Rewatching), used by the bulk list-management screen.
fn all_lists_query() -> String {
    format!(
        r#"{} query ($userId: Int) {{
  MediaListCollection(userId: $userId, type: ANIME) {{
    lists {{
      entries {{
        id
        progress
        status
        media {{ ...mediaFields }}
      }}
    }}
  }}
}}"#,
        MEDIA_FIELDS
    )
}

const MEDIA_GENRES_QUERY: &str = r#"
query ($ids: [Int]) {
  Page(page: 1, perPage: 50) {
    media(id_in: $ids, type: ANIME) {
      id
      genres
    }
  }
}"#;

const AIRING_SCHEDULE_QUERY: &str = r#"
query ($airingAtGreater: Int, $airingAtLesser: Int, $page: Int) {
  Page(page: $page, perPage: 50) {
    airingSchedules(airingAt_greater: $airingAtGreater, airingAt_lesser: $airingAtLesser, sort: TIME) {
      episode
      airingAt
      media {
        id idMal
        title { romaji english }
        coverImage { medium large }
        episodes averageScore status season seasonYear genres
        description(asHtml: false)
        mediaListEntry { id progress status }
      }
    }
  }
}
"#;

const USER_STATS_QUERY: &str = r#"
query ($userId: Int) {
  User(id: $userId) {
    name
    avatar { medium }
    statistics {
      anime {
        count
        episodesWatched
        minutesWatched
        meanScore
        genres(limit: 5, sort: COUNT_DESC) { genre count }
        formats { format count minutesWatched }
        statuses { status count }
      }
    }
  }
}
"#;

// ─── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
fn get_config(state: State<AppState>) -> Value {
    let cfg = state.config.lock().unwrap();
    let bash = cfg.bash_path.clone().or_else(find_bash);
    serde_json::json!({
        "bash_path": bash.unwrap_or_default(),
        "quality": cfg.quality.clone().unwrap_or_else(|| "best".to_string()),
        "confirm_before_sync": cfg.confirm_before_sync.unwrap_or(true),
        "auto_sync": cfg.auto_sync.unwrap_or(false),
        "dub": cfg.dub.unwrap_or(false),
        "theme": cfg.theme.clone().unwrap_or_else(|| "coral".to_string()),
        "anilist_token": cfg.anilist_token.clone().unwrap_or_default(),
        "download_dir": cfg.download_dir.clone().unwrap_or_else(|| {
            dirs_next::download_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        }),
        "skip_auto_setup": cfg.skip_auto_setup.unwrap_or(false)
    })
}

#[tauri::command]
fn save_config(state: State<AppState>, config: Value) -> bool {
    let mut cfg = state.config.lock().unwrap();
    if let Some(v) = config.get("bash_path").and_then(|v| v.as_str()) {
        cfg.bash_path = if v.is_empty() { None } else { Some(v.to_string()) };
    }
    if let Some(v) = config.get("anilist_token").and_then(|v| v.as_str()) {
        cfg.anilist_token = if v.is_empty() { None } else { Some(v.to_string()) };
    }
    if let Some(v) = config.get("quality").and_then(|v| v.as_str()) {
        cfg.quality = Some(v.to_string());
    }
    if let Some(v) = config.get("confirm_before_sync").and_then(|v| v.as_bool()) {
        cfg.confirm_before_sync = Some(v);
    }
    if let Some(v) = config.get("download_dir").and_then(|v| v.as_str()) {
        cfg.download_dir = if v.is_empty() { None } else { Some(v.to_string()) };
    }
    if let Some(v) = config.get("theme").and_then(|v| v.as_str()) {
        cfg.theme = Some(v.to_string());
    }
    if let Some(v) = config.get("auto_sync").and_then(|v| v.as_bool()) {
        cfg.auto_sync = Some(v);
    }
    if let Some(v) = config.get("dub").and_then(|v| v.as_bool()) {
        cfg.dub = Some(v);
    }
    save_config_to_disk(&state.config_path, &cfg);
    true
}

#[tauri::command]
fn open_anilist_login() -> bool {
    let url = format!(
        "https://anilist.co/api/v2/oauth/authorize?client_id={}&response_type=token",
        ANILIST_CLIENT_ID
    );
    let _ = open::that(url);
    true
}

#[tauri::command]
async fn search_anime(state: State<'_, AppState>, query: String, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    let token = state.config.lock().unwrap().anilist_token.clone();
    anilist_query(
        &state.http,
        &search_query(),
        serde_json::json!({ "search": query, "page": page }),
        token.as_deref(),
    )
    .await
}

#[tauri::command]
async fn get_trending(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    let token = state.config.lock().unwrap().anilist_token.clone();
    anilist_query(
        &state.http,
        &trending_query(),
        serde_json::json!({ "page": page }),
        token.as_deref(),
    )
    .await
}

#[tauri::command]
async fn get_continue_watching(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in".to_string())?;

    let viewer = anilist_query(&state.http, VIEWER_QUERY, serde_json::json!({}), Some(&token)).await?;
    let user_id = viewer["data"]["Viewer"]["id"]
        .as_i64()
        .ok_or("Could not get user ID")?;

    anilist_query(
        &state.http,
        &current_query(),
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    )
    .await
}

#[tauri::command]
async fn sync_progress(state: State<'_, AppState>, media_id: i64, ep_num: i64) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    anilist_query(
        &state.http,
        UPDATE_MUTATION,
        serde_json::json!({ "mediaId": media_id, "progress": ep_num, "status": "CURRENT" }),
        Some(&token),
    )
    .await
}

#[tauri::command]
async fn update_status(state: State<'_, AppState>, media_id: i64, status: String) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();

    if status == "Not in List" {
        // We need the list entry id — skip for now, handle on frontend
        return Ok(serde_json::json!({ "deleted": true }));
    }

    let token = token.ok_or("Not logged in")?;
    anilist_query(
        &state.http,
        UPDATE_STATUS_MUTATION,
        serde_json::json!({ "mediaId": media_id, "status": status }),
        Some(&token),
    )
    .await
}

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
fn play_episode(
    state: State<AppState>,
    app: AppHandle,
    title: String,
    ep_num: i64,
    mal_id: Option<i64>,
    total_eps: Option<i64>,
) -> Value {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash_path.clone().or_else(find_bash);

    let Some(bash_path) = bash else {
        return serde_json::json!({ "error": "bash not found. Set it in Settings." });
    };

    let quality = cfg.quality.clone().unwrap_or_else(|| "best".to_string());
    let token = cfg.anilist_token.clone();
    let player_active = state.player_active.clone();
    {
        let mut active = player_active.lock().unwrap();
        if *active {
            return serde_json::json!({ "error": "A video player is already running." });
        }
        *active = true;
    }

    let total_eps = total_eps.unwrap_or(0);

    std::thread::spawn(move || {
        let safe_title = title.replace('"', "");
        let dub_flag = if cfg.dub.unwrap_or(false) { " --dub" } else { "" };
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
            let cmd = format!(
                r#"ani-cli "{}" -S 1 -e {} -q {}{} --exit-after-play"#,
                safe_title, current_ep, quality, dub_flag
            );
            let start = std::time::Instant::now();

            // Stale files from a previous episode must not be mistaken for this one's.
            for stale in [&stats_file, &nav_file].into_iter().flatten() {
                let _ = std::fs::remove_file(stale);
            }

            let mut command = spawn_bash_command(&bash_path);
            command
                .args(["-lc", &cmd])
                .env("ANIGUI_EP", current_ep.to_string())
                .env("ANIGUI_EP_TOTAL", total_eps.to_string());
            if let Some(id) = mal_id {
                command.env("ANIGUI_MAL_ID", id.to_string());
            }
            if let Some(nav) = &nav_file {
                command.env("ANIGUI_NAV_FILE", nav);
            }
            let _ = command.status();

            let elapsed = start.elapsed().as_secs_f64();
            let mut percent = 0.0;
            let mut time_pos = 0.0;
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
                }
            }

            let next_ep = nav_file
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .and_then(|raw| parse_nav_action(&raw))
                .and_then(|action| navigation_target(action, current_ep, total_eps));

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
            if token.is_some() && (is_first_launch || played) {
                let _ = app.emit("playback_finished", serde_json::json!({
                    "epNum": current_ep,
                    "elapsed": elapsed,
                    "percent": percent,
                    "timePos": time_pos
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

#[tauri::command]
fn start_download(state: State<AppState>, app: AppHandle, title: String, ep_num: i64) -> Value {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash_path.clone().or_else(find_bash);

    let Some(bash_path) = bash else {
        return serde_json::json!({ "error": "bash not found" });
    };

    let quality = cfg.quality.clone().unwrap_or_else(|| "best".to_string());
    let download_dir = cfg.download_dir.clone().unwrap_or_else(|| {
        let mut d = dirs_next::download_dir().unwrap_or_default();
        d.push("AniGUI");
        let _ = std::fs::create_dir_all(&d);
        d.to_string_lossy().to_string()
    });

    std::thread::spawn(move || {
        let safe_title = title.replace('"', "");
        let dub_flag = if cfg.dub.unwrap_or(false) { " --dub" } else { "" };
        let cmd = format!(
            r#"cd "{}" && ani-cli "{}" -S 1 -e {} -q {}{} -d"#,
            download_dir, safe_title, ep_num, quality, dub_flag
        );

        let mut child = match spawn_bash_command(&bash_path)
            .args(["-lc", &cmd])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("download_log", serde_json::json!({ "line": format!("Error: {}", e) }));
                return;
            }
        };

        if let Some(mut stdout) = child.stdout.take() {
            use std::io::Read;
            let mut buf = [0u8; 1024];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 { break; }
                let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = app.emit("download_chunk", serde_json::json!({ "chunk": chunk }));
            }
        }

        let status = child.wait();
        let _ = app.emit("download_finished", serde_json::json!({
            "success": status.map(|s| s.success()).unwrap_or(false)
        }));
    });

    serde_json::json!({ "success": true })
}

#[tauri::command]
async fn get_viewer_info(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    const Q: &str = r#"query { Viewer { id name avatar { medium } } }"#;
    anilist_query(&state.http, Q, serde_json::json!({}), Some(&token)).await
}

#[tauri::command]
async fn get_popular_this_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    let token = state.config.lock().unwrap().anilist_token.clone();
    anilist_query(
        &state.http,
        &popular_season_query(),
        serde_json::json!({ "season": season, "year": year, "page": page }),
        token.as_deref(),
    ).await
}

#[tauri::command]
async fn get_upcoming_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    let token = state.config.lock().unwrap().anilist_token.clone();
    anilist_query(
        &state.http,
        &upcoming_season_query(),
        serde_json::json!({ "season": season, "year": year, "page": page }),
        token.as_deref(),
    ).await
}

#[tauri::command]
async fn get_all_time_popular(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    let token = state.config.lock().unwrap().anilist_token.clone();
    anilist_query(
        &state.http,
        &all_time_popular_query(),
        serde_json::json!({ "page": page }),
        token.as_deref(),
    ).await
}

#[tauri::command]
async fn get_planning(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    let viewer = anilist_query(&state.http, VIEWER_QUERY, serde_json::json!({}), Some(&token)).await?;
    let user_id = viewer["data"]["Viewer"]["id"].as_i64().ok_or("Could not get user ID")?;
    anilist_query(
        &state.http,
        &planning_query(),
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    ).await
}

// ─── Bulk List Management ────────────────────────────────────────────────────

#[tauri::command]
async fn get_all_lists(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    let viewer = anilist_query(&state.http, VIEWER_QUERY, serde_json::json!({}), Some(&token)).await?;
    let user_id = viewer["data"]["Viewer"]["id"].as_i64().ok_or("Could not get user ID")?;
    anilist_query(
        &state.http,
        &all_lists_query(),
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    ).await
}

#[derive(Debug, Clone, Deserialize)]
struct BulkUpdateOp {
    #[serde(rename = "mediaId")]
    media_id: i64,
    progress: Option<i64>,
    status: Option<String>,
}

// Applies a batch of per-show progress/status updates sequentially (AniList has
// no batch mutation), returning per-item success/failure so the UI can report
// partial failures rather than aborting the whole batch on the first error.
#[tauri::command]
async fn bulk_update_entries(state: State<'_, AppState>, ops: Vec<BulkUpdateOp>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;

    let mut results = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        if i > 0 {
            // Small spacing between mutations to stay well under AniList's rate limit.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }

        let outcome = if let Some(progress) = op.progress {
            anilist_query(
                &state.http,
                UPDATE_MUTATION,
                serde_json::json!({
                    "mediaId": op.media_id,
                    "progress": progress,
                    "status": op.status.clone().unwrap_or_else(|| "COMPLETED".to_string()),
                }),
                Some(&token),
            ).await
        } else if let Some(status) = &op.status {
            anilist_query(
                &state.http,
                UPDATE_STATUS_MUTATION,
                serde_json::json!({ "mediaId": op.media_id, "status": status }),
                Some(&token),
            ).await
        } else {
            Err("op has neither progress nor status".to_string())
        };

        match outcome {
            Ok(_) => results.push(serde_json::json!({ "mediaId": op.media_id, "ok": true })),
            Err(e) => results.push(serde_json::json!({ "mediaId": op.media_id, "ok": false, "error": e })),
        }
    }

    Ok(serde_json::json!({ "results": results }))
}


// ─── Airing Schedule ──────────────────────────────────────────────────────────

#[tauri::command]
async fn get_airing_schedule(state: State<'_, AppState>, days_ahead: Option<i64>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;
    let days = days_ahead.unwrap_or(7);
    // Slight look-back (1 h) so "airing now" shows up; look-ahead covers full week.
    let start = now - 3_600;
    let end   = now + days * 86_400;
    anilist_query(
        &state.http,
        AIRING_SCHEDULE_QUERY,
        serde_json::json!({ "airingAtGreater": start, "airingAtLesser": end, "page": 1 }),
        token.as_deref(),
    ).await
}

// ─── User Stats ────────────────────────────────────────────────────────────────

#[tauri::command]
async fn get_user_stats(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    let viewer = anilist_query(&state.http, VIEWER_QUERY, serde_json::json!({}), Some(&token)).await?;
    let user_id = viewer["data"]["Viewer"]["id"]
        .as_i64()
        .ok_or("Could not get user ID")?;
    anilist_query(
        &state.http,
        USER_STATS_QUERY,
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    ).await
}

// ─── Local Watch History ───────────────────────────────────────────────────────

#[tauri::command]
fn append_history(entry: Value) -> bool {
    let path = get_history_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut history: Vec<Value> = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    history.insert(0, entry);
    history.truncate(200);
    if let Ok(data) = serde_json::to_string_pretty(&history) {
        let _ = std::fs::write(&path, data);
        true
    } else {
        false
    }
}

#[tauri::command]
fn get_history() -> Value {
    let path = get_history_path();
    if !path.exists() {
        return serde_json::json!([]);
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!([]))
}

#[tauri::command]
fn clear_history() -> bool {
    let path = get_history_path();
    let _ = std::fs::write(&path, "[]");
    true
}

#[tauri::command]
fn export_history_csv(path: String, csv: String) -> Result<Value, String> {
    std::fs::write(&path, csv).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// Best-effort genre lookup for a batch of media ids — public data, works logged
// out. Used to enrich local watch-history stats with a "top genres" breakdown.
#[tauri::command]
async fn get_media_genres(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let mut all = Vec::new();
    for chunk in ids.chunks(50) {
        let res = anilist_query(
            &state.http,
            MEDIA_GENRES_QUERY,
            serde_json::json!({ "ids": chunk }),
            token.as_deref(),
        ).await?;
        if let Some(arr) = res["data"]["Page"]["media"].as_array() {
            all.extend(arr.clone());
        }
    }
    Ok(serde_json::json!({ "media": all }))
}

#[tauri::command]
async fn advanced_search(
    state: State<'_, AppState>,
    search: Option<String>,
    genres: Option<Vec<String>>,
    year: Option<i64>,
    season: Option<String>,
    format: Option<String>,
    sort: Option<Vec<String>>,
    page: Option<i64>,
) -> Result<Value, String> {
    let q = advanced_search_query();
    let page = page.unwrap_or(1);
    let mut vars = serde_json::Map::new();
    if let Some(s) = search { if !s.is_empty() { vars.insert("search".to_string(), serde_json::json!(s)); } }
    if let Some(g) = genres { if !g.is_empty() { vars.insert("genres".to_string(), serde_json::json!(g)); } }
    if let Some(y) = year { vars.insert("year".to_string(), serde_json::json!(y)); }
    if let Some(s) = season { if !s.is_empty() { vars.insert("season".to_string(), serde_json::json!(s)); } }
    if let Some(f) = format { if !f.is_empty() { vars.insert("format".to_string(), serde_json::json!(f)); } }
    let sort_val = sort.unwrap_or_else(|| vec!["TRENDING_DESC".to_string()]);
    vars.insert("sort".to_string(), serde_json::json!(sort_val));
    vars.insert("page".to_string(), serde_json::json!(page));

    let token = state.config.lock().unwrap().anilist_token.clone();
    anilist_query(&state.http, &q, serde_json::json!(vars), token.as_deref()).await
}

#[tauri::command]
fn get_downloads(state: State<AppState>) -> Value {
    let cfg = state.config.lock().unwrap().clone();
    let download_dir = cfg.download_dir.clone().unwrap_or_else(|| {
        let mut d = dirs_next::download_dir().unwrap_or_default();
        d.push("AniGUI");
        d.to_string_lossy().to_string()
    });

    let path = std::path::Path::new(&download_dir);
    if !path.exists() {
        return serde_json::json!([]);
    }

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "mp4" || ext == "mkv" {
                        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        files.push(serde_json::json!({
                            "name": name,
                            "path": path.to_string_lossy().to_string(),
                            "size": size,
                        }));
                    }
                }
            }
        }
    }
    serde_json::json!(files)
}

#[tauri::command]
fn play_local_file(path: String) -> Result<Value, String> {
    match open::that(&path) {
        Ok(_) => Ok(serde_json::json!({ "success": true })),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn delete_local_file(path: String) -> Result<Value, String> {
    match std::fs::remove_file(&path) {
        Ok(_) => Ok(serde_json::json!({ "success": true })),
        Err(e) => Err(e.to_string()),
    }
}

// ─── App Info ─────────────────────────────────────────────────────────────────

/// The Windows portable build is the same binary as the installer build, just
/// copied to a differently-named file in CI (see release.yml) — so the exe's
/// own filename is the only reliable signal for which one is currently running.
#[tauri::command]
fn get_app_info(app: AppHandle) -> Value {
    let version = app.package_info().version.to_string();
    let build_type = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .map(|name| if name.contains("portable") { "Portable" } else { "Installed" }.to_string())
        .unwrap_or_else(|| "Installed".to_string());
    serde_json::json!({ "version": version, "buildType": build_type })
}

// ─── Auto-Updater ───────────────────────────────────────────────────────────────

#[tauri::command]
async fn check_for_update(app: AppHandle) -> Result<Value, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(serde_json::json!({
            "available": true,
            "version": update.version,
            "body": update.body.unwrap_or_default()
        })),
        Ok(None) => Ok(serde_json::json!({ "available": false })),
        Err(e) => Ok(serde_json::json!({ "available": false, "error": e.to_string() })),
    }
}

#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart();
    }
    Ok(())
}

// ─── Ani-CLI Version Check ──────────────────────────────────────────────────────

/// Parses the numeric part of an ani-cli version for safe comparison.
///
/// GitHub tags can omit trailing zeroes (`v5.1`) while local builds may include
/// a patch component (`5.1.1`). Missing numeric components compare as zero.
/// Unknown version formats return `None` so that the UI never falsely claims an
/// update is available.
fn parse_anicli_version(version: &str) -> Option<Vec<u64>> {
    let numeric_part = version.trim().trim_start_matches(['v', 'V']);
    let numeric_part = numeric_part.split(['-', '+']).next().unwrap_or_default();

    let parts: Option<Vec<u64>> = numeric_part
        .split('.')
        .map(|part| {
            (!part.is_empty())
                .then(|| part.parse::<u64>().ok())
                .flatten()
        })
        .collect();

    match parts {
        Some(parts) if !parts.is_empty() => Some(parts),
        _ => None,
    }
}

/// Returns true only when `candidate` is strictly newer than `installed`.
fn anicli_update_available(installed: &str, candidate: &str) -> bool {
    let (Some(installed), Some(candidate)) = (
        parse_anicli_version(installed),
        parse_anicli_version(candidate),
    ) else {
        return false;
    };

    let component_count = installed.len().max(candidate.len());
    for index in 0..component_count {
        let local = installed.get(index).copied().unwrap_or(0);
        let latest = candidate.get(index).copied().unwrap_or(0);
        match latest.cmp(&local) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }

    false
}

#[tauri::command]
async fn check_anicli_version(state: State<'_, AppState>) -> Result<Value, String> {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash_path.clone().or_else(find_bash);

    let Some(bash_path) = bash else {
        return Err("bash not found".to_string());
    };

    // Get local ani-cli version
    let local_output = spawn_bash_command(&bash_path)
        .args(["-lc", "ani-cli -V 2>/dev/null || echo 'not-installed'"])
        .output()
        .map_err(|e| format!("Failed to run ani-cli: {}", e))?;

    let local_raw = String::from_utf8_lossy(&local_output.stdout)
        .trim()
        .to_string();

    // ani-cli -V typically prints something like "ani-cli 4.9" or just "4.9"
    let local_version = local_raw
        .replace("ani-cli", "")
        .replace('v', "")
        .trim()
        .to_string();

    if local_version.is_empty() || local_version == "not-installed" {
        return Ok(serde_json::json!({
            "installed": false,
            "local_version": null,
            "latest_version": null,
            "update_available": false
        }));
    }

    // Fetch latest release from GitHub, reusing the shared timeout-bounded client.
    let latest = match state
        .http
        .get("https://api.github.com/repos/pystardust/ani-cli/releases/latest")
        .send()
        .await
    {
        Ok(resp) => {
            if let Ok(json) = resp.json::<Value>().await {
                json["tag_name"]
                    .as_str()
                    .unwrap_or("")
                    .replace('v', "")
                    .to_string()
            } else {
                String::new()
            }
        }
        Err(_) => String::new(),
    };

    if latest.is_empty() {
        // Can't determine latest — don't nag the user
        return Ok(serde_json::json!({
            "installed": true,
            "local_version": local_version,
            "latest_version": null,
            "update_available": false
        }));
    }

    // A different string is not necessarily a newer release: for example a
    // local 5.1.1 build is already ahead of GitHub's v5.1 tag.
    let update_available = anicli_update_available(&local_version, &latest);

    Ok(serde_json::json!({
        "installed": true,
        "local_version": local_version,
        "latest_version": latest,
        "update_available": update_available
    }))
}

#[cfg(test)]
mod backend_helper_tests {
    use super::{
        advanced_search_query, all_time_popular_query, anicli_update_available,
        navigation_target, parse_nav_action, popular_season_query, search_query,
        trending_query, upcoming_season_query, usable_anilist_token, NavAction,
        AIRING_SCHEDULE_QUERY, MEDIA_FIELDS,
    };

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
    fn does_not_offer_an_older_release_as_an_update() {
        assert!(!anicli_update_available("5.1.1", "5.1"));
    }

    #[test]
    fn recognises_a_strictly_newer_release() {
        assert!(anicli_update_available("5.0", "v5.1"));
    }

    #[test]
    fn treats_missing_trailing_components_as_zero() {
        assert!(!anicli_update_available("5.1", "5.1.0"));
    }

    #[test]
    fn ignores_unparseable_versions() {
        assert!(!anicli_update_available("5.1", "latest"));
    }

    #[test]
    fn treats_blank_anilist_tokens_as_anonymous_requests() {
        assert_eq!(usable_anilist_token(Some("   ")), None);
        assert_eq!(usable_anilist_token(None), None);
        assert_eq!(usable_anilist_token(Some(" token ")), Some("token"));
    }

    #[test]
    fn discovery_queries_request_viewer_progress_when_logged_in() {
        // AniList returns `mediaListEntry` as null for anonymous requests, so
        // requesting it is safe for logged-out users while still letting
        // logged-in users see their real progress on Browse/Search results.
        let queries = [
            advanced_search_query(),
            all_time_popular_query(),
            popular_season_query(),
            search_query(),
            trending_query(),
            upcoming_season_query(),
            AIRING_SCHEDULE_QUERY.to_string(),
        ];

        for query in queries {
            assert!(query.contains("mediaListEntry"));
        }
    }

    #[test]
    fn media_fields_fragment_requests_next_airing_episode() {
        // Powers the "next episode airs in..." line on the detail page for
        // RELEASING shows — every query built on the shared fragment gets it.
        assert!(MEDIA_FIELDS.contains("nextAiringEpisode"));
    }
}

#[tauri::command]
async fn update_anicli(state: State<'_, AppState>, app: AppHandle) -> Result<Value, String> {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash_path.clone().or_else(find_bash);

    let Some(bash_path) = bash else {
        return Err("bash not found".to_string());
    };

    // The bundled ani-cli is a raw script, not a git checkout, so its own
    // `-U` self-updater has nothing to pull — re-fetch it pinned to the
    // latest release tag instead, and keep runtime/version.json in sync.
    if bootstrap::is_bundled_bash(&bash_path) {
        let result = bootstrap::reinstall_anicli_only(&state.http).await;
        let success = result.is_ok();
        let _ = app.emit("anicli_updated", serde_json::json!({ "success": success }));
        return result;
    }

    let output = spawn_bash_command(&bash_path)
        .args(["-lc", "ani-cli -U 2>&1"])
        .output()
        .map_err(|e| format!("Failed to update ani-cli: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    // Emit an event so the frontend can react
    let _ = app.emit("anicli_updated", serde_json::json!({ "success": success }));

    Ok(serde_json::json!({
        "success": success,
        "output": stdout,
        "stderr": stderr
    }))
}

// ─── App Entry ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    bootstrap::install_mpv_script();
    let config_path = get_config_path();
    let config = load_config_from_disk(&config_path);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            config: Mutex::new(config),
            config_path,
            player_active: Arc::new(Mutex::new(false)),
            http: reqwest::Client::builder()
                .user_agent("AniGUI/1.4.0")
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
            get_config,
            save_config,
            open_anilist_login,
            get_viewer_info,
            advanced_search,
            get_downloads,
            play_local_file,
            delete_local_file,
            search_anime,
            get_trending,
            get_popular_this_season,
            get_upcoming_season,
            get_all_time_popular,
            get_continue_watching,
            get_planning,
            get_all_lists,
            bulk_update_entries,
            sync_progress,
            update_status,
            play_episode,
            start_download,
            get_airing_schedule,
            get_user_stats,
            append_history,
            get_history,
            clear_history,
            export_history_csv,
            get_media_genres,
            get_app_info,
            check_for_update,
            install_update,
            check_anicli_version,
            update_anicli,
            bootstrap::check_runtime_status,
            bootstrap::install_runtime,
            bootstrap::skip_runtime_setup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
