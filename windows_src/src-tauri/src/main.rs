use serde::{Deserialize, Serialize};
use std::{collections::HashSet, env, fs, path::PathBuf, process::Command};

// Must match the frontend AliasEntry shape. serde's camelCase conversion keeps
// Rust idiomatic while still producing JSON fields like customCommand/createdAt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AliasEntry {
    id: String,
    name: String,
    path: String,
    action: String,
    custom_command: Option<String>,
    command_preview: String,
    created_at: String,
    updated_at: String,
}

// State returned to the frontend on load/save. Besides aliases, it contains
// display paths and setup status for the UI header.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppState {
    aliases: Vec<AliasEntry>,
    config_file: String,
    command_dir: String,
    path_entry: String,
    path_configured: bool,
}

const APP_ALIAS_NAME: &str = "easya";

// Resolve the user's home directory without pulling in extra dependencies.
fn home_dir() -> Result<PathBuf, String> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "USERPROFILE/HOME could not be read.".to_string())
}

// All app-managed files live below ~/.easyalias.
fn app_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".easyalias"))
}

fn config_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("config.json"))
}

fn command_dir() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("bin"))
}

fn command_file(name: &str) -> Result<PathBuf, String> {
    Ok(command_dir()?.join(format!("{}.cmd", name)))
}

// First-run setup: create ~/.easyalias and the command bin directory.
fn ensure_app_files() -> Result<(), String> {
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    let bin = command_dir()?;
    fs::create_dir_all(&bin)
        .map_err(|error| format!("{} could not be created: {}", bin.display(), error))?;

    Ok(())
}

// Shorten paths below HOME for display, e.g. C:\Users\Name\.easyalias -> ~/.easyalias.
fn display_home_path(path: PathBuf) -> Result<String, String> {
    let home = home_dir()?;
    if let Ok(stripped) = path.strip_prefix(&home) {
        return Ok(format!("~/{}", stripped.display()));
    }

    Ok(path.display().to_string())
}

// Alias names become command file names, so the accepted character set is strict.
fn validate_alias_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|char| char.is_ascii_alphanumeric() || char == '_' || char == '-')
}

fn normalize_path(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

fn path_contains_command_dir(path_value: &str) -> Result<bool, String> {
    let bin = command_dir()?;
    let needle = normalize_path(&bin.display().to_string());

    Ok(path_value
        .split(';')
        .any(|entry| normalize_path(entry) == needle))
}

fn parse_registry_path(stdout: &str) -> String {
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if !trimmed.to_ascii_lowercase().starts_with("path") {
            continue;
        }

        let Some(type_index) = trimmed.find("REG_") else {
            continue;
        };
        let value_with_type = &trimmed[type_index..];
        let Some(value_index) = value_with_type.find(|char: char| char.is_whitespace()) else {
            continue;
        };

        return value_with_type[value_index..].trim().to_string();
    }

    String::new()
}

fn user_path_value() -> String {
    if !cfg!(windows) {
        return env::var("PATH").unwrap_or_default();
    }

    let output = Command::new("reg")
        .args(["query", "HKCU\\Environment", "/v", "Path"])
        .output();

    output
        .ok()
        .filter(|result| result.status.success())
        .map(|result| String::from_utf8_lossy(&result.stdout).to_string())
        .map(|stdout| parse_registry_path(&stdout))
        .unwrap_or_default()
}

fn path_configured() -> bool {
    path_contains_command_dir(&user_path_value()).unwrap_or(false)
        || env::var("PATH")
            .ok()
            .and_then(|path| path_contains_command_dir(&path).ok())
            .unwrap_or(false)
}

fn persist_user_path(next_path: &str) -> Result<(), String> {
    if !cfg!(windows) {
        return Ok(());
    }

    // setx broadcasts the environment update to future processes. It has a
    // historical length limit, so fall back to the registry for unusually long
    // user PATH values rather than risking truncation.
    let result = if next_path.len() <= 1000 {
        Command::new("setx").args(["Path", next_path]).output()
    } else {
        Command::new("reg")
            .args([
                "add",
                "HKCU\\Environment",
                "/v",
                "Path",
                "/t",
                "REG_EXPAND_SZ",
                "/d",
                next_path,
                "/f",
            ])
            .output()
    };

    let output = result.map_err(|error| format!("User PATH could not be updated: {}", error))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(format!(
        "User PATH could not be updated: {}{}",
        stdout, stderr
    ))
}

fn ensure_path_contains_command_dir() -> Result<(), String> {
    let bin = command_dir()?;
    let bin_value = bin.display().to_string();
    let current_user_path = user_path_value();

    if path_contains_command_dir(&current_user_path)? {
        return Ok(());
    }

    let next_path = if current_user_path.trim().is_empty() {
        bin_value
    } else {
        format!("{};{}", current_user_path.trim_end_matches(';'), bin_value)
    };

    persist_user_path(&next_path)
}

fn escape_cmd_double_quoted(value: &str) -> String {
    value.replace('%', "%%").replace('"', "\"\"")
}

fn cmd_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed == "~" {
        return "\"%USERPROFILE%\"".to_string();
    }

    if trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
        let without_home = trimmed[2..].replace('/', "\\");
        return format!(
            "\"%USERPROFILE%\\{}\"",
            escape_cmd_double_quoted(&without_home)
        );
    }

    format!("\"{}\"", escape_cmd_double_quoted(trimmed))
}

fn build_command_preview(alias: &AliasEntry) -> String {
    let path = cmd_path(&alias.path);

    match alias.action.as_str() {
        "navigate" => {
            if path.is_empty() {
                String::new()
            } else {
                format!("cd /d {}", path)
            }
        }
        "open" => {
            if path.is_empty() {
                String::new()
            } else {
                format!("start \"\" {}", path)
            }
        }
        "execute" => {
            if path.is_empty() {
                String::new()
            } else {
                format!("call {} %*", path)
            }
        }
        "compile_gradle" => {
            if path.is_empty() {
                String::new()
            } else {
                format!("cd /d {} && call gradlew.bat build", path)
            }
        }
        "compile_maven" => {
            if path.is_empty() {
                String::new()
            } else {
                format!("cd /d {} && call mvn clean package", path)
            }
        }
        "custom" => alias
            .custom_command
            .as_deref()
            .unwrap_or(&alias.command_preview)
            .trim()
            .to_string(),
        _ => alias.command_preview.trim().to_string(),
    }
}

fn normalize_aliases(aliases: Vec<AliasEntry>) -> Vec<AliasEntry> {
    aliases
        .into_iter()
        .map(|mut alias| {
            alias.command_preview = build_command_preview(&alias);
            alias
        })
        .collect()
}

fn render_cmd_script(alias: &AliasEntry) -> Result<String, String> {
    if !validate_alias_name(&alias.name) {
        return Err(format!("Invalid alias name: {}", alias.name));
    }

    if alias.command_preview.trim().is_empty() {
        return Err(format!("Alias {} has no command.", alias.name));
    }

    Ok(format!("@echo off\r\n{}\r\n", alias.command_preview))
}

fn render_app_shortcut() -> String {
    [
        "@echo off",
        "if exist \"%LOCALAPPDATA%\\Programs\\EasyAlias\\EasyAlias.exe\" (",
        "  start \"\" \"%LOCALAPPDATA%\\Programs\\EasyAlias\\EasyAlias.exe\"",
        "  exit /b",
        ")",
        "if exist \"%ProgramFiles%\\EasyAlias\\EasyAlias.exe\" (",
        "  start \"\" \"%ProgramFiles%\\EasyAlias\\EasyAlias.exe\"",
        "  exit /b",
        ")",
        "start \"\" \"EasyAlias\"",
        "",
    ]
    .join("\r\n")
}

fn write_command_scripts(aliases: &[AliasEntry]) -> Result<(), String> {
    let bin = command_dir()?;
    fs::create_dir_all(&bin)
        .map_err(|error| format!("{} could not be created: {}", bin.display(), error))?;

    let mut expected_names = HashSet::new();
    for alias in aliases {
        expected_names.insert(alias.name.to_ascii_lowercase());
    }

    if let Ok(entries) = fs::read_dir(&bin) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_cmd = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("cmd"))
                .unwrap_or(false);
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.to_ascii_lowercase());

            if is_cmd
                && stem.as_deref() != Some(APP_ALIAS_NAME)
                && !stem.map_or(false, |name| expected_names.contains(&name))
            {
                fs::remove_file(&path).map_err(|error| {
                    format!("{} could not be removed: {}", path.display(), error)
                })?;
            }
        }
    }

    for alias in aliases {
        let script = render_cmd_script(alias)?;
        let path = command_file(&alias.name)?;
        fs::write(&path, script)
            .map_err(|error| format!("{} could not be written: {}", path.display(), error))?;
    }

    if !expected_names.contains(APP_ALIAS_NAME) {
        let shortcut = command_file(APP_ALIAS_NAME)?;
        if !shortcut.exists() {
            fs::write(&shortcut, render_app_shortcut()).map_err(|error| {
                format!("{} could not be written: {}", shortcut.display(), error)
            })?;
        }
    }

    Ok(())
}

// Build a complete AppState after load/save.
fn app_state(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        command_dir: display_home_path(command_dir()?)?,
        path_entry: command_dir()?.display().to_string(),
        path_configured: path_configured(),
    })
}

// Called by the frontend when the app starts.
// Also performs first-run file and User PATH setup.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_path_contains_command_dir()?;

    let path = config_file()?;

    if !path.exists() {
        write_command_scripts(&[])?;
        return app_state(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;

    let aliases = serde_json::from_str::<Vec<AliasEntry>>(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {}", error))?;
    let aliases = normalize_aliases(aliases);

    write_command_scripts(&aliases)?;
    app_state(aliases)
}

// Called whenever aliases are created, edited, or deleted.
// Writes config.json for the UI and one .cmd command file per alias.
#[tauri::command]
fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let aliases = normalize_aliases(aliases);
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    ensure_path_contains_command_dir()?;

    let config = serde_json::to_string_pretty(&aliases)
        .map_err(|error| format!("Aliases could not be serialized: {}", error))?;

    let config_path = config_file()?;

    fs::write(&config_path, format!("{}\n", config))
        .map_err(|error| format!("{} could not be written: {}", config_path.display(), error))?;

    write_command_scripts(&aliases)?;

    app_state(aliases)
}

fn main() {
    // Register native plugins before exposing commands to the frontend.
    // dialog = file/folder picker, opener = open GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![load_aliases, save_aliases])
        .run(tauri::generate_context!())
        .expect("error while running EasyAlias");
}
