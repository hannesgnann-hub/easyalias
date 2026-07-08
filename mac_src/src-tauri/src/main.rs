use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

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
    aliases_file: String,
    source_line: String,
    zshrc_source_present: bool,
}

// EasyAlias owns ~/.easyalias/aliases.zsh and only adds a source line to ~/.zshrc.
// This keeps the user's existing shell config mostly untouched.
const SOURCE_LINE: &str = "source ~/.easyalias/aliases.zsh";
const APP_ALIAS_NAME: &str = "easya";
const APP_ALIAS_LINE: &str = "alias easya='open /Applications/EasyAlias.app'";

// Resolve the user's home directory without pulling in extra dependencies.
fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME could not be read.".to_string())
}

// All app-managed files live below ~/.easyalias.
fn app_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".easyalias"))
}

fn config_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("config.json"))
}

fn aliases_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("aliases.zsh"))
}

fn zshrc_file() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".zshrc"))
}

// Used by the UI to show whether the shell is already wired up.
fn zshrc_source_present() -> bool {
    zshrc_file()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|content| content.lines().any(|line| line.trim() == SOURCE_LINE))
        .unwrap_or(false)
}

// First-run setup: create ~/.easyalias and an empty generated aliases.zsh.
// Creating aliases.zsh early prevents zsh from sourcing a missing file.
fn ensure_app_files() -> Result<(), String> {
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    let aliases_path = aliases_file()?;
    if !aliases_path.exists() {
        fs::write(&aliases_path, render_aliases(&[])?).map_err(|error| {
            format!("{} could not be created: {}", aliases_path.display(), error)
        })?;
    }

    Ok(())
}

// First-run shell setup. The app appends only the missing EasyAlias lines and
// avoids overwriting an existing easya alias.
fn ensure_zshrc_source() -> Result<(), String> {
    let path = zshrc_file()?;
    let content = fs::read_to_string(&path).unwrap_or_default();

    let source_present = content.lines().any(|line| line.trim() == SOURCE_LINE);
    let app_alias_present = content.lines().any(|line| {
        line.trim_start()
            .starts_with(&format!("alias {}=", APP_ALIAS_NAME))
    });

    if source_present && app_alias_present {
        return Ok(());
    }

    let mut next_content = content;
    if !next_content.is_empty() && !next_content.ends_with('\n') {
        next_content.push('\n');
    }

    if !source_present {
        next_content.push_str("\n# EasyAlias aliases\n");
        next_content.push_str(SOURCE_LINE);
        next_content.push('\n');
    }

    if !app_alias_present {
        next_content.push_str("\n# EasyAlias app shortcut\n");
        next_content.push_str(APP_ALIAS_LINE);
        next_content.push('\n');
    }

    fs::write(&path, next_content)
        .map_err(|error| format!("{} could not be updated: {}", path.display(), error))
}

// Shorten paths below HOME for display, e.g. /Users/name/.easyalias -> ~/.easyalias.
fn display_home_path(path: PathBuf) -> Result<String, String> {
    let home = home_dir()?;
    if let Ok(stripped) = path.strip_prefix(&home) {
        return Ok(format!("~/{}", stripped.display()));
    }

    Ok(path.display().to_string())
}

// Alias names become shell identifiers, so the accepted character set is strict.
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

// Wrap a zsh command in single quotes for an alias assignment.
// Embedded single quotes are escaped using the standard '\'' pattern.
fn single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

// Convert the structured alias list into the generated ~/.easyalias/aliases.zsh file.
// Validation is repeated here so invalid frontend data cannot produce a broken file.
fn render_aliases(aliases: &[AliasEntry]) -> Result<String, String> {
    let mut lines = vec![
        "# Generated by EasyAlias.".to_string(),
        "# Edit aliases in the app, not by hand.".to_string(),
        String::new(),
    ];

    for alias in aliases {
        if !validate_alias_name(&alias.name) {
            return Err(format!("Invalid alias name: {}", alias.name));
        }

        if alias.command_preview.trim().is_empty() {
            return Err(format!("Alias {} has no command.", alias.name));
        }

        lines.push(format!(
            "alias {}={}",
            alias.name,
            single_quote(&alias.command_preview)
        ));
    }

    Ok(format!("{}\n", lines.join("\n")))
}

// Build a complete AppState after load/save.
fn app_state(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        aliases_file: display_home_path(aliases_file()?)?,
        source_line: SOURCE_LINE.to_string(),
        zshrc_source_present: zshrc_source_present(),
    })
}

// Called by the frontend when the app starts.
// Also performs first-run file and .zshrc setup.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_zshrc_source()?;

    let path = config_file()?;

    if !path.exists() {
        return app_state(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;

    let aliases = serde_json::from_str::<Vec<AliasEntry>>(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {}", error))?;

    app_state(aliases)
}

// Called whenever aliases are created, edited, or deleted.
// Writes both config.json for the UI and aliases.zsh for zsh.
#[tauri::command]
fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    let config = serde_json::to_string_pretty(&aliases)
        .map_err(|error| format!("Aliases could not be serialized: {}", error))?;
    let aliases_zsh = render_aliases(&aliases)?;

    let config_path = config_file()?;
    let aliases_path = aliases_file()?;

    fs::write(&config_path, format!("{}\n", config))
        .map_err(|error| format!("{} could not be written: {}", config_path.display(), error))?;
    fs::write(&aliases_path, aliases_zsh)
        .map_err(|error| format!("{} could not be written: {}", aliases_path.display(), error))?;

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
