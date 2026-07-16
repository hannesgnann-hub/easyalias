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

// State returned to the frontend on load/save. The shell details let the UI
// explain exactly which startup file EasyAlias connected on this Linux system.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppState {
    aliases: Vec<AliasEntry>,
    config_file: String,
    aliases_file: String,
    source_line: String,
    shell_name: String,
    shell_config_file: String,
    shell_source_present: bool,
}

// EasyAlias owns ~/.easyalias/aliases.sh and adds only these small integration
// lines to the active shell's startup file.
const SOURCE_LINE: &str = "source ~/.easyalias/aliases.sh";
const APP_ALIAS_NAME: &str = "easya";
const APP_ALIAS_LINE: &str = "alias easya='setsid -f easyalias >/dev/null 2>&1'";

#[derive(Debug)]
struct ShellSetup {
    name: String,
    config_file: PathBuf,
}

// Resolve the user's home directory without pulling in an extra dependency.
fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME could not be read.".to_string())
}

// Linux desktop sessions normally expose the login shell in SHELL. Bash and
// zsh are supported directly; unknown or missing values use bash as a practical
// default because it is the most common interactive Linux shell.
fn shell_setup() -> Result<ShellSetup, String> {
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
fn app_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".easyalias"))
}

fn config_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("config.json"))
}

fn aliases_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("aliases.sh"))
}

// Used by the UI to show whether the detected shell is already wired up.
fn shell_source_present(setup: &ShellSetup) -> bool {
    fs::read_to_string(&setup.config_file)
        .ok()
        .map(|content| content.lines().any(|line| line.trim() == SOURCE_LINE))
        .unwrap_or(false)
}

// First-run setup creates both the private app directory and an empty alias
// file, so a newly added source line can never point at a missing file.
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

// Append only missing EasyAlias lines. Existing shell configuration is kept
// byte-for-byte, apart from the new block at the end of the file.
fn ensure_shell_source(setup: &ShellSetup) -> Result<(), String> {
    let content = fs::read_to_string(&setup.config_file).unwrap_or_default();

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

    fs::write(&setup.config_file, next_content).map_err(|error| {
        format!(
            "{} could not be updated: {}",
            setup.config_file.display(),
            error
        )
    })
}

// Shorten paths below HOME for display, e.g. /home/name/.easyalias -> ~/.easyalias.
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

    chars.all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
}

// Wrap a command in single quotes for a bash/zsh alias assignment. Embedded
// single quotes use the standard portable '\'' shell escaping pattern.
fn single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

// Convert structured entries into a generated file understood by both bash and
// zsh. Validation is repeated here because frontend input is never trusted.
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

// Assemble the state returned after every load/save operation.
fn app_state(aliases: Vec<AliasEntry>, setup: &ShellSetup) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        aliases_file: display_home_path(aliases_file()?)?,
        source_line: SOURCE_LINE.to_string(),
        shell_name: setup.name.clone(),
        shell_config_file: display_home_path(setup.config_file.clone())?,
        shell_source_present: shell_source_present(setup),
    })
}

// Startup command: prepare the private files, connect the detected shell, and
// load persisted entries if config.json already exists.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;

    let path = config_file()?;
    if !path.exists() {
        return app_state(Vec::new(), &setup);
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let aliases = serde_json::from_str::<Vec<AliasEntry>>(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {}", error))?;

    app_state(aliases, &setup)
}

// Save command: persist structured JSON and regenerate the shell-owned output
// file as one operation from the frontend's current list.
#[tauri::command]
fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;

    let config = serde_json::to_string_pretty(&aliases)
        .map_err(|error| format!("Aliases could not be serialized: {}", error))?;
    let aliases_shell = render_aliases(&aliases)?;

    let config_path = config_file()?;
    let aliases_path = aliases_file()?;
    fs::write(&config_path, format!("{}\n", config))
        .map_err(|error| format!("{} could not be written: {}", config_path.display(), error))?;
    fs::write(&aliases_path, aliases_shell)
        .map_err(|error| format!("{} could not be written: {}", aliases_path.display(), error))?;

    app_state(aliases, &setup)
}

fn main() {
    // dialog = native file/folder picker; opener = GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![load_aliases, save_aliases])
        .run(tauri::generate_context!())
        .expect("error while running EasyAlias");
}
