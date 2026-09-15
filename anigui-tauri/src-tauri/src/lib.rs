use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

const ANILIST_API: &str = "https://graphql.anilist.co";
const ANILIST_CLIENT_ID: &str = "45898";

// ─── Config ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    bash_path: Option<String>,
    quality: Option<String>,
    confirm_before_sync: Option<bool>,
    anilist_token: Option<String>,
    download_dir: Option<String>,
    theme: Option<String>,
    auto_sync: Option<bool>,
    dub: Option<bool>,
}

struct AppState {
    config: Mutex<Config>,
    config_path: std::path::PathBuf,
    player_active: Arc<Mutex<bool>>,
    http: reqwest::Client,
}

fn get_config_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs_next::home_dir().unwrap_or_default());
    base.join("anicli-gui").join("config.json")
}

fn get_history_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs_next::home_dir().unwrap_or_default());
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

const DELETE_MUTATION: &str = r#"
mutation ($id: Int) {
  DeleteMediaListEntry(id: $id) { deleted }
}"#;

fn popular_season_query() -> String {
    format!(
        r#"{} query ($season: MediaSeason, $year: Int, $page: Int) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media(type: ANIME, season: $season, seasonYear: $year, sort: POPULARITY_DESC) {{
      ...mediaFields
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
        "theme": cfg.theme.clone().unwrap_or_else(|| "purple".to_string()),
        "anilist_token": cfg.anilist_token.clone().unwrap_or_default(),
        "download_dir": cfg.download_dir.clone().unwrap_or_else(|| {
            dirs_next::download_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
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
    anilist_query(
        &state.http,
        &search_query(),
        serde_json::json!({ "search": query, "page": page }),
        None,
    )
    .await
}

#[tauri::command]
async fn get_trending(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    anilist_query(
        &state.http,
        &trending_query(),
        serde_json::json!({ "page": page }),
        None,
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

#[tauri::command]
fn play_episode(state: State<AppState>, app: AppHandle, title: String, ep_num: i64) -> Value {
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

    std::thread::spawn(move || {
        let safe_title = title.replace('"', "");
        let dub_flag = if cfg.dub.unwrap_or(false) { " --dub" } else { "" };
        let cmd = format!(
            r#"ani-cli "{}" -S 1 -e {} -q {}{} --exit-after-play"#,
            safe_title, ep_num, quality, dub_flag
        );
        let start = std::time::Instant::now();
        
        let window_clone = app.get_webview_window("main");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(6));
            if let Some(window) = window_clone {
                let _ = window.minimize();
            }
        });
        let _ = std::process::Command::new(&bash_path)
            .args(["-lc", &cmd])
            .status();

        *player_active.lock().unwrap() = false;

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        
        let _ = app.emit("player_closed", ());

        let elapsed = start.elapsed().as_secs_f64();
        let mut percent = 0.0;
        let mut time_pos = 0.0;
        
        if let Some(mut path) = dirs_next::data_dir() {
            path.push("AniGUI");
            let stats_file = path.join("last_watched.json");
            if let Ok(content) = std::fs::read_to_string(&stats_file) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(p) = json["percent"].as_f64() {
                        percent = p;
                    }
                    if let Some(t) = json["time"].as_f64() {
                        time_pos = t;
                    }
                }
            }
        }

        if token.is_some() {
            let _ = app.emit("playback_finished", serde_json::json!({
                "epNum": ep_num,
                "elapsed": elapsed,
                "percent": percent,
                "timePos": time_pos
            }));
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

        let mut child = match std::process::Command::new(&bash_path)
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
    anilist_query(
        &state.http,
        &popular_season_query(),
        serde_json::json!({ "season": season, "year": year, "page": page }),
        None,
    ).await
}

#[tauri::command]
async fn get_upcoming_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    anilist_query(
        &state.http,
        &upcoming_season_query(),
        serde_json::json!({ "season": season, "year": year, "page": page }),
        None,
    ).await
}

#[tauri::command]
async fn get_all_time_popular(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
    let page = page.unwrap_or(1);
    anilist_query(
        &state.http,
        &all_time_popular_query(),
        serde_json::json!({ "page": page }),
        None,
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

    anilist_query(&state.http, &q, serde_json::json!(vars), None).await
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
    let local_output = std::process::Command::new(&bash_path)
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

    // Fetch latest release from GitHub
    let client = reqwest::Client::builder()
        .user_agent("AniGUI")
        .build()
        .map_err(|e| e.to_string())?;

    let latest = match client
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
        popular_season_query, search_query, trending_query, upcoming_season_query,
        usable_anilist_token, AIRING_SCHEDULE_QUERY,
    };

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
    fn public_discovery_queries_do_not_request_user_list_data() {
        let public_queries = [
            advanced_search_query(),
            all_time_popular_query(),
            popular_season_query(),
            search_query(),
            trending_query(),
            upcoming_season_query(),
            AIRING_SCHEDULE_QUERY.to_string(),
        ];

        for query in public_queries {
            assert!(!query.contains("mediaListEntry"));
        }
    }
}

#[tauri::command]
async fn update_anicli(state: State<'_, AppState>, app: AppHandle) -> Result<Value, String> {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash_path.clone().or_else(find_bash);

    let Some(bash_path) = bash else {
        return Err("bash not found".to_string());
    };

    let output = std::process::Command::new(&bash_path)
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

// ─── MPV Script Installation ────────────────────────────────────────────────────

fn get_mpv_scripts_dir() -> Option<std::path::PathBuf> {
    let mut script_dir = None;
    if let Ok(path) = which::which("mpv") {
        if let Some(parent) = path.parent() {
            let portable = parent.join("portable_config");
            if portable.exists() {
                script_dir = Some(portable.join("scripts"));
            }
        }
    }
    
    if script_dir.is_none() {
        if let Some(mut path) = dirs_next::data_dir() {
            path.push("mpv");
            path.push("scripts");
            script_dir = Some(path);
        }
    }
    script_dir
}

fn install_mpv_script() {
    if let Some(path) = get_mpv_scripts_dir() {
        let _ = std::fs::create_dir_all(&path);
        let script_path = path.join("anigui-tracker.lua");
        
        let script_content = r#"
local mp = require 'mp'
local utils = require 'mp.utils'
local msg = require 'mp.msg'

local appdata = os.getenv("APPDATA")
if not appdata then return end

local anigui_dir = appdata .. "/AniGUI"
-- Ensure directory exists. Wrap in pcall so sandboxed mpv builds (e.g. Scoop)
-- that block os.execute don't crash the entire script on startup.
pcall(function() os.execute('mkdir "' .. anigui_dir .. '" >nul 2>&1') end)
local timestamps_file = anigui_dir .. "/timestamps.json"
local last_watched_file = anigui_dir .. "/last_watched.json"

local function read_json()
    local f = io.open(timestamps_file, "r")
    if not f then return {} end
    local content = f:read("*all")
    f:close()
    if not content or content == "" then return {} end
    local data, err = utils.parse_json(content)
    if not data then return {} end
    return data
end

local function write_json(data)
    local f = io.open(timestamps_file, "w")
    if not f then return end
    f:write(utils.format_json(data))
    f:close()
end

mp.register_event("file-loaded", function()
    local title = mp.get_property("media-title")
    if not title then return end
    
    local data = read_json()
    if data[title] then
        local time = data[title]
        mp.commandv("seek", tostring(time), "absolute")
        mp.osd_message("AniGUI: Resumed at " .. tostring(math.floor(time)) .. "s")
    end
end)

mp.add_periodic_timer(5, function()
    local title = mp.get_property("media-title")
    local time = mp.get_property_number("time-pos")
    local duration = mp.get_property_number("duration")
    
    if title and time and duration then
        if time > 10 and (duration - time) > 10 then
            local data = read_json()
            data[title] = time
            write_json(data)
        elseif (duration - time) <= 10 then
            local data = read_json()
            if data[title] then
                data[title] = nil
                write_json(data)
            end
        end
        
        -- Track the last played stats so the backend knows if the episode was finished.
        local stats = {
            percent = time / duration,
            duration = duration,
            time = time
        }
        local stats_file = io.open(last_watched_file, "w")
        if stats_file then
            stats_file:write(utils.format_json(stats))
            stats_file:close()
        end
    end
end)
"#;
        let _ = std::fs::write(script_path, script_content);
    }
}

// ─── App Entry ────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_mpv_script();
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
                .user_agent("AniGUI/1.3.0")
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
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
            sync_progress,
            update_status,
            play_episode,
            start_download,
            get_airing_schedule,
            get_user_stats,
            append_history,
            get_history,
            clear_history,
            check_for_update,
            install_update,
            check_anicli_version,
            update_anicli,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
