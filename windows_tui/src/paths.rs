//! Locations of every file EasyAlias owns, plus shell detection.

use crate::*;

// Resolve the user's home directory without pulling in extra dependencies.
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "USERPROFILE/HOME could not be read.".to_string())
}

// All app-managed files live below ~/.easyalias.
pub(crate) fn app_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".easyalias"))
}

pub(crate) fn config_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("config.json"))
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

pub(crate) fn sun_location_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("sun-location.json"))
}

pub(crate) fn tui_settings_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("tui-settings.json"))
}

pub(crate) fn import_marker_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".cmd-import-v1"))
}

// First-run setup: create ~/.easyalias and the command bin directory. The bin
// directory is where Windows finds aliases once it is present in User PATH.
pub(crate) fn ensure_app_files() -> Result<(), String> {
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    let bin = command_dir()?;
    fs::create_dir_all(&bin)
        .map_err(|error| format!("{} could not be created: {}", bin.display(), error))?;

    Ok(())
}

// Shorten paths below HOME for display, e.g. C:\Users\Name\.easyalias -> ~/.easyalias.
pub(crate) fn display_home_path(path: PathBuf) -> Result<String, String> {
    let home = home_dir()?;
    if let Ok(stripped) = path.strip_prefix(&home) {
        return Ok(format!("~/{}", stripped.display()));
    }

    Ok(path.display().to_string())
}
