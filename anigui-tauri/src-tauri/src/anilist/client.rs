//! Talking to AniList: the HTTP/GraphQL transport (retries, expired tokens)
//! and the cached viewer id. Knows nothing about which queries exist.

use serde_json::Value;

use crate::state::AppState;

const ANILIST_API: &str = "https://graphql.anilist.co";

/// An empty token is not authentication. Treat it as an anonymous request so
/// public discovery remains available for users who have not signed in.
fn usable_anilist_token(token: Option<&str>) -> Option<&str> {
    token.map(str::trim).filter(|token| !token.is_empty())
}

/// AniList answers an expired or revoked token with this error, even on
/// queries that need no login at all.
fn is_invalid_token_error(message: &str) -> bool {
    message.to_ascii_lowercase().contains("invalid token")
}

const EXPIRED_LOGIN: &str = "Your AniList login has expired — log in again in Settings.";

pub(crate) async fn anilist_query(
    client: &reqwest::Client,
    query: &str,
    variables: Value,
    token: Option<&str>,
) -> Result<Value, String> {
    match anilist_query_once(client, query, &variables, token).await {
        // A bad token must not take public data (Browse, Search, Calendar) down
        // with it: retry anonymously, and only report the expired login if the
        // query genuinely needed it.
        Err(e) if usable_anilist_token(token).is_some() && is_invalid_token_error(&e) => {
            anilist_query_once(client, query, &variables, None)
                .await
                .map_err(|_| EXPIRED_LOGIN.to_string())
        }
        other => other,
    }
}

async fn anilist_query_once(
    client: &reqwest::Client,
    query: &str,
    variables: &Value,
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

const VIEWER_QUERY: &str = "query { Viewer { id } }";

/// The logged-in user's id, looked up once per token: every list query needs
/// it, and asking AniList each time doubled the request count.
pub(crate) async fn viewer_id(state: &AppState, token: &str) -> Result<i64, String> {
    if let Some((cached_token, id)) = state.viewer.lock().unwrap().clone() {
        if cached_token == token {
            return Ok(id);
        }
    }
    let viewer = anilist_query(&state.http, VIEWER_QUERY, serde_json::json!({}), Some(token)).await?;
    let id = viewer["data"]["Viewer"]["id"].as_i64().ok_or("Could not get user ID")?;
    *state.viewer.lock().unwrap() = Some((token.to_string(), id));
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_anilist_invalid_token_errors() {
        assert!(is_invalid_token_error("AniList API: Invalid token"));
        assert!(is_invalid_token_error("invalid token"));
        assert!(!is_invalid_token_error("AniList API: rate limited (429)"));
    }

    #[test]
    fn treats_blank_anilist_tokens_as_anonymous_requests() {
        assert_eq!(usable_anilist_token(Some("   ")), None);
        assert_eq!(usable_anilist_token(None), None);
        assert_eq!(usable_anilist_token(Some(" token ")), Some("token"));
    }
}
