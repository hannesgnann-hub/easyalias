//! Locations of every file EasyAlias owns below ~/.easyalias, plus shell detection.

use crate::*;

// Resolve the user's home directory without pulling in extra dependencies.
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME could not be read.".to_string())
}

// macOS and editor terminals can use different shells while inheriting the same
// SHELL value. Connect both supported shells so a zsh login session and a Bash
// VS Code terminal see the same EasyAlias commands.
pub(crate) fn shell_setup() -> Result<ShellSetup, String> {
    let home = home_dir()?;

    Ok(ShellSetup {
        name: "zsh + Bash".to_string(),
        config_files: vec![
            home.join(".zshrc"),
            home.join(".bash_profile"),
            home.join(".bashrc"),
        ],
    })
}

// All app-managed files live below ~/.easyalias.
pub(crate) fn app_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".easyalias"))
}

pub(crate) fn config_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("config.json"))
}

pub(crate) fn aliases_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("aliases.zsh"))
}

pub(crate) fn trash_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("trash.json"))
}

pub(crate) fn automations_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("automations.json"))
}

pub(crate) fn automation_trash_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("automations-trash.json"))
}

pub(crate) fn timed_automations_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("timed-automations.json"))
}

pub(crate) fn timed_automation_log_dir() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("timed-automation-logs"))
}

pub(crate) fn sun_location_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("sun-location.json"))
}

pub(crate) fn tui_settings_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("tui-settings.json"))
}

pub(crate) fn import_marker_file(_setup: &ShellSetup) -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".shell-import-v3"))
}

// A missing startup file is a valid first-run state. Every other read error is
// surfaced so EasyAlias can never overwrite an unreadable startup file as empty.
pub(crate) fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {}", path.display(), error)),
    }
}

// Shorten paths below HOME for display, e.g. /Users/name/.easyalias -> ~/.easyalias.
pub(crate) fn display_home_path(path: PathBuf) -> Result<String, String> {
    let home = home_dir()?;
    if let Ok(stripped) = path.strip_prefix(&home) {
        return Ok(format!("~/{}", stripped.display()));
    }

    Ok(path.display().to_string())
}
