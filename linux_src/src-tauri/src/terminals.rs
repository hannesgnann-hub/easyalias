//! Installed terminal discovery and opening an automation working directory.

use crate::*;

fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|path| path.join(name).is_file()))
}

#[tauri::command]
pub(crate) fn list_terminal_apps() -> Vec<TerminalApp> {
    [
        ("ghostty", "Ghostty", "ghostty"),
        ("wezterm", "WezTerm", "wezterm"),
        ("kitty", "Kitty", "kitty"),
        ("alacritty", "Alacritty", "alacritty"),
        ("gnome", "GNOME Terminal", "gnome-terminal"),
        ("konsole", "Konsole", "konsole"),
        ("xfce", "Xfce Terminal", "xfce4-terminal"),
        ("tilix", "Tilix", "tilix"),
        ("xterm", "XTerm", "xterm"),
    ]
    .into_iter()
    .filter(|(_, _, command)| command_exists(command))
    .map(|(id, name, _)| TerminalApp {
        id: id.into(),
        name: name.into(),
    })
    .collect()
}

#[tauri::command]
pub(crate) fn open_in_terminal(terminal_id: String, path: String) -> Result<(), String> {
    let directory = automation_working_directory(&path)?;
    let mut command = match terminal_id.as_str() {
        "ghostty" => {
            let mut value = Command::new("ghostty");
            value.arg(format!("--working-directory={}", directory.display()));
            value
        }
        "wezterm" => {
            let mut value = Command::new("wezterm");
            value.args(["start", "--cwd"]).arg(&directory);
            value
        }
        "kitty" => {
            let mut value = Command::new("kitty");
            value.arg("--directory").arg(&directory);
            value
        }
        "alacritty" => {
            let mut value = Command::new("alacritty");
            value.arg("--working-directory").arg(&directory);
            value
        }
        "gnome" => {
            let mut value = Command::new("gnome-terminal");
            value.arg(format!("--working-directory={}", directory.display()));
            value
        }
        "konsole" => {
            let mut value = Command::new("konsole");
            value.arg("--workdir").arg(&directory);
            value
        }
        "xfce" => {
            let mut value = Command::new("xfce4-terminal");
            value.arg(format!("--working-directory={}", directory.display()));
            value
        }
        "tilix" => {
            let mut value = Command::new("tilix");
            value.arg("--working-directory").arg(&directory);
            value
        }
        "xterm" => {
            let mut value = Command::new("xterm");
            value.current_dir(&directory);
            value
        }
        _ => return Err("Unknown terminal application.".to_string()),
    };
    command
        .spawn()
        .map_err(|error| format!("Terminal could not be opened: {}", error))?;
    Ok(())
}
