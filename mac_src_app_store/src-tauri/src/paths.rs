//! Locations of every file EasyAlias owns, plus shell detection.

use crate::*;

// Tauri resolves app_data_dir inside the macOS App Sandbox container. All
// structured data, bookmarks, and backups stay there.
pub(crate) fn app_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("The EasyAlias app data directory is unavailable: {error}"))
}

pub(crate) fn config_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("config.json"))
}

pub(crate) fn bookmark_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("home.bookmark"))
}

pub(crate) fn import_marker_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join(".shell-import-v2"))
}

pub(crate) fn legacy_bookmark_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("zshrc.bookmark"))
}

pub(crate) fn backups_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("backups"))
}

pub(crate) fn trash_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("trash.json"))
}

pub(crate) fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {error}", path.display())),
    }
}

pub(crate) fn shell_config_paths(home: &Path) -> Vec<PathBuf> {
    SHELL_STARTUP_FILES
        .iter()
        .map(|file_name| home.join(file_name))
        .collect()
}

pub(crate) fn display_shell_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("~/{name}"))
        .unwrap_or_else(|| path.display().to_string())
}
