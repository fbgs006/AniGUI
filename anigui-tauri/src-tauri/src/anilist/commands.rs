//! The Tauri commands backed by AniList (browse, search, lists, sync, stats).
//!
//! Each command is a thin adapter: it reads the token from `AppState`, picks
//! the real `HttpAniList`, and hands over to a plain function that only knows
//! the `AniListApi` trait. Those functions hold the actual logic and are what
//! the tests below exercise, against `FakeAniList`.

use std::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::State;

use super::client::{viewer_id, AniListApi, HttpAniList};
use super::queries::{
    advanced_search_query, all_lists_query, all_time_popular_query, current_query,
    planning_query, search_query, season_query, trending_query,
    AIRING_SCHEDULE_QUERY, DELETE_ENTRY_MUTATION, MEDIA_GENRES_QUERY, UPDATE_MUTATION,
    UPDATE_STATUS_MUTATION, USER_STATS_QUERY,
};
use crate::state::AppState;

const ANILIST_CLIENT_ID: &str = "45898";
const NOT_LOGGED_IN: &str = "Not logged in";
const AIRING_MAX_PAGES: i64 = 10;
const VIEWER_INFO_QUERY: &str = r#"query { Viewer { id name avatar { medium } } }"#;

fn token(state: &AppState) -> Option<String> {
    state.config.lock().unwrap().anilist_token.clone()
}

fn require_token(state: &AppState) -> Result<String, String> {
    token(state).ok_or_else(|| NOT_LOGGED_IN.to_string())
}

// ─── Commands (adapters) ─────────────────────────────────────────────────────

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
    let vars = json!({ "search": query, "page": page.unwrap_or(1) });
    HttpAniList(&state.http).query(&search_query(), vars, token(&state).as_deref()).await
}

#[tauri::command]
pub async fn get_trending(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
    let vars = json!({ "page": page.unwrap_or(1) });
    HttpAniList(&state.http).query(&trending_query(), vars, token(&state).as_deref()).await
}

#[tauri::command]
pub async fn get_popular_this_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
    let vars = json!({ "season": season, "year": year, "page": page.unwrap_or(1) });
    HttpAniList(&state.http).query(&season_query(), vars, token(&state).as_deref()).await
}

#[tauri::command]
pub async fn get_upcoming_season(state: State<'_, AppState>, season: String, year: i64, page: Option<i64>) -> Result<Value, String> {
    let vars = json!({ "season": season, "year": year, "page": page.unwrap_or(1) });
    HttpAniList(&state.http).query(&season_query(), vars, token(&state).as_deref()).await
}

#[tauri::command]
pub async fn get_all_time_popular(state: State<'_, AppState>, page: Option<i64>) -> Result<Value, String> {
    let vars = json!({ "page": page.unwrap_or(1) });
    HttpAniList(&state.http).query(&all_time_popular_query(), vars, token(&state).as_deref()).await
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
    let vars = advanced_search_variables(search, genres, year, season, format, sort, page);
    HttpAniList(&state.http).query(&advanced_search_query(), vars, token(&state).as_deref()).await
}

#[tauri::command]
pub async fn get_viewer_info(state: State<'_, AppState>) -> Result<Value, String> {
    let token = require_token(&state)?;
    HttpAniList(&state.http).query(VIEWER_INFO_QUERY, json!({}), Some(&token)).await
}

#[tauri::command]
pub async fn get_continue_watching(state: State<'_, AppState>) -> Result<Value, String> {
    let token = require_token(&state)?;
    fetch_user_query(&HttpAniList(&state.http), &state.viewer, &token, &current_query()).await
}

#[tauri::command]
pub async fn get_planning(state: State<'_, AppState>) -> Result<Value, String> {
    let token = require_token(&state)?;
    fetch_user_query(&HttpAniList(&state.http), &state.viewer, &token, &planning_query()).await
}

/// No status filter: every list entry, for the bulk list-management screen.
#[tauri::command]
pub async fn get_all_lists(state: State<'_, AppState>) -> Result<Value, String> {
    let token = require_token(&state)?;
    fetch_user_query(&HttpAniList(&state.http), &state.viewer, &token, &all_lists_query()).await
}

#[tauri::command]
pub async fn get_user_stats(state: State<'_, AppState>) -> Result<Value, String> {
    let token = require_token(&state)?;
    fetch_user_query(&HttpAniList(&state.http), &state.viewer, &token, USER_STATS_QUERY).await
}

#[tauri::command]
pub async fn sync_progress(state: State<'_, AppState>, media_id: i64, ep_num: i64) -> Result<Value, String> {
    let token = require_token(&state)?;
    let vars = json!({ "mediaId": media_id, "progress": ep_num, "status": "CURRENT" });
    HttpAniList(&state.http).query(UPDATE_MUTATION, vars, Some(&token)).await
}

#[tauri::command]
pub async fn update_status(
    state: State<'_, AppState>,
    media_id: i64,
    status: String,
    entry_id: Option<i64>,
) -> Result<Value, String> {
    let token = require_token(&state)?;
    apply_status(&HttpAniList(&state.http), &token, media_id, &status, entry_id).await
}

#[derive(Debug, Clone, Deserialize)]
pub struct BulkUpdateOp {
    #[serde(rename = "mediaId")]
    media_id: i64,
    progress: Option<i64>,
    status: Option<String>,
}

#[tauri::command]
pub async fn bulk_update_entries(state: State<'_, AppState>, ops: Vec<BulkUpdateOp>) -> Result<Value, String> {
    let token = require_token(&state)?;
    Ok(apply_bulk_updates(&HttpAniList(&state.http), &token, &ops).await)
}

#[tauri::command]
pub async fn get_airing_schedule(state: State<'_, AppState>, days_ahead: Option<i64>) -> Result<Value, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as i64;
    let token = token(&state);
    fetch_airing_schedule(&HttpAniList(&state.http), token.as_deref(), now, days_ahead.unwrap_or(7)).await
}

/// Best-effort genre lookup for a batch of media ids — public data, works
/// logged out. Enriches local watch-history stats with a "top genres" breakdown.
#[tauri::command]
pub async fn get_media_genres(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Value, String> {
    let token = token(&state);
    fetch_media_genres(&HttpAniList(&state.http), token.as_deref(), &ids).await
}

// ─── Logic (depends only on AniListApi) ──────────────────────────────────────

/// Runs a query scoped to the logged-in user (`$userId`).
async fn fetch_user_query(
    api: &impl AniListApi,
    viewer_cache: &Mutex<Option<(String, i64)>>,
    token: &str,
    query: &str,
) -> Result<Value, String> {
    let user_id = viewer_id(api, viewer_cache, token).await?;
    api.query(query, json!({ "userId": user_id }), Some(token)).await
}

/// Sets a show's list status; `"Not in List"` removes the entry instead.
async fn apply_status(
    api: &impl AniListApi,
    token: &str,
    media_id: i64,
    status: &str,
    entry_id: Option<i64>,
) -> Result<Value, String> {
    if status == "Not in List" {
        // Removing needs the list entry's own id; no entry means nothing to remove.
        let Some(id) = entry_id.filter(|id| *id > 0) else {
            return Ok(json!({ "deleted": false }));
        };
        let res = api.query(DELETE_ENTRY_MUTATION, json!({ "id": id }), Some(token)).await?;
        if res["data"]["DeleteMediaListEntry"]["deleted"].as_bool() != Some(true) {
            return Err("AniList did not remove the entry".to_string());
        }
        return Ok(json!({ "deleted": true }));
    }

    api.query(UPDATE_STATUS_MUTATION, json!({ "mediaId": media_id, "status": status }), Some(token)).await
}

/// Applies per-show progress/status updates one by one (AniList has no batch
/// mutation), reporting each item's outcome so one failure doesn't abort the rest.
async fn apply_bulk_updates(api: &impl AniListApi, token: &str, ops: &[BulkUpdateOp]) -> Value {
    let mut results = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        if i > 0 {
            // Small spacing between mutations to stay well under AniList's rate limit.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }

        let outcome = if let Some(progress) = op.progress {
            let vars = json!({
                "mediaId": op.media_id,
                "progress": progress,
                "status": op.status.clone().unwrap_or_else(|| "COMPLETED".to_string()),
            });
            api.query(UPDATE_MUTATION, vars, Some(token)).await
        } else if let Some(status) = &op.status {
            api.query(UPDATE_STATUS_MUTATION, json!({ "mediaId": op.media_id, "status": status }), Some(token)).await
        } else {
            Err("op has neither progress nor status".to_string())
        };

        results.push(match outcome {
            Ok(_) => json!({ "mediaId": op.media_id, "ok": true }),
            Err(e) => json!({ "mediaId": op.media_id, "ok": false, "error": e }),
        });
    }
    json!({ "results": results })
}

/// Episodes airing from a day before `now` (so today's earlier ones still
/// show as "Aired") to `days` after it. `now` is a parameter, not read from
/// the clock, so tests can pin it.
async fn fetch_airing_schedule(
    api: &impl AniListApi,
    token: Option<&str>,
    now: i64,
    days: i64,
) -> Result<Value, String> {
    let start = now - 86_400;
    let end = now + days * 86_400;

    // A week is well over one page (50) of episodes: keep paging, or the
    // calendar runs dry halfway through the week.
    let mut schedules = Vec::new();
    for page in 1..=AIRING_MAX_PAGES {
        let vars = json!({ "airingAtGreater": start, "airingAtLesser": end, "page": page });
        let res = api.query(AIRING_SCHEDULE_QUERY, vars, token).await?;
        let page_data = &res["data"]["Page"];
        if let Some(items) = page_data["airingSchedules"].as_array() {
            schedules.extend(items.iter().cloned());
        }
        if page_data["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
    }
    Ok(json!({ "data": { "Page": { "airingSchedules": schedules } } }))
}

/// AniList takes at most 50 ids per page, so larger batches are split.
async fn fetch_media_genres(api: &impl AniListApi, token: Option<&str>, ids: &[i64]) -> Result<Value, String> {
    let mut all = Vec::new();
    for chunk in ids.chunks(50) {
        let res = api.query(MEDIA_GENRES_QUERY, json!({ "ids": chunk }), token).await?;
        if let Some(arr) = res["data"]["Page"]["media"].as_array() {
            all.extend(arr.clone());
        }
    }
    Ok(json!({ "media": all }))
}

/// Builds the advanced-search variables, leaving out empty filters (AniList
/// would otherwise filter on an empty value) and defaulting to trending.
fn advanced_search_variables(
    search: Option<String>,
    genres: Option<Vec<String>>,
    year: Option<i64>,
    season: Option<String>,
    format: Option<String>,
    sort: Option<Vec<String>>,
    page: Option<i64>,
) -> Value {
    let mut vars = serde_json::Map::new();
    if let Some(s) = search.filter(|s| !s.is_empty()) { vars.insert("search".into(), json!(s)); }
    if let Some(g) = genres.filter(|g| !g.is_empty()) { vars.insert("genres".into(), json!(g)); }
    if let Some(y) = year { vars.insert("year".into(), json!(y)); }
    if let Some(s) = season.filter(|s| !s.is_empty()) { vars.insert("season".into(), json!(s)); }
    if let Some(f) = format.filter(|f| !f.is_empty()) { vars.insert("format".into(), json!(f)); }
    let sort = sort.unwrap_or_else(|| vec!["TRENDING_DESC".to_string()]);
    vars.insert("sort".into(), json!(sort));
    vars.insert("page".into(), json!(page.unwrap_or(1)));
    Value::Object(vars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anilist::client::fake::FakeAniList;

    fn ok() -> Result<Value, String> {
        Ok(json!({ "data": {} }))
    }

    #[tokio::test]
    async fn removing_a_show_without_a_list_entry_sends_nothing() {
        let api = FakeAniList::new(|_, _| ok());
        let res = apply_status(&api, "tok", 1, "Not in List", None).await;
        assert_eq!(res, Ok(json!({ "deleted": false })));
        assert!(api.calls().is_empty());
    }

    #[tokio::test]
    async fn removing_a_show_deletes_its_list_entry() {
        let api = FakeAniList::new(|_, _| Ok(json!({ "data": { "DeleteMediaListEntry": { "deleted": true } } })));
        let res = apply_status(&api, "tok", 1, "Not in List", Some(99)).await;
        assert_eq!(res, Ok(json!({ "deleted": true })));
        let calls = api.calls();
        assert_eq!(calls[0].0, DELETE_ENTRY_MUTATION);
        assert_eq!(calls[0].1, json!({ "id": 99 }));
    }

    #[tokio::test]
    async fn a_refused_delete_is_an_error() {
        let api = FakeAniList::new(|_, _| Ok(json!({ "data": { "DeleteMediaListEntry": { "deleted": false } } })));
        assert!(apply_status(&api, "tok", 1, "Not in List", Some(99)).await.is_err());
    }

    #[tokio::test]
    async fn other_statuses_update_the_entry() {
        let api = FakeAniList::new(|_, _| ok());
        apply_status(&api, "tok", 7, "PAUSED", Some(99)).await.unwrap();
        let calls = api.calls();
        assert_eq!(calls[0].0, UPDATE_STATUS_MUTATION);
        assert_eq!(calls[0].1, json!({ "mediaId": 7, "status": "PAUSED" }));
        assert_eq!(calls[0].2.as_deref(), Some("tok"));
    }

    #[tokio::test]
    async fn bulk_updates_report_each_item_and_keep_going() {
        let api = FakeAniList::new(|_, vars| {
            if vars["mediaId"] == 2 { Err("boom".to_string()) } else { ok() }
        });
        let ops = vec![
            BulkUpdateOp { media_id: 1, progress: Some(12), status: None },
            BulkUpdateOp { media_id: 2, progress: None, status: Some("DROPPED".into()) },
            BulkUpdateOp { media_id: 3, progress: None, status: None },
        ];
        let res = apply_bulk_updates(&api, "tok", &ops).await;

        assert_eq!(res["results"][0], json!({ "mediaId": 1, "ok": true }));
        assert_eq!(res["results"][1], json!({ "mediaId": 2, "ok": false, "error": "boom" }));
        assert_eq!(res["results"][2]["ok"], false);
        // Progress without a status marks the show completed; op 3 never hits AniList.
        let calls = api.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1["status"], "COMPLETED");
    }

    #[tokio::test]
    async fn airing_schedule_pages_until_the_last_page() {
        let api = FakeAniList::new(|_, vars| {
            let page = vars["page"].as_i64().unwrap();
            Ok(json!({ "data": { "Page": {
                "pageInfo": { "hasNextPage": page < 3 },
                "airingSchedules": [{ "episode": page }],
            } } }))
        });
        let res = fetch_airing_schedule(&api, None, 1_000_000, 7).await.unwrap();

        assert_eq!(res["data"]["Page"]["airingSchedules"].as_array().unwrap().len(), 3);
        let first = &api.calls()[0].1;
        assert_eq!(first["airingAtGreater"], 1_000_000 - 86_400);
        assert_eq!(first["airingAtLesser"], 1_000_000 + 7 * 86_400);
    }

    #[tokio::test]
    async fn airing_schedule_stops_at_the_page_cap() {
        let api = FakeAniList::new(|_, _| Ok(json!({ "data": { "Page": {
            "pageInfo": { "hasNextPage": true }, "airingSchedules": [],
        } } })));
        fetch_airing_schedule(&api, None, 0, 7).await.unwrap();
        assert_eq!(api.calls().len() as i64, AIRING_MAX_PAGES);
    }

    #[tokio::test]
    async fn genre_lookup_splits_into_batches_of_fifty() {
        let api = FakeAniList::new(|_, vars| {
            let media: Vec<Value> = vars["ids"].as_array().unwrap().iter().map(|id| json!({ "id": id })).collect();
            Ok(json!({ "data": { "Page": { "media": media } } }))
        });
        let ids: Vec<i64> = (1..=120).collect();
        let res = fetch_media_genres(&api, None, &ids).await.unwrap();

        assert_eq!(api.calls().len(), 3);
        assert_eq!(res["media"].as_array().unwrap().len(), 120);
    }

    #[tokio::test]
    async fn user_queries_are_scoped_to_the_viewer() {
        let api = FakeAniList::new(|query, _| {
            if query.contains("Viewer") {
                Ok(json!({ "data": { "Viewer": { "id": 42 } } }))
            } else {
                ok()
            }
        });
        let cache = Mutex::new(None);
        fetch_user_query(&api, &cache, "tok", USER_STATS_QUERY).await.unwrap();

        let calls = api.calls();
        assert_eq!(calls[1].0, USER_STATS_QUERY);
        assert_eq!(calls[1].1, json!({ "userId": 42 }));
    }

    #[test]
    fn advanced_search_drops_empty_filters() {
        let vars = advanced_search_variables(
            Some(String::new()), Some(vec![]), Some(2024), Some("WINTER".into()), None, None, None,
        );
        assert_eq!(vars, json!({ "year": 2024, "season": "WINTER", "sort": ["TRENDING_DESC"], "page": 1 }));
    }
}
