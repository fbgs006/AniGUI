//! Finding and launching bash, which runs ani-cli. Every subprocess the app
//! starts goes through here.

use crate::bootstrap;

pub(crate) fn find_bash() -> Option<String> {
    let bundled = bootstrap::bundled_bash_path();
    if bundled.exists() {
        return Some(bundled.to_string_lossy().to_string());
    }

    system_bash_candidates()
        .into_iter()
        .find(|path| std::path::Path::new(path).exists())
}

/// Where a system bash usually is: on PATH, or a standard Git for Windows install.
pub(crate) fn system_bash_candidates() -> Vec<String> {
    let on_path = which::which("bash").ok().map(|p| p.to_string_lossy().to_string());
    on_path
        .into_iter()
        .chain([
            r"C:\Program Files\Git\bin\bash.exe".to_string(),
            r"C:\Program Files (x86)\Git\bin\bash.exe".to_string(),
        ])
        .collect()
}

/// The `-lc` script for `cmd`, with the bundled runtime put on PATH first when
/// needed (see `bootstrap::shell_path_prefix` for why this can't be an env var).
pub(crate) fn shell_command(cmd: &str) -> String {
    format!("{}{}", bootstrap::shell_path_prefix(), cmd)
}

/// Wraps `Command::new(bash_path)`, augmenting the child process's PATH with
/// the bundled runtime's directories when `bash_path` is our own bundled
/// bash — so `ani-cli` (invoked by bare name in the shell command string)
/// can resolve `mpv`/`fzf`/`curl`/`grep`/`sed` without any of them being on
/// the *system* PATH. A user-set `bash_path` override is left untouched.
pub(crate) fn spawn_bash_command(bash_path: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(bash_path);
    if bootstrap::is_bundled_bash(bash_path) {
        let existing = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", bootstrap::build_augmented_path(&existing));
    }
    // bash.exe is a console-subsystem binary — spawning it from our windowed
    // app would otherwise pop a visible console window for every play/download/
    // version-check. CREATE_NO_WINDOW suppresses that; stdout/stderr piping
    // (where callers use it) is unaffected.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub(crate) fn kill_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill").args(["-TERM", "-P", &pid.to_string()]).status();
        let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
}
