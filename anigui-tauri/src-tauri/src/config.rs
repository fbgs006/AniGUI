//! User settings: the `Config` shape, where it is stored, and the
//! `get_config`/`save_config` commands the Settings panel talks to.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::bootstrap;
use crate::shell::find_bash;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct Config {
    pub(crate) bash_path: Option<String>,
    pub(crate) quality: Option<String>,
    pub(crate) confirm_before_sync: Option<bool>,
    pub(crate) anilist_token: Option<String>,
    pub(crate) download_dir: Option<String>,
    pub(crate) theme: Option<String>,
    pub(crate) auto_sync: Option<bool>,
    pub(crate) dub: Option<bool>,
    pub(crate) autoplay_next: Option<bool>,
    pub(crate) fullscreen: Option<bool>,
    pub(crate) skip_auto_setup: Option<bool>,
}

pub(crate) fn get_config_path() -> std::path::PathBuf {
    let base = dirs_next::config_dir().unwrap_or_else(|| dirs_next::home_dir().unwrap_or_default());
    base.join("anicli-gui").join("config.json")
}

/// Where downloads go when the user hasn't picked a folder.
pub(crate) fn default_download_dir() -> std::path::PathBuf {
    dirs_next::download_dir().unwrap_or_default().join("AniGUI")
}

pub(crate) fn effective_download_dir(cfg: &Config) -> std::path::PathBuf {
    cfg.download_dir
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_download_dir)
}

pub(crate) fn load_config_from_disk(path: &std::path::Path) -> Config {
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(cfg) = serde_json::from_str(&data) {
                return cfg;
            }
        }
    }
    Config::default()
}

pub(crate) fn save_config_to_disk(path: &std::path::Path, cfg: &Config) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, data);
    }
}

/// Settings used to pre-fill the bash field with the auto-detected path, so
/// saving them stored it as a manual override — which then kept a system Git
/// Bash in use (without the bundled runtime on PATH) after AniGUI installed its
/// own. Drops such an override when it's just a standard Git install location.
pub(crate) fn clear_autosaved_bash_path(cfg: &mut Config) -> bool {
    if !bootstrap::bundled_bash_path().exists() {
        return false;
    }
    let auto_candidates = [
        which::which("bash").ok().map(|p| p.to_string_lossy().to_string()),
        Some(r"C:\Program Files\Git\bin\bash.exe".to_string()),
        Some(r"C:\Program Files (x86)\Git\bin\bash.exe".to_string()),
    ];
    let is_auto = cfg.bash_path.as_deref().is_some_and(|saved| {
        auto_candidates.iter().flatten().any(|c| c.eq_ignore_ascii_case(saved))
    });
    if is_auto {
        cfg.bash_path = None;
    }
    is_auto
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Value {
    let cfg = state.config.lock().unwrap();
    // `bash_path`/`download_dir` are the user's own overrides (empty = automatic).
    // The automatic values are sent separately so Settings can show them as a
    // hint without saving them back as if the user had chosen them.
    serde_json::json!({
        "bash_path": cfg.bash_path.clone().unwrap_or_default(),
        "detected_bash_path": find_bash().unwrap_or_default(),
        "default_download_dir": default_download_dir().to_string_lossy(),
        "quality": cfg.quality.clone().unwrap_or_else(|| "best".to_string()),
        "confirm_before_sync": cfg.confirm_before_sync.unwrap_or(true),
        "auto_sync": cfg.auto_sync.unwrap_or(false),
        "dub": cfg.dub.unwrap_or(false),
        "autoplay_next": cfg.autoplay_next.unwrap_or(true),
        "fullscreen": cfg.fullscreen.unwrap_or(true),
        "theme": cfg.theme.clone().unwrap_or_else(|| "coral".to_string()),
        "anilist_token": cfg.anilist_token.clone().unwrap_or_default(),
        "download_dir": cfg.download_dir.clone().unwrap_or_default(),
        "skip_auto_setup": cfg.skip_auto_setup.unwrap_or(false)
    })
}

#[tauri::command]
pub fn save_config(state: State<AppState>, config: Value) -> bool {
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
    if let Some(v) = config.get("autoplay_next").and_then(|v| v.as_bool()) {
        cfg.autoplay_next = Some(v);
    }
    if let Some(v) = config.get("fullscreen").and_then(|v| v.as_bool()) {
        cfg.fullscreen = Some(v);
    }
    save_config_to_disk(&state.config_path, &cfg);
    true
}
