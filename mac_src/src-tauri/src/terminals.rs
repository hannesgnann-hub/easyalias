//! Installed terminal discovery and opening an automation working directory.

use crate::*;

fn app_exists(bundle: &str) -> bool {
    let system = PathBuf::from("/Applications").join(bundle);
    let user = home_dir()
        .map(|home| home.join("Applications").join(bundle))
        .ok();
    system.exists() || user.is_some_and(|path| path.exists())
}

#[tauri::command]
pub(crate) fn list_terminal_apps() -> Vec<TerminalApp> {
    let mut apps = Vec::new();
    let candidates = [
        ("ghostty", "Ghostty", "Ghostty.app"),
        ("warp", "Warp", "Warp.app"),
        ("iterm", "iTerm2", "iTerm.app"),
        ("wezterm", "WezTerm", "WezTerm.app"),
        ("kitty", "Kitty", "kitty.app"),
        ("alacritty", "Alacritty", "Alacritty.app"),
        ("rio", "Rio", "Rio.app"),
        ("hyper", "Hyper", "Hyper.app"),
    ];
    for (id, name, bundle) in candidates {
        if app_exists(bundle) {
            apps.push(TerminalApp {
                id: id.into(),
                name: name.into(),
            });
        }
    }
    apps.push(TerminalApp {
        id: "terminal".into(),
        name: "Terminal".into(),
    });
    apps
}

#[tauri::command]
pub(crate) fn open_in_terminal(terminal_id: String, path: String) -> Result<(), String> {
    let directory = automation_working_directory(&path)?;
    let directory_text = directory.to_string_lossy().to_string();
    let (app_name, working_directory_flag) = match terminal_id.as_str() {
        "ghostty" => ("Ghostty", Some("--working-directory=")),
        "warp" => ("Warp", None),
        "iterm" => ("iTerm", None),
        "wezterm" => ("WezTerm", Some("--cwd=")),
        "kitty" => ("kitty", Some("--directory=")),
        "alacritty" => ("Alacritty", Some("--working-directory=")),
        "rio" => ("Rio", Some("--working-dir=")),
        "hyper" => ("Hyper", None),
        "terminal" => ("Terminal", None),
        _ => return Err("Unknown terminal application.".to_string()),
    };

    let mut command = Command::new("open");
    command.args(["-a", app_name]);
    if let Some(flag) = working_directory_flag {
        command
            .arg("--args")
            .arg(format!("{}{}", flag, directory_text));
    } else {
        command.arg(&directory);
    }
    command
        .spawn()
        .map_err(|error| format!("{} could not be opened: {}", app_name, error))?;
    Ok(())
}
