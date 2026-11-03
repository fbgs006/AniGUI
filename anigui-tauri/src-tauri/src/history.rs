//! Local, login-free records: the watch history and the "continue watching"
//! list (`history.json` / `in_progress.json`).

use serde_json::Value;

fn get_history_path() -> std::path::PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| dirs_next::home_dir().unwrap_or_default());
    base.join("anicli-gui").join("history.json")
}

/// Shows started but not finished (local "continue watching"), one entry per
/// anime, newest first. Written by the frontend after each episode.
fn get_in_progress_path() -> std::path::PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| dirs_next::home_dir().unwrap_or_default());
    base.join("anicli-gui").join("in_progress.json")
}

#[tauri::command]
pub fn append_history(entry: Value) -> bool {
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
pub fn get_history() -> Value {
    let path = get_history_path();
    if !path.exists() {
        return serde_json::json!([]);
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!([]))
}

const IN_PROGRESS_LIMIT: usize = 50;

pub(crate) fn read_in_progress() -> Vec<Value> {
    std::fs::read_to_string(get_in_progress_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_in_progress(list: &[Value]) -> bool {
    let path = get_in_progress_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    serde_json::to_string_pretty(list)
        .map(|data| std::fs::write(&path, data).is_ok())
        .unwrap_or(false)
}

/// Replaces `entry`'s anime in `list` and moves it to the front.
fn upsert_in_progress(mut list: Vec<Value>, entry: Value) -> Vec<Value> {
    let id = entry["animeId"].as_i64();
    list.retain(|e| e["animeId"].as_i64() != id);
    list.insert(0, entry);
    list.truncate(IN_PROGRESS_LIMIT);
    list
}

/// Where to resume `anime_id` episode `ep`, if it was left part-way through.
pub(crate) fn resume_position(list: &[Value], anime_id: i64, ep: i64) -> Option<f64> {
    list.iter()
        .find(|e| e["animeId"].as_i64() == Some(anime_id) && e["ep"].as_i64() == Some(ep))
        .and_then(|e| e["timePos"].as_f64())
        .filter(|t| *t > 10.0)
}

#[tauri::command]
pub fn get_in_progress() -> Value {
    Value::Array(read_in_progress())
}

#[tauri::command]
pub fn save_in_progress(entry: Value) -> bool {
    write_in_progress(&upsert_in_progress(read_in_progress(), entry))
}

#[tauri::command]
pub fn remove_in_progress(anime_id: i64) -> bool {
    let mut list = read_in_progress();
    list.retain(|e| e["animeId"].as_i64() != Some(anime_id));
    write_in_progress(&list)
}

#[tauri::command]
pub fn clear_history() -> bool {
    let path = get_history_path();
    let _ = std::fs::write(&path, "[]");
    true
}

#[tauri::command]
pub fn export_history_csv(path: String, csv: String) -> Result<Value, String> {
    // The path comes from a save dialog; refusing anything but .csv keeps this
    // from being a general "write any file" command.
    let is_csv = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if !is_csv {
        return Err("History can only be exported to a .csv file".to_string());
    }
    std::fs::write(&path, csv).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_progress_keeps_one_entry_per_anime_newest_first() {
        let list = vec![
            serde_json::json!({ "animeId": 1, "ep": 2, "timePos": 300.0 }),
            serde_json::json!({ "animeId": 2, "ep": 5, "timePos": 50.0 }),
        ];
        let list = upsert_in_progress(list, serde_json::json!({ "animeId": 2, "ep": 6, "timePos": 0.0 }));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["animeId"], 2);
        assert_eq!(list[0]["ep"], 6);
        assert_eq!(list[1]["animeId"], 1);
    }

    #[test]
    fn resumes_only_the_episode_left_part_way() {
        let list = vec![
            serde_json::json!({ "animeId": 1, "ep": 2, "timePos": 300.0 }),
            serde_json::json!({ "animeId": 2, "ep": 6, "timePos": 0.0 }),
        ];
        assert_eq!(resume_position(&list, 1, 2), Some(300.0));
        assert_eq!(resume_position(&list, 1, 3), None);
        assert_eq!(resume_position(&list, 2, 6), None);
        assert_eq!(resume_position(&list, 3, 1), None);
    }
}
