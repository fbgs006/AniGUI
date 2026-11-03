//! The GraphQL documents sent to AniList. Pure data: no I/O here.

pub(crate) const MEDIA_FIELDS: &str = r#"
fragment mediaFields on Media {
  id
  idMal
  title { romaji english }
  format
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

/// A page of anime cards. Every discovery list (Browse, Search) is built here,
/// so each one gets the shared fields and the viewer's progress.
///
/// Open/Closed: a new list is a new call with its own `params`/`media_args`;
/// this builder never has to change for it.
fn media_page_query(params: &str, media_args: &str) -> String {
    format!(
        r#"{MEDIA_FIELDS} query ({params}) {{
  Page(page: $page, perPage: 20) {{
    pageInfo {{ hasNextPage }}
    media({media_args}) {{
      ...mediaFields
      mediaListEntry {{ id progress status }}
    }}
  }}
}}"#
    )
}

/// One of the logged-in user's lists; `None` returns every list.
fn user_list_query(status: Option<&str>) -> String {
    let filter = status.map(|s| format!(", status: {s}")).unwrap_or_default();
    format!(
        r#"{MEDIA_FIELDS} query ($userId: Int) {{
  MediaListCollection(userId: $userId, type: ANIME{filter}) {{
    lists {{
      entries {{
        id
        progress
        status
        media {{ ...mediaFields }}
      }}
    }}
  }}
}}"#
    )
}

pub(crate) fn advanced_search_query() -> String {
    media_page_query(
        "$search: String, $genres: [String], $year: Int, $season: MediaSeason, $format: MediaFormat, $sort: [MediaSort], $page: Int",
        "search: $search, genre_in: $genres, seasonYear: $year, season: $season, format: $format, type: ANIME, sort: $sort",
    )
}

pub(crate) fn search_query() -> String {
    media_page_query("$search: String, $page: Int", "search: $search, type: ANIME, sort: SEARCH_MATCH")
}

pub(crate) fn trending_query() -> String {
    media_page_query("$page: Int", "type: ANIME, sort: TRENDING_DESC")
}

/// Most popular shows of a season (this one, or an upcoming one).
pub(crate) fn season_query() -> String {
    media_page_query(
        "$season: MediaSeason, $year: Int, $page: Int",
        "type: ANIME, season: $season, seasonYear: $year, sort: POPULARITY_DESC",
    )
}

pub(crate) fn all_time_popular_query() -> String {
    media_page_query("$page: Int", "type: ANIME, sort: POPULARITY_DESC")
}

pub(crate) fn current_query() -> String {
    user_list_query(Some("CURRENT"))
}

pub(crate) fn planning_query() -> String {
    user_list_query(Some("PLANNING"))
}

/// Every list entry (Watching/Planning/Paused/Dropped/Completed/Rewatching).
pub(crate) fn all_lists_query() -> String {
    user_list_query(None)
}

pub(crate) const UPDATE_MUTATION: &str = r#"
mutation ($mediaId: Int, $progress: Int, $status: MediaListStatus) {
  SaveMediaListEntry(mediaId: $mediaId, progress: $progress, status: $status) {
    id progress status
  }
}"#;

pub(crate) const UPDATE_STATUS_MUTATION: &str = r#"
mutation ($mediaId: Int, $status: MediaListStatus) {
  SaveMediaListEntry(mediaId: $mediaId, status: $status) {
    id progress status
  }
}"#;

pub(crate) const DELETE_ENTRY_MUTATION: &str = r#"
mutation ($id: Int) {
  DeleteMediaListEntry(id: $id) { deleted }
}"#;

pub(crate) const MEDIA_GENRES_QUERY: &str = r#"
query ($ids: [Int]) {
  Page(page: 1, perPage: 50) {
    media(id_in: $ids, type: ANIME) {
      id
      genres
    }
  }
}"#;

pub(crate) const AIRING_SCHEDULE_QUERY: &str = r#"
query ($airingAtGreater: Int, $airingAtLesser: Int, $page: Int) {
  Page(page: $page, perPage: 50) {
    pageInfo { hasNextPage }
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

pub(crate) const USER_STATS_QUERY: &str = r#"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_fields_fragment_requests_format() {
        // Cards show TV/Movie/OVA and Browse filters by it.
        assert!(MEDIA_FIELDS.contains("format"));
    }

    #[test]
    fn airing_schedule_query_reports_more_pages() {
        // get_airing_schedule pages until hasNextPage is false.
        assert!(AIRING_SCHEDULE_QUERY.contains("hasNextPage"));
    }

    #[test]
    fn discovery_queries_request_viewer_progress_when_logged_in() {
        // AniList returns `mediaListEntry` as null for anonymous requests, so
        // requesting it is safe for logged-out users while still letting
        // logged-in users see their real progress on Browse/Search results.
        let queries = [
            advanced_search_query(),
            all_time_popular_query(),
            season_query(),
            search_query(),
            trending_query(),
            AIRING_SCHEDULE_QUERY.to_string(),
        ];

        for query in queries {
            assert!(query.contains("mediaListEntry"));
        }
    }

    #[test]
    fn user_lists_filter_by_status_only_when_asked() {
        assert!(current_query().contains("type: ANIME, status: CURRENT)"));
        assert!(planning_query().contains("status: PLANNING"));
        assert!(all_lists_query().contains("type: ANIME)"));
    }

    #[test]
    fn media_fields_fragment_requests_next_airing_episode() {
        // Powers the "next episode airs in..." line on the detail page for
        // RELEASING shows — every query built on the shared fragment gets it.
        assert!(MEDIA_FIELDS.contains("nextAiringEpisode"));
    }
}
