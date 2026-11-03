//! Version info and self-updates, for AniGUI itself and for ani-cli.

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::UpdaterExt;

use crate::bootstrap;
use crate::shell::{shell_command, spawn_bash_command};
use crate::state::AppState;

// ─── App Info ─────────────────────────────────────────────────────────────────

/// The Windows portable build is the same binary as the installer build, just
/// copied to a differently-named file in CI (see release.yml) — so the exe's
/// own filename is the only reliable signal for which one is currently running.
#[tauri::command]
pub fn get_app_info(app: AppHandle) -> Value {
    let version = app.package_info().version.to_string();
    let build_type = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .map(|name| if name.contains("portable") { "Portable" } else { "Installed" }.to_string())
        .unwrap_or_else(|| "Installed".to_string());
    serde_json::json!({ "version": version, "buildType": build_type })
}

// ─── Auto-Updater ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Value, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(serde_json::json!({
            "available": true,
            "version": update.version,
            "body": update.body.unwrap_or_default()
        })),
        Ok(None) => Ok(serde_json::json!({ "available": false })),
        Err(e) => Ok(serde_json::json!({ "available": false, "error": e.to_string() })),
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart();
    }
    Ok(())
}

// ─── Ani-CLI Version Check ──────────────────────────────────────────────────────

/// Parses the numeric part of an ani-cli version for safe comparison.
///
/// GitHub tags can omit trailing zeroes (`v5.1`) while local builds may include
/// a patch component (`5.1.1`). Missing numeric components compare as zero.
/// Unknown version formats return `None` so that the UI never falsely claims an
/// update is available.
fn parse_anicli_version(version: &str) -> Option<Vec<u64>> {
    let numeric_part = version.trim().trim_start_matches(['v', 'V']);
    let numeric_part = numeric_part.split(['-', '+']).next().unwrap_or_default();

    let parts: Option<Vec<u64>> = numeric_part
        .split('.')
        .map(|part| {
            (!part.is_empty())
                .then(|| part.parse::<u64>().ok())
                .flatten()
        })
        .collect();

    match parts {
        Some(parts) if !parts.is_empty() => Some(parts),
        _ => None,
    }
}

/// Returns true only when `candidate` is strictly newer than `installed`.
fn anicli_update_available(installed: &str, candidate: &str) -> bool {
    let (Some(installed), Some(candidate)) = (
        parse_anicli_version(installed),
        parse_anicli_version(candidate),
    ) else {
        return false;
    };

    let component_count = installed.len().max(candidate.len());
    for index in 0..component_count {
        let local = installed.get(index).copied().unwrap_or(0);
        let latest = candidate.get(index).copied().unwrap_or(0);
        match latest.cmp(&local) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }

    false
}

/// The version in `ani-cli -V` output ("ani-cli 4.9" or just "4.9"), or
/// `None` when ani-cli isn't installed.
fn parse_installed_version(output: &str) -> Option<String> {
    let version = output.trim().replace("ani-cli", "").replace('v', "").trim().to_string();
    (!version.is_empty() && version != "not-installed").then_some(version)
}

/// The version of a GitHub "latest release" response, if it has one.
fn parse_release_tag(release: &Value) -> Option<String> {
    let tag = release["tag_name"].as_str()?.replace('v', "");
    (!tag.is_empty()).then_some(tag)
}

/// What the ani-cli update banner shows.
fn version_report(installed: Option<&str>, latest: Option<&str>) -> Value {
    let Some(installed) = installed else {
        return serde_json::json!({
            "installed": false,
            "local_version": null,
            "latest_version": null,
            "update_available": false
        });
    };
    serde_json::json!({
        "installed": true,
        "local_version": installed,
        // Unknown latest (offline, rate-limited): don't nag the user.
        "latest_version": latest,
        // A different string is not necessarily a newer release: for example
        // a local 5.1.1 build is already ahead of GitHub's v5.1 tag.
        "update_available": latest.is_some_and(|l| anicli_update_available(installed, l))
    })
}

async fn latest_anicli_release(http: &reqwest::Client) -> Option<String> {
    let resp = http
        .get("https://api.github.com/repos/pystardust/ani-cli/releases/latest")
        .send()
        .await
        .ok()?;
    parse_release_tag(&resp.json::<Value>().await.ok()?)
}

#[tauri::command]
pub async fn check_anicli_version(state: State<'_, AppState>) -> Result<Value, String> {
    let bash = state.config.lock().unwrap().bash();
    let Some(bash_path) = bash else {
        return Err("bash not found".to_string());
    };

    let output = spawn_bash_command(&bash_path)
        .args(["-lc", &shell_command("ani-cli -V 2>/dev/null || echo 'not-installed'")])
        .output()
        .map_err(|e| format!("Failed to run ani-cli: {}", e))?;
    let installed = parse_installed_version(&String::from_utf8_lossy(&output.stdout));

    let latest = match &installed {
        Some(_) => latest_anicli_release(&state.http).await,
        None => None,
    };
    Ok(version_report(installed.as_deref(), latest.as_deref()))
}

#[tauri::command]
pub async fn update_anicli(state: State<'_, AppState>, app: AppHandle) -> Result<Value, String> {
    let cfg = state.config.lock().unwrap().clone();
    let bash = cfg.bash();

    let Some(bash_path) = bash else {
        return Err("bash not found".to_string());
    };

    // The bundled ani-cli is a raw script, not a git checkout, so its own
    // `-U` self-updater has nothing to pull — re-fetch it pinned to the
    // latest release tag instead, and keep runtime/version.json in sync.
    // On Linux the bash is the system's, so "bundled" means our ani-cli copy exists.
    let uses_bundled_anicli = if cfg!(windows) {
        bootstrap::is_bundled_bash(&bash_path)
    } else {
        bootstrap::bundled_anicli_path().exists()
    };
    if uses_bundled_anicli {
        let result = bootstrap::reinstall_anicli_only(&state.http).await;
        let success = result.is_ok();
        let _ = app.emit("anicli_updated", serde_json::json!({ "success": success }));
        return result;
    }

    let output = spawn_bash_command(&bash_path)
        .args(["-lc", &shell_command("ani-cli -U 2>&1")])
        .output()
        .map_err(|e| format!("Failed to update ani-cli: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    // Emit an event so the frontend can react
    let _ = app.emit("anicli_updated", serde_json::json!({ "success": success }));

    Ok(serde_json::json!({
        "success": success,
        "output": stdout,
        "stderr": stderr
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_installed_version() {
        assert_eq!(parse_installed_version("ani-cli 4.9\n").as_deref(), Some("4.9"));
        assert_eq!(parse_installed_version("v5.1.1").as_deref(), Some("5.1.1"));
        assert_eq!(parse_installed_version("not-installed\n"), None);
        assert_eq!(parse_installed_version("  "), None);
    }

    #[test]
    fn reads_the_release_tag() {
        assert_eq!(parse_release_tag(&serde_json::json!({ "tag_name": "v4.10" })).as_deref(), Some("4.10"));
        assert_eq!(parse_release_tag(&serde_json::json!({ "message": "API rate limit exceeded" })), None);
    }

    #[test]
    fn report_never_nags_without_a_known_latest_release() {
        let report = version_report(Some("4.9"), None);
        assert_eq!(report["installed"], true);
        assert_eq!(report["latest_version"], Value::Null);
        assert_eq!(report["update_available"], false);

        assert_eq!(version_report(Some("4.9"), Some("4.10"))["update_available"], true);
        assert_eq!(version_report(None, None)["installed"], false);
    }

    #[test]
    fn does_not_offer_an_older_release_as_an_update() {
        assert!(!anicli_update_available("5.1.1", "5.1"));
    }

    #[test]
    fn recognises_a_strictly_newer_release() {
        assert!(anicli_update_available("5.0", "v5.1"));
    }

    #[test]
    fn treats_missing_trailing_components_as_zero() {
        assert!(!anicli_update_available("5.1", "5.1.0"));
    }

    #[test]
    fn ignores_unparseable_versions() {
        assert!(!anicli_update_available("5.1", "latest"));
    }
}
