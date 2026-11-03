//! Building `ani-cli` invocations. The one place that knows ani-cli's flags
//! and how data reaches its script safely.

use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::shell::{shell_command, spawn_bash_command};

/// What an ani-cli run needs from the settings, and nothing more.
///
/// Interface Segregation: playback and downloads get this, not the whole
/// `Config` (which also holds the AniList token, theme, etc.).
#[derive(Debug, Clone)]
pub(crate) struct AniCli {
    bash: String,
    quality: String,
    dub: bool,
}

/// Whether ani-cli streams the episode to mpv or saves it to a folder.
#[derive(Clone, Copy)]
pub(crate) enum Mode<'a> {
    Stream,
    Download { dir: &'a Path },
}

impl AniCli {
    /// `None` when no bash could be found.
    pub(crate) fn from_config(cfg: &Config) -> Option<Self> {
        Some(Self { bash: cfg.bash()?, quality: cfg.quality(), dub: cfg.dub() })
    }

    /// ani-cli for episode `ep` of `title`.
    ///
    /// The title, quality and folder reach bash as environment variables,
    /// never as script text, so quotes, `$` or backticks in a title can't
    /// break out of the command.
    pub(crate) fn episode(&self, title: &str, ep: i64, mode: Mode) -> Command {
        let dub = if self.dub { " --dub" } else { "" };
        let run = format!(r#"ani-cli "$ANIGUI_TITLE" -S 1 -e {ep} -q "$ANIGUI_QUALITY"{dub}"#);
        let script = match mode {
            Mode::Stream => format!("{run} --exit-after-play"),
            Mode::Download { .. } => format!(r#"cd "$ANIGUI_DL_DIR" && {run} -d"#),
        };

        let mut cmd = self.script(&script);
        cmd.env("ANIGUI_TITLE", title).env("ANIGUI_QUALITY", &self.quality);
        if let Mode::Download { dir } = mode {
            cmd.env("ANIGUI_DL_DIR", dir);
        }
        cmd
    }

    /// Any bash script, run the way ani-cli is (bundled runtime on PATH).
    pub(crate) fn script(&self, script: &str) -> Command {
        let mut cmd = spawn_bash_command(&self.bash);
        cmd.args(["-lc", &shell_command(script)]);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn anicli(dub: bool) -> AniCli {
        AniCli { bash: "bash".into(), quality: "720".into(), dub }
    }

    fn script_of(cmd: &Command) -> String {
        cmd.get_args().last().unwrap().to_string_lossy().into_owned()
    }

    fn env_of<'a>(cmd: &'a Command, key: &str) -> Option<&'a OsStr> {
        cmd.get_envs().find(|(k, _)| *k == key).and_then(|(_, v)| v)
    }

    #[test]
    fn streaming_exits_after_playing() {
        let cmd = anicli(false).episode("Frieren", 3, Mode::Stream);
        assert!(script_of(&cmd).ends_with(r#"ani-cli "$ANIGUI_TITLE" -S 1 -e 3 -q "$ANIGUI_QUALITY" --exit-after-play"#));
        assert_eq!(env_of(&cmd, "ANIGUI_QUALITY"), Some(OsStr::new("720")));
    }

    #[test]
    fn downloading_runs_in_the_download_folder() {
        let dir = Path::new("D:/Anime");
        let cmd = anicli(true).episode("Frieren", 3, Mode::Download { dir });
        let script = script_of(&cmd);
        assert!(script.contains(r#"cd "$ANIGUI_DL_DIR" && ani-cli"#));
        assert!(script.ends_with("--dub -d"));
        assert_eq!(env_of(&cmd, "ANIGUI_DL_DIR"), Some(dir.as_os_str()));
    }

    #[test]
    fn the_title_never_becomes_script_text() {
        let title = r#"Evil"; rm -rf ~; echo "$(whoami)`id`"#;
        let cmd = anicli(false).episode(title, 1, Mode::Stream);
        assert!(!script_of(&cmd).contains("rm -rf"));
        assert_eq!(env_of(&cmd, "ANIGUI_TITLE"), Some(OsStr::new(title)));
    }
}
