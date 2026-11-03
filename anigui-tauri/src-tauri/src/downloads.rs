//! Downloading episodes with ani-cli, and managing the downloaded files.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::anicli::{AniCli, Mode};
use crate::config::effective_download_dir;
use crate::shell::kill_process_tree;
use crate::state::AppState;

/// ani-cli downloads with yt-dlp, or ffmpeg when yt-dlp is missing — neither
/// ships with AniGUI's runtime, so check before queueing a download that can
/// only fail.
fn downloader_available(anicli: &AniCli) -> bool {
    anicli
        .script("command -v yt-dlp >/dev/null || command -v ffmpeg >/dev/null")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

const NO_DOWNLOADER: &str = "Downloading needs yt-dlp or ffmpeg, and neither is installed. Install one (e.g. `winget install yt-dlp`) and try again.";

#[tauri::command]
pub fn start_download(state: State<AppState>, app: AppHandle, title: String, ep_num: i64) -> Value {
    let cfg = state.config.lock().unwrap().clone();
    let Some(anicli) = AniCli::from_config(&cfg) else {
        return serde_json::json!({ "error": "bash not found" });
    };

    let download_dir = effective_download_dir(&cfg);
    if let Err(e) = std::fs::create_dir_all(&download_dir) {
        return serde_json::json!({ "error": format!("Can't create the download folder: {e}") });
    }
    let download_pid = state.download_pid.clone();

    std::thread::spawn(move || {
        // Checked here, not before spawning: it runs bash, and a sync command
        // would block the UI thread meanwhile. `fatal` tells the queue not to retry.
        if !downloader_available(&anicli) {
            let _ = app.emit("download_finished", serde_json::json!({ "success": false, "fatal": true, "error": NO_DOWNLOADER }));
            return;
        }
        let mut child = match anicli
            .episode(&title, ep_num, Mode::Download { dir: &download_dir })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = app.emit("download_chunk", serde_json::json!({ "chunk": format!("Error: {}\n", e) }));
                let _ = app.emit("download_finished", serde_json::json!({ "success": false }));
                return;
            }
        };
        *download_pid.lock().unwrap() = Some(child.id());

        // Both pipes must be drained: ffmpeg writes its progress to stderr
        // non-stop, and an unread pipe fills up and freezes the download.
        let pump = |pipe: Option<Box<dyn std::io::Read + Send>>, app: AppHandle| {
            std::thread::spawn(move || {
                use std::io::Read;
                let Some(mut pipe) = pipe else { return };
                let mut buf = [0u8; 1024];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 { break; }
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app.emit("download_chunk", serde_json::json!({ "chunk": chunk }));
                }
            })
        };
        let out = pump(child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>), app.clone());
        let err = pump(child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>), app.clone());

        let status = child.wait();
        let _ = out.join();
        let _ = err.join();
        let cancelled = download_pid.lock().unwrap().take().is_none();
        let _ = app.emit("download_finished", serde_json::json!({
            "success": !cancelled && status.map(|s| s.success()).unwrap_or(false),
            "cancelled": cancelled
        }));
    });

    serde_json::json!({ "success": true })
}

/// Stops the running download, including the ani-cli/ffmpeg/yt-dlp processes
/// bash started.
#[tauri::command]
pub fn cancel_download(state: State<AppState>) -> bool {
    // Taking the pid marks the download as cancelled for start_download's thread.
    let Some(pid) = state.download_pid.lock().unwrap().take() else { return false };
    kill_process_tree(pid);
    true
}

#[tauri::command]
pub fn get_downloads(state: State<AppState>) -> Value {
    let cfg = state.config.lock().unwrap().clone();
    let download_dir = effective_download_dir(&cfg);
    let path = download_dir.as_path();
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

fn download_dir(state: &AppState) -> PathBuf {
    effective_download_dir(&state.config.lock().unwrap())
}

/// `path` if it's a video directly inside the download folder. The Offline
/// screen only ever passes those; anything else is refused rather than letting
/// the page open or delete arbitrary files.
fn downloaded_video(download_dir: &Path, path: &str) -> Result<PathBuf, String> {
    let dir = std::fs::canonicalize(download_dir).map_err(|e| e.to_string())?;
    let file = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let is_video = matches!(
        file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("mp4" | "mkv")
    );
    if file.parent() == Some(dir.as_path()) && is_video {
        Ok(file)
    } else {
        Err("Not a downloaded episode".to_string())
    }
}

#[tauri::command]
pub fn play_local_file(state: State<AppState>, path: String) -> Result<Value, String> {
    let path = downloaded_video(&download_dir(&state), &path)?;
    match open::that(&path) {
        Ok(_) => Ok(serde_json::json!({ "success": true })),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn delete_local_file(state: State<AppState>, path: String) -> Result<Value, String> {
    let path = downloaded_video(&download_dir(&state), &path)?;
    match std::fs::remove_file(&path) {
        Ok(_) => Ok(serde_json::json!({ "success": true })),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_videos_directly_in_the_download_folder_are_accepted() {
        let root = std::env::temp_dir().join(format!("anigui-test-{}-downloads", std::process::id()));
        let dir = root.join("AniGUI");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        for f in ["ep1.mkv", "EP2.MP4", "notes.txt", "nested/ep3.mkv"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        std::fs::write(root.join("outside.mkv"), "").unwrap();
        let check = |f: &Path| downloaded_video(&dir, f.to_str().unwrap()).is_ok();

        assert!(check(&dir.join("ep1.mkv")));
        assert!(check(&dir.join("EP2.MP4")));
        assert!(!check(&dir.join("notes.txt")), "not a video");
        assert!(!check(&dir.join("nested/ep3.mkv")), "not directly inside");
        assert!(!check(&root.join("outside.mkv")), "outside the folder");
        assert!(!check(&dir.join("..").join("outside.mkv")), "no escaping with ..");
        assert!(!check(&dir.join("missing.mkv")));

        let _ = std::fs::remove_dir_all(root);
    }
}
