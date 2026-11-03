//! Local, login-free records: the watch history and the "continue watching"
//! list (`history.json` / `in_progress.json`).

use std::path::PathBuf;

use serde_json::Value;

/// Where a list of JSON records is kept.
///
/// Dependency Inversion: the history logic below only knows this trait, not
/// the file system. The app uses `JsonFile`; tests use an in-memory store.
///
/// Contract every implementation keeps (Liskov: any store can stand in for
/// another; `list_store_contract` in the tests checks each one):
/// - nothing stored yet, or unreadable data, loads as an empty list;
/// - after `save` returns true, `load` returns exactly that list;
/// - `save` replaces the whole list, it never appends;
/// - neither method panics.
pub(crate) trait ListStore {
    fn load(&self) -> Vec<Value>;
    /// Returns whether the list was written.
    fn save(&self, list: &[Value]) -> bool;
}

/// A list stored as a pretty-printed JSON array in one file.
pub(crate) struct JsonFile(PathBuf);

impl ListStore for JsonFile {
    fn load(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.0)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self, list: &[Value]) -> bool {
        if let Some(parent) = self.0.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        serde_json::to_string_pretty(list)
            .map(|data| std::fs::write(&self.0, data).is_ok())
            .unwrap_or(false)
    }
}

fn app_file(name: &str) -> JsonFile {
    let base = dirs_next::config_dir().unwrap_or_else(|| dirs_next::home_dir().unwrap_or_default());
    JsonFile(base.join("anicli-gui").join(name))
}

/// Every episode played, newest first.
fn history_file() -> JsonFile {
    app_file("history.json")
}

/// Shows started but not finished (local "continue watching"), one entry per
/// anime, newest first. Written by the frontend after each episode.
fn in_progress_file() -> JsonFile {
    app_file("in_progress.json")
}

const HISTORY_LIMIT: usize = 200;
const IN_PROGRESS_LIMIT: usize = 50;

// ─── Commands (adapters) ─────────────────────────────────────────────────────

#[tauri::command]
pub fn append_history(entry: Value) -> bool {
    push_history(&history_file(), entry)
}

#[tauri::command]
pub fn get_history() -> Value {
    Value::Array(history_file().load())
}

#[tauri::command]
pub fn clear_history() -> bool {
    let _ = history_file().save(&[]);
    true
}

pub(crate) fn read_in_progress() -> Vec<Value> {
    in_progress_file().load()
}

#[tauri::command]
pub fn get_in_progress() -> Value {
    Value::Array(read_in_progress())
}

#[tauri::command]
pub fn save_in_progress(entry: Value) -> bool {
    let store = in_progress_file();
    store.save(&upsert_in_progress(store.load(), entry))
}

#[tauri::command]
pub fn remove_in_progress(anime_id: i64) -> bool {
    drop_in_progress(&in_progress_file(), anime_id)
}

// ─── Logic (depends only on ListStore) ───────────────────────────────────────

fn push_history(store: &impl ListStore, entry: Value) -> bool {
    let mut history = store.load();
    history.insert(0, entry);
    history.truncate(HISTORY_LIMIT);
    store.save(&history)
}

fn drop_in_progress(store: &impl ListStore, anime_id: i64) -> bool {
    let mut list = store.load();
    list.retain(|e| e["animeId"].as_i64() != Some(anime_id));
    store.save(&list)
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
    use std::cell::RefCell;

    #[derive(Default)]
    struct MemoryStore(RefCell<Vec<Value>>);

    impl ListStore for MemoryStore {
        fn load(&self) -> Vec<Value> {
            self.0.borrow().clone()
        }
        fn save(&self, list: &[Value]) -> bool {
            *self.0.borrow_mut() = list.to_vec();
            true
        }
    }

    #[test]
    fn history_is_newest_first_and_capped() {
        let store = MemoryStore::default();
        for i in 0..(HISTORY_LIMIT + 5) {
            push_history(&store, serde_json::json!({ "n": i }));
        }
        let history = store.load();
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(history[0]["n"], HISTORY_LIMIT + 4);
    }

    #[test]
    fn removing_a_show_keeps_the_others() {
        let store = MemoryStore::default();
        store.save(&[serde_json::json!({ "animeId": 1 }), serde_json::json!({ "animeId": 2 })]);
        drop_in_progress(&store, 1);
        assert_eq!(store.load(), vec![serde_json::json!({ "animeId": 2 })]);
    }

    /// The `ListStore` contract, checked the same way on every implementation.
    fn list_store_contract(store: &impl ListStore) {
        assert!(store.load().is_empty(), "a new store is empty");

        let two = vec![serde_json::json!({ "animeId": 1 }), serde_json::json!({ "animeId": 2 })];
        assert!(store.save(&two));
        assert_eq!(store.load(), two, "load returns what was saved");

        let one = vec![serde_json::json!({ "animeId": 3 })];
        assert!(store.save(&one));
        assert_eq!(store.load(), one, "save replaces, never appends");

        assert!(store.save(&[]));
        assert!(store.load().is_empty());
    }

    /// A file path no other test uses, under the system temp folder.
    fn temp_file(test: &str) -> (PathBuf, JsonFile) {
        let dir = std::env::temp_dir().join(format!("anigui-test-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = JsonFile(dir.join("sub").join("list.json"));
        (dir, file)
    }

    #[test]
    fn memory_store_keeps_the_contract() {
        list_store_contract(&MemoryStore::default());
    }

    #[test]
    fn json_file_keeps_the_contract() {
        // Its folder doesn't exist yet: the first save must create it.
        let (dir, file) = temp_file("contract");
        list_store_contract(&file);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn json_file_reads_a_corrupt_file_as_empty() {
        let (dir, file) = temp_file("corrupt");
        file.save(&[]);
        std::fs::write(&file.0, "{ not json").unwrap();
        assert!(file.load().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

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
