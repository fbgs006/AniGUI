//! User settings: the `Config` shape, where it is stored, and the
//! `get_config`/`save_config` commands the Settings panel talks to.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::State;

use crate::bootstrap;
use crate::shell::{find_bash, system_bash_candidates};
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

/// How a setting is shown to, and accepted from, the Settings panel.
#[derive(Clone, Copy)]
enum Kind {
    /// Text where blank means "automatic": stored as unset, shown as "".
    Override,
    /// Text with a default.
    Text(&'static str),
    /// On/off with a default.
    Flag(bool),
}

struct Setting {
    key: &'static str,
    kind: Kind,
    /// Whether Settings may change it (`skip_auto_setup` is set by the setup screen).
    editable: bool,
}

const fn setting(key: &'static str, kind: Kind) -> Setting {
    Setting { key, kind, editable: true }
}

const DEFAULT_QUALITY: &str = "best";
const DEFAULT_DUB: bool = false;
const DEFAULT_AUTOPLAY_NEXT: bool = true;
const DEFAULT_FULLSCREEN: bool = true;

/// Every user setting, with its default.
///
/// Open/Closed: `get_config`/`save_config` are generic over this table, so a
/// new setting is one `Config` field plus one line here, with no change to
/// the commands. (`every_config_field_is_a_setting` fails if the line is forgotten.)
const SETTINGS: &[Setting] = &[
    setting("bash_path", Kind::Override),
    setting("anilist_token", Kind::Override),
    setting("download_dir", Kind::Override),
    setting("quality", Kind::Text(DEFAULT_QUALITY)),
    setting("theme", Kind::Text("coral")),
    setting("confirm_before_sync", Kind::Flag(true)),
    setting("auto_sync", Kind::Flag(false)),
    setting("dub", Kind::Flag(DEFAULT_DUB)),
    setting("autoplay_next", Kind::Flag(DEFAULT_AUTOPLAY_NEXT)),
    setting("fullscreen", Kind::Flag(DEFAULT_FULLSCREEN)),
    Setting { key: "skip_auto_setup", kind: Kind::Flag(false), editable: false },
];

impl Config {
    /// The bash to run ani-cli with: the user's override, else auto-detected.
    pub(crate) fn bash(&self) -> Option<String> {
        self.bash_path.clone().or_else(find_bash)
    }

    pub(crate) fn quality(&self) -> String {
        self.quality.clone().unwrap_or_else(|| DEFAULT_QUALITY.to_string())
    }

    pub(crate) fn dub(&self) -> bool {
        self.dub.unwrap_or(DEFAULT_DUB)
    }

    pub(crate) fn autoplay_next(&self) -> bool {
        self.autoplay_next.unwrap_or(DEFAULT_AUTOPLAY_NEXT)
    }

    pub(crate) fn fullscreen(&self) -> bool {
        self.fullscreen.unwrap_or(DEFAULT_FULLSCREEN)
    }
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
    let is_auto = cfg.bash_path.as_deref().is_some_and(|saved| {
        system_bash_candidates().iter().any(|c| c.eq_ignore_ascii_case(saved))
    });
    if is_auto {
        cfg.bash_path = None;
    }
    is_auto
}

/// The settings as the panel sees them, defaults filled in.
fn settings_view(cfg: &Config) -> Map<String, Value> {
    let stored = serde_json::to_value(cfg).unwrap_or_default();
    SETTINGS
        .iter()
        .map(|s| {
            let v = &stored[s.key];
            let shown = match s.kind {
                Kind::Override => json!(v.as_str().unwrap_or("")),
                Kind::Text(default) => json!(v.as_str().unwrap_or(default)),
                Kind::Flag(default) => json!(v.as_bool().unwrap_or(default)),
            };
            (s.key.to_string(), shown)
        })
        .collect()
}

/// `cfg` with the editable settings present (and well-typed) in `incoming`
/// applied. Anything else in `incoming` is ignored.
fn apply_settings(cfg: &Config, incoming: &Value) -> Config {
    let Ok(mut stored) = serde_json::to_value(cfg) else { return cfg.clone() };
    for s in SETTINGS.iter().filter(|s| s.editable) {
        let new = match s.kind {
            Kind::Override => incoming[s.key]
                .as_str()
                .map(|v| if v.is_empty() { Value::Null } else { json!(v) }),
            Kind::Text(_) => incoming[s.key].as_str().map(|v| json!(v)),
            Kind::Flag(_) => incoming[s.key].as_bool().map(Value::from),
        };
        if let Some(v) = new {
            stored[s.key] = v;
        }
    }
    serde_json::from_value(stored).unwrap_or_else(|_| cfg.clone())
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Value {
    let mut view = settings_view(&state.config.lock().unwrap());
    // `bash_path`/`download_dir` are the user's own overrides (empty = automatic).
    // The automatic values are sent separately so Settings can show them as a
    // hint without saving them back as if the user had chosen them.
    view.insert("detected_bash_path".into(), json!(find_bash().unwrap_or_default()));
    view.insert("default_download_dir".into(), json!(default_download_dir().to_string_lossy()));
    Value::Object(view)
}

#[tauri::command]
pub fn save_config(state: State<AppState>, config: Value) -> bool {
    let mut cfg = state.config.lock().unwrap();
    *cfg = apply_settings(&cfg, &config);
    save_config_to_disk(&state.config_path, &cfg);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_config_field_is_a_setting() {
        let fields = serde_json::to_value(Config::default()).unwrap();
        for field in fields.as_object().unwrap().keys() {
            assert!(SETTINGS.iter().any(|s| s.key == field), "`{field}` is missing from SETTINGS");
        }
    }

    #[test]
    fn view_fills_in_defaults() {
        let view = settings_view(&Config::default());
        assert_eq!(view["quality"], "best");
        assert_eq!(view["theme"], "coral");
        assert_eq!(view["bash_path"], "");
        assert_eq!(view["confirm_before_sync"], true);
        assert_eq!(view["auto_sync"], false);
        assert_eq!(view["skip_auto_setup"], false);
    }

    #[test]
    fn saving_blank_overrides_resets_them_to_automatic() {
        let cfg = Config { bash_path: Some("C:/bash.exe".into()), quality: Some("720".into()), ..Default::default() };
        let saved = apply_settings(&cfg, &json!({ "bash_path": "", "quality": "" }));
        assert_eq!(saved.bash_path, None);
        // A plain text setting keeps what was typed, even blank (as before).
        assert_eq!(saved.quality.as_deref(), Some(""));
    }

    #[test]
    fn saving_ignores_read_only_hints_and_wrong_types() {
        let cfg = Config { dub: Some(true), ..Default::default() };
        let saved = apply_settings(&cfg, &json!({
            "detected_bash_path": "C:/x.exe",
            "skip_auto_setup": true,
            "dub": "yes",
            "fullscreen": false,
        }));
        assert_eq!(saved.dub, Some(true));
        assert_eq!(saved.skip_auto_setup, None);
        assert_eq!(saved.fullscreen, Some(false));
    }

    #[test]
    fn accessors_use_the_same_defaults_as_the_panel() {
        let cfg = Config::default();
        let view = settings_view(&cfg);
        assert_eq!(view["quality"], cfg.quality());
        assert_eq!(view["dub"], cfg.dub());
        assert_eq!(view["autoplay_next"], cfg.autoplay_next());
        assert_eq!(view["fullscreen"], cfg.fullscreen());
    }
}
