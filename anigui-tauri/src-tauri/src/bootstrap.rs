// ─── Runtime Bootstrap ─────────────────────────────────────────────────────
// Downloads and manages a self-contained Git-Bash + mpv + fzf + ani-cli
// runtime under %APPDATA%/AniGUI/runtime/, so end users never have to
// install anything themselves. A user-set `Config.bash_path` (Settings)
// always wins over the bundled runtime — see `find_bash()` in lib.rs.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

use crate::AppState;

// ─── Paths ───────────────────────────────────────────────────────────────────

pub fn runtime_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_next::home_dir().unwrap_or_default());
    base.join("AniGUI").join("runtime")
}

fn staging_dir() -> PathBuf {
    runtime_dir().join(".tmp")
}

fn version_file() -> PathBuf {
    runtime_dir().join("version.json")
}

pub fn bundled_bash_dir() -> PathBuf {
    runtime_dir().join("bash")
}

pub fn bundled_bash_path() -> PathBuf {
    bundled_bash_dir().join("bin").join("bash.exe")
}

pub fn bundled_mpv_dir() -> PathBuf {
    runtime_dir().join("mpv")
}

pub fn bundled_mpv_path() -> PathBuf {
    bundled_mpv_dir().join("mpv.exe")
}

pub fn bundled_fzf_dir() -> PathBuf {
    runtime_dir().join("fzf")
}

pub fn bundled_fzf_path() -> PathBuf {
    bundled_fzf_dir().join("fzf.exe")
}

pub fn bundled_anicli_dir() -> PathBuf {
    runtime_dir().join("ani-cli")
}

pub fn bundled_anicli_path() -> PathBuf {
    bundled_anicli_dir().join("ani-cli")
}

/// True when `path` is exactly the bundled bash executable — used to decide
/// whether a spawned process should get its PATH augmented with the rest of
/// the bundled runtime (mpv/fzf/ani-cli), or left alone for a system bash.
pub fn is_bundled_bash(path: &str) -> bool {
    let candidate = Path::new(path);
    let bundled = bundled_bash_path();
    match (std::fs::canonicalize(candidate), std::fs::canonicalize(&bundled)) {
        (Ok(a), Ok(b)) => a == b,
        _ => candidate == bundled,
    }
}

/// Prepends every bundled runtime component's directory (that actually
/// exists on disk) to `existing_path`, so a bundled-bash child process can
/// resolve `ani-cli`, `mpv`, `fzf`, `curl`, `grep`, `sed`, etc. by bare name
/// without any of them being on the *system* PATH.
pub fn build_augmented_path(existing_path: &str) -> String {
    let dirs = [
        bundled_bash_dir().join("usr").join("bin"),
        bundled_bash_dir().join("bin"),
        bundled_mpv_dir(),
        bundled_fzf_dir(),
        bundled_anicli_dir(),
    ];
    join_existing_dirs_with_path(&dirs, existing_path)
}

fn join_existing_dirs_with_path(dirs: &[PathBuf], existing_path: &str) -> String {
    let mut parts: Vec<String> = dirs
        .iter()
        .filter(|d| d.exists())
        .map(|d| d.to_string_lossy().to_string())
        .collect();
    parts.push(existing_path.to_string());
    parts.join(";")
}

fn to_posix_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        format!("/{}{}", drive, &s[2..])
    } else {
        s
    }
}

// ─── version.json ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeVersions {
    pub git_tag: Option<String>,
    pub mpv_tag: Option<String>,
    pub fzf_tag: Option<String>,
    pub anicli_tag: Option<String>,
}

fn load_versions() -> RuntimeVersions {
    std::fs::read_to_string(version_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_versions(v: &RuntimeVersions) {
    if let Some(parent) = version_file().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(v) {
        let _ = std::fs::write(version_file(), data);
    }
}

// ─── GitHub release/asset resolution ─────────────────────────────────────────
// Same "query the latest release, read tag_name" call `check_anicli_version`
// already makes against pystardust/ani-cli — generalized so git-for-windows,
// shinchiro's mpv builds, and fzf can all reuse it.

async fn latest_release_json(client: &reqwest::Client, owner_repo: &str) -> Result<Value, String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", owner_repo);
    client
        .get(&url)
        .header("User-Agent", "AniGUI")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Value>()
        .await
        .map_err(|e| e.to_string())
}

pub async fn latest_github_release_tag(client: &reqwest::Client, owner_repo: &str) -> Result<String, String> {
    let json = latest_release_json(client, owner_repo).await?;
    json["tag_name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("GitHub release response for {} is missing tag_name", owner_repo))
}

pub async fn latest_github_asset(
    client: &reqwest::Client,
    owner_repo: &str,
    name_matches: impl Fn(&str) -> bool,
) -> Result<(String, String), String> {
    let json = latest_release_json(client, owner_repo).await?;
    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| format!("GitHub release response for {} is missing tag_name", owner_repo))?
        .to_string();
    let asset_url = json["assets"]
        .as_array()
        .ok_or_else(|| format!("GitHub release response for {} is missing assets", owner_repo))?
        .iter()
        .find_map(|a| {
            let name = a["name"].as_str()?;
            if name_matches(name) {
                a["browser_download_url"].as_str().map(str::to_string)
            } else {
                None
            }
        })
        .ok_or_else(|| format!("no matching release asset found for {}", owner_repo))?;
    Ok((tag, asset_url))
}

// ─── Asset matchers (pure, unit-tested below) ────────────────────────────────

/// Matches the self-extracting portable Git-for-Windows archive. Deliberately
/// rejects `MinGit-*.zip` (no bash/coreutils) and the interactive
/// `Git-*-64-bit.exe` NSIS installer (wrong asset entirely).
pub fn is_portable_git_asset(name: &str) -> bool {
    name.starts_with("PortableGit-") && name.ends_with("-64-bit.7z.exe")
}

/// Matches a 64-bit mpv build from shinchiro/mpv-winbuild-cmake's releases.
/// Rejects 32-bit (`i686`) builds and the AVX2-optimized `v3` variant, which
/// isn't guaranteed to run on every CPU.
pub fn is_mpv_winbuild_asset(name: &str) -> bool {
    name.contains("mpv-x86_64") && !name.contains("x86_64-v3") && name.ends_with(".7z")
}

/// Matches the Windows amd64 fzf release zip, rejecting other platforms.
pub fn is_fzf_windows_asset(name: &str) -> bool {
    name.starts_with("fzf-") && name.ends_with("-windows_amd64.zip")
}

// ─── Download with progress ───────────────────────────────────────────────────

async fn download_with_progress(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    use futures_util::StreamExt;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    Ok(())
}

// ─── Extraction ────────────────────────────────────────────────────────────────

/// PortableGit ships as a self-extracting 7z SFX. Running it with `-y -o<dir>`
/// extracts silently with no UI and no separate archive crate needed.
fn extract_portable_git_sfx(sfx_path: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let dest_arg = format!("-o{}", dest.display());
    let status = std::process::Command::new(sfx_path)
        .args(["-y", &dest_arg])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("Git Bash self-extractor exited with {:?}", status.code()));
    }
    Ok(())
}

fn extract_7z(archive_path: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    sevenz_rust2::decompress_file(archive_path, dest).map_err(|e| e.to_string())
}

fn extract_zip(archive_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(relative) = entry.enclosed_name() else { continue };
        let out_path = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out_file = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out_file).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Recursively searches `dir` for a file named `filename` — used because
/// shinchiro's mpv 7z and fzf's zip may or may not wrap their contents in a
/// version-named subfolder; we don't want to hardcode a depth.
fn find_file(dir: &Path, filename: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, filename) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(filename) {
            return Some(path);
        }
    }
    None
}

/// Moves everything inside `found_file`'s parent directory into `dest`,
/// flattening away whatever wrapper folder the archive extracted into.
fn flatten_into(found_file: &Path, dest: &Path) -> Result<(), String> {
    let source_dir = found_file.parent().ok_or("extracted file has no parent directory")?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(source_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = dest.join(entry.file_name());
        std::fs::rename(entry.path(), target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ─── mpv Lua tracker install (bundled-runtime aware) ─────────────────────────
// Moved here from lib.rs: writes the resume-tracking Lua script into mpv's
// portable_config/scripts/ dir. When the bundled runtime's mpv exists, the
// layout is ours to control, so we skip the `which::which("mpv")` detection
// entirely and write straight into the known bundled path.

fn get_mpv_scripts_dir() -> Option<PathBuf> {
    if bundled_mpv_path().exists() {
        return Some(bundled_mpv_dir().join("portable_config").join("scripts"));
    }

    if let Ok(path) = which::which("mpv") {
        if let Some(parent) = path.parent() {
            let portable = parent.join("portable_config");
            if portable.exists() {
                return Some(portable.join("scripts"));
            }
        }
    }

    let mut path = dirs_next::data_dir()?;
    path.push("mpv");
    path.push("scripts");
    Some(path)
}

pub fn install_mpv_script() {
    let Some(path) = get_mpv_scripts_dir() else { return };
    let _ = std::fs::create_dir_all(&path);
    let script_path = path.join("anigui-tracker.lua");

    let script_content = r#"
local mp = require 'mp'
local utils = require 'mp.utils'
local msg = require 'mp.msg'

local appdata = os.getenv("APPDATA")
if not appdata then return end

local anigui_dir = appdata .. "/AniGUI"
-- Ensure directory exists. Wrap in pcall so sandboxed mpv builds (e.g. Scoop)
-- that block os.execute don't crash the entire script on startup.
pcall(function() os.execute('mkdir "' .. anigui_dir .. '" >nul 2>&1') end)
local timestamps_file = anigui_dir .. "/timestamps.json"
local last_watched_file = anigui_dir .. "/last_watched.json"

local function read_json()
    local f = io.open(timestamps_file, "r")
    if not f then return {} end
    local content = f:read("*all")
    f:close()
    if not content or content == "" then return {} end
    local data, err = utils.parse_json(content)
    if not data then return {} end
    return data
end

local function write_json(data)
    local f = io.open(timestamps_file, "w")
    if not f then return end
    f:write(utils.format_json(data))
    f:close()
end

mp.register_event("file-loaded", function()
    local title = mp.get_property("media-title")
    if not title then return end

    local data = read_json()
    if data[title] then
        local time = data[title]
        mp.commandv("seek", tostring(time), "absolute")
        mp.osd_message("AniGUI: Resumed at " .. tostring(math.floor(time)) .. "s")
    end
end)

mp.add_periodic_timer(5, function()
    local title = mp.get_property("media-title")
    local time = mp.get_property_number("time-pos")
    local duration = mp.get_property_number("duration")

    if title and time and duration then
        if time > 10 and (duration - time) > 10 then
            local data = read_json()
            data[title] = time
            write_json(data)
        elseif (duration - time) <= 10 then
            local data = read_json()
            if data[title] then
                data[title] = nil
                write_json(data)
            end
        end

        -- Track the last played stats so the backend knows if the episode was finished.
        local stats = {
            percent = time / duration,
            duration = duration,
            time = time
        }
        local stats_file = io.open(last_watched_file, "w")
        if stats_file then
            stats_file:write(utils.format_json(stats))
            stats_file:close()
        end
    end
end)
"#;
    let _ = std::fs::write(script_path, script_content);
}

// ─── Progress events ──────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct InstallProgress {
    component: &'static str,
    phase: &'static str,
    bytes: u64,
    total: u64,
    message: String,
}

fn emit_progress(app: &AppHandle, component: &'static str, phase: &'static str, bytes: u64, total: u64, message: impl Into<String>) {
    let _ = app.emit(
        "runtime_install_progress",
        InstallProgress { component, phase, bytes, total, message: message.into() },
    );
}

// ─── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn check_runtime_status(state: State<'_, AppState>) -> Value {
    let bash_present = bundled_bash_path().exists();
    let mpv_present = bundled_mpv_path().exists();
    let fzf_present = bundled_fzf_path().exists();
    let anicli_present = bundled_anicli_path().exists();
    let all_present = bash_present && mpv_present && fzf_present && anicli_present;
    let system_fallback_present = which::which("bash").is_ok() && which::which("mpv").is_ok();
    let skip_auto_setup = state.config.lock().unwrap().skip_auto_setup.unwrap_or(false);

    serde_json::json!({
        "bash_present": bash_present,
        "mpv_present": mpv_present,
        "fzf_present": fzf_present,
        "anicli_present": anicli_present,
        "all_present": all_present,
        "system_fallback_present": system_fallback_present,
        "skip_auto_setup": skip_auto_setup,
    })
}

#[tauri::command]
pub fn skip_runtime_setup(state: State<'_, AppState>) -> bool {
    let mut cfg = state.config.lock().unwrap();
    cfg.skip_auto_setup = Some(true);
    crate::save_config_to_disk(&state.config_path, &cfg);
    true
}

#[tauri::command]
pub async fn install_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<Value, String> {
    let force = force.unwrap_or(false);
    let client = state.http.clone();
    let mut versions = load_versions();

    std::fs::create_dir_all(staging_dir()).map_err(|e| e.to_string())?;

    // ── Git-Bash ─────────────────────────────────────────────────────────
    if force || !bundled_bash_path().exists() {
        emit_progress(&app, "git-bash", "downloading", 0, 0, "Fetching Git Bash…");
        let (tag, url) = latest_github_asset(&client, "git-for-windows/git", is_portable_git_asset)
            .await
            .inspect_err(|e| emit_progress(&app, "git-bash", "error", 0, 0, e))?;

        let archive = staging_dir().join("PortableGit.7z.exe");
        {
            let app2 = app.clone();
            download_with_progress(&client, &url, &archive, move |b, t| {
                emit_progress(&app2, "git-bash", "downloading", b, t, "Downloading Git Bash…");
            })
            .await
            .inspect_err(|e| emit_progress(&app, "git-bash", "error", 0, 0, e))?;
        }

        emit_progress(&app, "git-bash", "extracting", 0, 0, "Extracting Git Bash…");
        let _ = std::fs::remove_dir_all(bundled_bash_dir());
        extract_portable_git_sfx(&archive, &bundled_bash_dir())
            .inspect_err(|e| emit_progress(&app, "git-bash", "error", 0, 0, e))?;
        let _ = std::fs::remove_file(&archive);

        versions.git_tag = Some(tag);
        save_versions(&versions);
        emit_progress(&app, "git-bash", "done", 0, 0, "Git Bash ready");
    } else {
        emit_progress(&app, "git-bash", "done", 0, 0, "Already installed");
    }

    // ── mpv ──────────────────────────────────────────────────────────────
    if force || !bundled_mpv_path().exists() {
        emit_progress(&app, "mpv", "downloading", 0, 0, "Fetching mpv…");
        let (tag, url) = latest_github_asset(&client, "shinchiro/mpv-winbuild-cmake", is_mpv_winbuild_asset)
            .await
            .inspect_err(|e| emit_progress(&app, "mpv", "error", 0, 0, e))?;

        let archive = staging_dir().join("mpv.7z");
        {
            let app2 = app.clone();
            download_with_progress(&client, &url, &archive, move |b, t| {
                emit_progress(&app2, "mpv", "downloading", b, t, "Downloading mpv…");
            })
            .await
            .inspect_err(|e| emit_progress(&app, "mpv", "error", 0, 0, e))?;
        }

        emit_progress(&app, "mpv", "extracting", 0, 0, "Extracting mpv…");
        let extract_dest = staging_dir().join("mpv-extract");
        let _ = std::fs::remove_dir_all(&extract_dest);
        extract_7z(&archive, &extract_dest)
            .inspect_err(|e| emit_progress(&app, "mpv", "error", 0, 0, e))?;
        let mpv_exe = find_file(&extract_dest, "mpv.exe")
            .ok_or_else(|| "mpv.exe not found inside downloaded archive".to_string())
            .inspect_err(|e| emit_progress(&app, "mpv", "error", 0, 0, e))?;
        let _ = std::fs::remove_dir_all(bundled_mpv_dir());
        flatten_into(&mpv_exe, &bundled_mpv_dir())
            .inspect_err(|e| emit_progress(&app, "mpv", "error", 0, 0, e))?;
        let _ = std::fs::remove_file(&archive);
        let _ = std::fs::remove_dir_all(&extract_dest);

        install_mpv_script();

        versions.mpv_tag = Some(tag);
        save_versions(&versions);
        emit_progress(&app, "mpv", "done", 0, 0, "mpv ready");
    } else {
        emit_progress(&app, "mpv", "done", 0, 0, "Already installed");
    }

    // Re-written unconditionally: a fresh Git-Bash extraction wipes any
    // previous usr/bin/mpv shim, and a forced mpv-only reinstall needs it
    // repointed even when bash itself wasn't touched this run.
    write_mpv_shim().inspect_err(|e| emit_progress(&app, "mpv", "error", 0, 0, e))?;

    // ── fzf ──────────────────────────────────────────────────────────────
    if force || !bundled_fzf_path().exists() {
        emit_progress(&app, "fzf", "downloading", 0, 0, "Fetching fzf…");
        let (tag, url) = latest_github_asset(&client, "junegunn/fzf", is_fzf_windows_asset)
            .await
            .inspect_err(|e| emit_progress(&app, "fzf", "error", 0, 0, e))?;

        let archive = staging_dir().join("fzf.zip");
        {
            let app2 = app.clone();
            download_with_progress(&client, &url, &archive, move |b, t| {
                emit_progress(&app2, "fzf", "downloading", b, t, "Downloading fzf…");
            })
            .await
            .inspect_err(|e| emit_progress(&app, "fzf", "error", 0, 0, e))?;
        }

        emit_progress(&app, "fzf", "extracting", 0, 0, "Extracting fzf…");
        let extract_dest = staging_dir().join("fzf-extract");
        let _ = std::fs::remove_dir_all(&extract_dest);
        extract_zip(&archive, &extract_dest)
            .inspect_err(|e| emit_progress(&app, "fzf", "error", 0, 0, e))?;
        let fzf_exe = find_file(&extract_dest, "fzf.exe")
            .ok_or_else(|| "fzf.exe not found inside downloaded archive".to_string())
            .inspect_err(|e| emit_progress(&app, "fzf", "error", 0, 0, e))?;
        let _ = std::fs::remove_dir_all(bundled_fzf_dir());
        flatten_into(&fzf_exe, &bundled_fzf_dir())
            .inspect_err(|e| emit_progress(&app, "fzf", "error", 0, 0, e))?;
        let _ = std::fs::remove_file(&archive);
        let _ = std::fs::remove_dir_all(&extract_dest);

        versions.fzf_tag = Some(tag);
        save_versions(&versions);
        emit_progress(&app, "fzf", "done", 0, 0, "fzf ready");
    } else {
        emit_progress(&app, "fzf", "done", 0, 0, "Already installed");
    }

    // ── ani-cli ──────────────────────────────────────────────────────────
    if force || !bundled_anicli_path().exists() {
        emit_progress(&app, "ani-cli", "downloading", 0, 0, "Fetching ani-cli…");
        let tag = latest_github_release_tag(&client, "pystardust/ani-cli")
            .await
            .inspect_err(|e| emit_progress(&app, "ani-cli", "error", 0, 0, e))?;

        reinstall_anicli_only_with_tag(&client, &tag)
            .await
            .inspect_err(|e| emit_progress(&app, "ani-cli", "error", 0, 0, e))?;

        versions.anicli_tag = Some(tag);
        save_versions(&versions);
        emit_progress(&app, "ani-cli", "done", 0, 0, "ani-cli ready");
    } else {
        emit_progress(&app, "ani-cli", "done", 0, 0, "Already installed");
    }

    let _ = std::fs::remove_dir_all(staging_dir());

    Ok(serde_json::json!({ "success": true, "versions": versions }))
}

fn chmod_executable(bash_path: &Path, target: &Path) -> Result<(), String> {
    let posix_target = to_posix_path(target);
    let status = std::process::Command::new(bash_path)
        .args(["-lc", &format!("chmod +x '{}'", posix_target)])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("chmod +x on {} failed", target.display()));
    }
    Ok(())
}

/// Fetches the ani-cli script pinned to `tag` and marks it executable via the
/// bundled bash. Shared by `install_runtime` and `update_anicli`'s bundled path.
async fn reinstall_anicli_only_with_tag(client: &reqwest::Client, tag: &str) -> Result<(), String> {
    let url = format!("https://raw.githubusercontent.com/pystardust/ani-cli/{}/ani-cli", tag);
    let bytes = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let dest = bundled_anicli_path();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&dest, &bytes).map_err(|e| e.to_string())?;

    let bash_path = bundled_bash_path();
    if bash_path.exists() {
        chmod_executable(&bash_path, &dest)?;
    }

    Ok(())
}

/// Writes a tiny POSIX shim named `mpv` into the bundled bash's own
/// `usr/bin/` — the one directory Git-Bash's login profile reliably puts
/// first on PATH — that `exec`s the real bundled `mpv.exe` by absolute path.
///
/// This exists because some Scoop mpv installs add their own app directory
/// directly to the *system* PATH (not just via `scoop/shims`), and Git-Bash's
/// `.exe`-suffix resolution does not reliably respect PATH order against that
/// extra entry the way it does for extension-less names like `ani-cli`/`fzf`
/// — confirmed empirically, not just suspected. Routing through a bare-name
/// shim in the highest-priority directory sidesteps the unreliable `.exe`
/// lookup entirely instead of trying to out-order it.
fn write_mpv_shim() -> Result<(), String> {
    let bash_path = bundled_bash_path();
    if !bash_path.exists() {
        return Ok(());
    }
    let usr_bin = bundled_bash_dir().join("usr").join("bin");
    std::fs::create_dir_all(&usr_bin).map_err(|e| e.to_string())?;

    let shim_path = usr_bin.join("mpv");
    let target_posix = to_posix_path(&bundled_mpv_path());
    let content = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", target_posix);
    std::fs::write(&shim_path, content).map_err(|e| e.to_string())?;

    chmod_executable(&bash_path, &shim_path)
}

/// Re-fetches ani-cli pinned to the latest release tag, keeping `version.json`
/// in sync. Used by `update_anicli` in lib.rs when the bundled runtime is in
/// play, since a raw script isn't a git checkout `ani-cli -U` can update itself.
pub async fn reinstall_anicli_only(client: &reqwest::Client) -> Result<Value, String> {
    let tag = latest_github_release_tag(client, "pystardust/ani-cli").await?;
    reinstall_anicli_only_with_tag(client, &tag).await?;
    let mut versions = load_versions();
    versions.anicli_tag = Some(tag.clone());
    save_versions(&versions);
    Ok(serde_json::json!({ "success": true, "output": format!("Updated to {}", tag), "stderr": "" }))
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    #[test]
    fn matches_portable_git_asset_and_rejects_mingit_and_full_installer() {
        assert!(is_portable_git_asset("PortableGit-2.47.0-64-bit.7z.exe"));
        assert!(!is_portable_git_asset("MinGit-2.47.0-64-bit.zip"));
        assert!(!is_portable_git_asset("Git-2.47.0-64-bit.exe"));
        assert!(!is_portable_git_asset("PortableGit-2.47.0-32-bit.7z.exe"));
    }

    #[test]
    fn matches_mpv_winbuild_asset_and_rejects_32bit_and_v3() {
        assert!(is_mpv_winbuild_asset("mpv-x86_64-20250101-git-abcdef.7z"));
        assert!(!is_mpv_winbuild_asset("mpv-i686-20250101-git-abcdef.7z"));
        assert!(!is_mpv_winbuild_asset("mpv-x86_64-v3-20250101-git-abcdef.7z"));
        assert!(!is_mpv_winbuild_asset("mpv-x86_64-20250101-git-abcdef.7z.sha256"));
    }

    #[test]
    fn matches_fzf_windows_asset_and_rejects_other_platforms() {
        assert!(is_fzf_windows_asset("fzf-0.55.0-windows_amd64.zip"));
        assert!(!is_fzf_windows_asset("fzf-0.55.0-linux_amd64.tar.gz"));
        assert!(!is_fzf_windows_asset("fzf-0.55.0-windows_arm64.zip"));
    }

    #[test]
    fn augmented_path_prepends_bundled_dirs_and_keeps_existing_path() {
        // Dependency-injected, guaranteed-nonexistent fake dirs — real
        // machine state (e.g. an actually-installed runtime) must not
        // affect this test either way.
        let fake_dirs = [
            PathBuf::from(r"Z:\anigui-test-nonexistent\a"),
            PathBuf::from(r"Z:\anigui-test-nonexistent\b"),
        ];
        let existing = "C:\\Windows\\System32";
        let augmented = join_existing_dirs_with_path(&fake_dirs, existing);
        assert_eq!(augmented, existing);
    }

    #[test]
    fn augmented_path_prepends_only_dirs_that_exist() {
        let real_dir = std::env::temp_dir();
        let fake_dir = PathBuf::from(r"Z:\anigui-test-nonexistent\a");
        let existing = "C:\\Windows\\System32";
        let augmented = join_existing_dirs_with_path(&[fake_dir, real_dir.clone()], existing);
        assert_eq!(augmented, format!("{};{}", real_dir.to_string_lossy(), existing));
    }

    #[test]
    fn runtime_version_json_round_trips() {
        let v = RuntimeVersions {
            git_tag: Some("v2.47.0.windows.1".to_string()),
            mpv_tag: Some("20250101".to_string()),
            fzf_tag: Some("0.55.0".to_string()),
            anicli_tag: Some("v5.1".to_string()),
        };
        let json = serde_json::to_string(&v).unwrap();
        let round_tripped: RuntimeVersions = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.git_tag, v.git_tag);
        assert_eq!(round_tripped.mpv_tag, v.mpv_tag);
        assert_eq!(round_tripped.fzf_tag, v.fzf_tag);
        assert_eq!(round_tripped.anicli_tag, v.anicli_tag);
    }

    #[test]
    fn posix_path_converts_windows_drive_letter() {
        assert_eq!(to_posix_path(Path::new(r"C:\Users\pc\file.txt")), "/c/Users/pc/file.txt");
    }
}
