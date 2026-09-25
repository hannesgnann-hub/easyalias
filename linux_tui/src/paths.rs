//! Locations of every file EasyAlias owns, plus shell detection.

use crate::*;

// Resolve the user's home directory without pulling in an extra dependency.
pub(crate) fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME could not be read.".to_string())
}

// Linux desktop sessions normally expose the login shell in SHELL. Bash and
// zsh are supported directly; unknown or missing values use bash as a practical
// default because it is the most common interactive Linux shell.
pub(crate) fn shell_setup() -> Result<ShellSetup, String> {
    let shell = env::var("SHELL").unwrap_or_default();
    let shell_path = PathBuf::from(&shell);
    let shell_name = shell_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let (name, startup_file) = match shell_name {
        "zsh" => ("zsh", ".zshrc"),
        "bash" => ("bash", ".bashrc"),
        _ => ("bash", ".bashrc"),
    };

    Ok(ShellSetup {
        name: name.to_string(),
        config_file: home_dir()?.join(startup_file),
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
    Ok(app_dir()?.join("aliases.sh"))
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

pub(crate) fn systemd_user_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/systemd/user"))
}

pub(crate) fn sun_location_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("sun-location.json"))
}

pub(crate) fn tui_settings_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("tui-settings.json"))
}

pub(crate) fn import_marker_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".shell-import-v1"))
}

// A missing startup file is valid. Other read failures are surfaced so an
// unreadable shell configuration can never be overwritten as if it were empty.
pub(crate) fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {}", path.display(), error)),
    }
}

// Shorten paths below HOME for display, e.g. /home/name/.easyalias -> ~/.easyalias.
pub(crate) fn display_home_path(path: PathBuf) -> Result<String, String> {
    let home = home_dir()?;
    if let Ok(stripped) = path.strip_prefix(&home) {
        return Ok(format!("~/{}", stripped.display()));
    }

    Ok(path.display().to_string())
}
