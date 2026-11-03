//! The Tauri commands backed by AniList (browse, search, lists, sync, stats).

use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use super::client::{anilist_query, viewer_id};
use super::queries::{
    advanced_search_query, all_lists_query, all_time_popular_query, current_query,
    planning_query, popular_season_query, search_query, trending_query, upcoming_season_query,
    AIRING_SCHEDULE_QUERY, DELETE_ENTRY_MUTATION, MEDIA_GENRES_QUERY, UPDATE_MUTATION,
    UPDATE_STATUS_MUTATION, USER_STATS_QUERY,
};
use crate::state::AppState;

const ANILIST_CLIENT_ID: &str = "45898";

#[tauri::command]
pub fn open_anilist_login() -> bool {
    let url = format!(
        "https://anilist.co/api/v2/oauth/authorize?client_id={}&response_type=token",
        ANILIST_CLIENT_ID
    );
    let _ = open::that(url);
    true
}

#[tauri::command]
pub async fn search_anime(state: State<'_, AppState>, query: String, page: Option<i64>) -> Result<Value, String> {
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
pub async fn get_trending(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
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
pub async fn get_continue_watching(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in".to_string())?;

    let user_id = viewer_id(&state, &token).await?;

    anilist_query(
        &state.http,
        &current_query(),
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    )
    .await
}

#[tauri::command]
pub async fn sync_progress(state: State<'_, AppState>, media_id: i64, ep_num: i64) -> Result<Value, String> {
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
pub async fn update_status(
    state: State<'_, AppState>,
    media_id: i64,
    status: String,
    entry_id: Option<i64>,
) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;

    if status == "Not in List" {
        // Removing needs the list entry's own id; no entry means nothing to remove.
        let Some(id) = entry_id.filter(|id| *id > 0) else {
            return Ok(serde_json::json!({ "deleted": false }));
        };
        let res = anilist_query(&state.http, DELETE_ENTRY_MUTATION, serde_json::json!({ "id": id }), Some(&token)).await?;
        if res["data"]["DeleteMediaListEntry"]["deleted"].as_bool() != Some(true) {
            return Err("AniList did not remove the entry".to_string());
        }
        return Ok(serde_json::json!({ "deleted": true }));
    }

    anilist_query(
        &state.http,
        UPDATE_STATUS_MUTATION,
        serde_json::json!({ "mediaId": media_id, "status": status }),
        Some(&token),
    )
    .await
}

#[tauri::command]
pub async fn get_viewer_info(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    const Q: &str = r#"query { Viewer { id name avatar { medium } } }"#;
    anilist_query(&state.http, Q, serde_json::json!({}), Some(&token)).await
}

#[tauri::command]
pub async fn get_popular_this_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
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
pub async fn get_upcoming_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
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
pub async fn get_all_time_popular(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
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
pub async fn get_planning(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    let user_id = viewer_id(&state, &token).await?;
    anilist_query(
        &state.http,
        &planning_query(),
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    ).await
}

// ─── Bulk List Management ────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_all_lists(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    let user_id = viewer_id(&state, &token).await?;
    anilist_query(
        &state.http,
        &all_lists_query(),
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    ).await
}

#[derive(Debug, Clone, Deserialize)]
pub struct BulkUpdateOp {
    #[serde(rename = "mediaId")]
    media_id: i64,
    progress: Option<i64>,
    status: Option<String>,
}

// Applies a batch of per-show progress/status updates sequentially (AniList has
// no batch mutation), returning per-item success/failure so the UI can report
// partial failures rather than aborting the whole batch on the first error.
#[tauri::command]
pub async fn bulk_update_entries(state: State<'_, AppState>, ops: Vec<BulkUpdateOp>) -> Result<Value, String> {
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
pub async fn get_airing_schedule(state: State<'_, AppState>, days_ahead: Option<i64>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;
    let days = days_ahead.unwrap_or(7);
    // A day of look-back so today's earlier episodes still show (as "Aired");
    // the calendar drops anything before local midnight itself.
    let start = now - 86_400;
    let end   = now + days * 86_400;

    // A week is well over one page (50) of episodes: keep paging, or the
    // calendar runs dry halfway through the week.
    let mut schedules = Vec::new();
    for page in 1..=AIRING_MAX_PAGES {
        let res = anilist_query(
            &state.http,
            AIRING_SCHEDULE_QUERY,
            serde_json::json!({ "airingAtGreater": start, "airingAtLesser": end, "page": page }),
            token.as_deref(),
        ).await?;
        let page_data = &res["data"]["Page"];
        if let Some(items) = page_data["airingSchedules"].as_array() {
            schedules.extend(items.iter().cloned());
        }
        if page_data["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
    }
    Ok(serde_json::json!({ "data": { "Page": { "airingSchedules": schedules } } }))
}

const AIRING_MAX_PAGES: i64 = 10;

// ─── User Stats ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_user_stats(state: State<'_, AppState>) -> Result<Value, String> {
    let token = state.config.lock().unwrap().anilist_token.clone();
    let token = token.ok_or("Not logged in")?;
    let user_id = viewer_id(&state, &token).await?;
    anilist_query(
        &state.http,
        USER_STATS_QUERY,
        serde_json::json!({ "userId": user_id }),
        Some(&token),
    ).await
}

// Best-effort genre lookup for a batch of media ids — public data, works logged
// out. Used to enrich local watch-history stats with a "top genres" breakdown.
#[tauri::command]
pub async fn get_media_genres(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Value, String> {
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
pub async fn advanced_search(
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
