//! The state Tauri shares between every command.

use std::sync::{Arc, Mutex};

use crate::config::Config;

pub(crate) struct AppState {
    pub(crate) config: Mutex<Config>,
    pub(crate) config_path: std::path::PathBuf,
    pub(crate) player_active: Arc<Mutex<bool>>,
    /// PID of the running download's bash process, so it can be cancelled.
    pub(crate) download_pid: Arc<Mutex<Option<u32>>>,
    /// (token, viewer id) — see `viewer_id`.
    pub(crate) viewer: Mutex<Option<(String, i64)>>,
    pub(crate) http: reqwest::Client,
}
