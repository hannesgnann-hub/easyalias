//! Installed terminal discovery and opening an automation working directory.

use crate::*;

fn command_exists(name: &str) -> bool {
    let extensions = ["", ".exe", ".cmd", ".bat"];
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|path| {
            extensions
                .iter()
                .any(|extension| path.join(format!("{}{}", name, extension)).is_file())
        })
    })
}

#[tauri::command]
pub(crate) fn list_terminal_apps() -> Vec<TerminalApp> {
    let mut apps = Vec::new();
    for (id, name, executable) in [
        ("windows-terminal", "Windows Terminal", "wt"),
        ("ghostty", "Ghostty", "ghostty"),
        ("wezterm", "WezTerm", "wezterm"),
        ("alacritty", "Alacritty", "alacritty"),
    ] {
        if command_exists(executable) {
            apps.push(TerminalApp {
                id: id.into(),
                name: name.into(),
            });
        }
    }
    apps.push(TerminalApp {
        id: "powershell".into(),
        name: "PowerShell".into(),
    });
    apps.push(TerminalApp {
        id: "cmd".into(),
        name: "Command Prompt".into(),
    });
    apps
}

#[tauri::command]
pub(crate) fn open_in_terminal(terminal_id: String, path: String) -> Result<(), String> {
    let directory = automation_working_directory(&path)?;
    let directory_text = directory.to_string_lossy().to_string();
    let mut command = match terminal_id.as_str() {
        "windows-terminal" => {
            let mut value = Command::new("wt.exe");
            value.args(["-d", &directory_text]);
            value
        }
        "ghostty" => {
            let mut value = Command::new("ghostty.exe");
            value.arg(format!("--working-directory={}", directory_text));
            value
        }
        "wezterm" => {
            let mut value = Command::new("wezterm.exe");
            value.args(["start", "--cwd", &directory_text]);
            value
        }
        "alacritty" => {
            let mut value = Command::new("alacritty.exe");
            value.args(["--working-directory", &directory_text]);
            value
        }
        "powershell" => {
            let mut value = Command::new("powershell.exe");
            value.args([
                "-NoExit",
                "-Command",
                "Set-Location",
                "-LiteralPath",
                &directory_text,
            ]);
            value
        }
        "cmd" => {
            let mut value = Command::new("cmd.exe");
            value.args(["/K", "cd", "/d", &directory_text]);
            value
        }
        _ => return Err("Unknown terminal application.".to_string()),
    };
    command
        .spawn()
        .map_err(|error| format!("Terminal could not be opened: {}", error))?;
    Ok(())
}
