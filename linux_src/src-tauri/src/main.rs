use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShellAliasCandidate {
    id: String,
    name: String,
    command: String,
    line_number: usize,
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
    import_candidates: Vec<ShellAliasCandidate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    state: AppState,
    imported_count: usize,
    backup_file: String,
}

// EasyAlias owns ~/.easyalias/aliases.sh and adds only these small integration
// lines to the active shell's startup file.
const SOURCE_LINE: &str = "source ~/.easyalias/aliases.sh";
const APP_ALIAS_NAME: &str = "easya";
const APP_ALIAS_LINE: &str = "alias easya='setsid -f easyalias >/dev/null 2>&1'";
const IMPORT_MARKER_CONTENT: &str = "shell alias import prompt handled\n";

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

fn import_marker_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".shell-import-v1"))
}

// A missing startup file is valid. Other read failures are surfaced so an
// unreadable shell configuration can never be overwritten as if it were empty.
fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {}", path.display(), error)),
    }
}

fn decode_alias_value(value: &str) -> Option<String> {
    #[derive(Clone, Copy, PartialEq)]
    enum QuoteMode {
        Unquoted,
        Single,
        Double,
    }

    let mut chars = value.chars().peekable();
    let mut mode = QuoteMode::Unquoted;
    let mut decoded = String::new();

    while let Some(character) = chars.next() {
        match mode {
            QuoteMode::Unquoted => match character {
                '\'' => mode = QuoteMode::Single,
                '"' => mode = QuoteMode::Double,
                '\\' => decoded.push(chars.next()?),
                character if character.is_whitespace() => {
                    let remainder: String = chars.collect();
                    let remainder = remainder.trim_start();
                    if !remainder.is_empty() && !remainder.starts_with('#') {
                        return None;
                    }
                    break;
                }
                _ => decoded.push(character),
            },
            QuoteMode::Single => {
                if character == '\'' {
                    mode = QuoteMode::Unquoted;
                } else {
                    decoded.push(character);
                }
            }
            QuoteMode::Double => match character {
                '"' => mode = QuoteMode::Unquoted,
                '\\' => {
                    let escaped = chars.next()?;
                    if matches!(escaped, '\\' | '$' | '`' | '"' | '\n') {
                        decoded.push(escaped);
                    } else {
                        decoded.push('\\');
                        decoded.push(escaped);
                    }
                }
                _ => decoded.push(character),
            },
        }
    }

    if mode != QuoteMode::Unquoted || decoded.trim().is_empty() {
        return None;
    }

    Some(decoded)
}

// Only unindented, one-line aliases with one assignment are movable without
// interpreting arbitrary shell syntax or changing conditional behavior.
fn parse_shell_alias_line(line: &str, line_number: usize) -> Option<ShellAliasCandidate> {
    if line.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }

    let after_alias = line.strip_prefix("alias")?;
    if !after_alias.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }

    let assignment = after_alias.trim_start();
    if assignment.starts_with('-') {
        return None;
    }

    let equals_index = assignment.find('=')?;
    let name = assignment[..equals_index].trim();
    if !validate_alias_name(name) || name == APP_ALIAS_NAME {
        return None;
    }

    let command = decode_alias_value(assignment[equals_index + 1..].trim_start())?;
    Some(ShellAliasCandidate {
        id: format!("shell-line-{}", line_number),
        name: name.to_string(),
        command,
        line_number,
    })
}

fn find_shell_aliases(content: &str) -> Vec<ShellAliasCandidate> {
    let parsed: Vec<ShellAliasCandidate> = content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| parse_shell_alias_line(line, index + 1))
        .collect();
    let mut name_counts: HashMap<String, usize> = HashMap::new();

    for candidate in &parsed {
        *name_counts.entry(candidate.name.clone()).or_default() += 1;
    }

    parsed
        .into_iter()
        .filter(|candidate| name_counts.get(&candidate.name) == Some(&1))
        .collect()
}

fn scan_shell_aliases(setup: &ShellSetup) -> Result<Vec<ShellAliasCandidate>, String> {
    Ok(find_shell_aliases(&read_text_or_empty(&setup.config_file)?))
}

fn mark_import_handled() -> Result<(), String> {
    let path = import_marker_file()?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("System time could not be read: {}", error))
}

fn next_shell_backup_file(setup: &ShellSetup) -> Result<PathBuf, String> {
    let file_name = setup
        .config_file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(".shellrc");
    let timestamp = unix_timestamp()?;

    for suffix in 0..1000 {
        let backup_name = if suffix == 0 {
            format!("{}.easyalias-backup-{}", file_name, timestamp)
        } else {
            format!("{}.easyalias-backup-{}-{}", file_name, timestamp, suffix)
        };
        let candidate = home_dir()?.join(backup_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("A unique shell configuration backup name could not be created.".to_string())
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
    let content = read_text_or_empty(&setup.config_file)?;

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
fn app_state(
    aliases: Vec<AliasEntry>,
    setup: &ShellSetup,
    import_candidates: Vec<ShellAliasCandidate>,
) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        aliases_file: display_home_path(aliases_file()?)?,
        source_line: SOURCE_LINE.to_string(),
        shell_name: setup.name.clone(),
        shell_config_file: display_home_path(setup.config_file.clone())?,
        shell_source_present: shell_source_present(setup),
        import_candidates,
    })
}

fn load_config_aliases() -> Result<Vec<AliasEntry>, String> {
    let path = config_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    serde_json::from_str::<Vec<AliasEntry>>(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {}", error))
}

fn write_alias_files(aliases: &[AliasEntry]) -> Result<(), String> {
    let config = serde_json::to_string_pretty(aliases)
        .map_err(|error| format!("Aliases could not be serialized: {}", error))?;
    let aliases_shell = render_aliases(aliases)?;
    let config_path = config_file()?;
    let aliases_path = aliases_file()?;

    fs::write(&aliases_path, aliases_shell)
        .map_err(|error| format!("{} could not be written: {}", aliases_path.display(), error))?;
    fs::write(&config_path, format!("{}\n", config))
        .map_err(|error| format!("{} could not be written: {}", config_path.display(), error))
}

fn replace_imported_alias_lines(content: &str, selected_lines: &HashMap<usize, &str>) -> String {
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    for (index, line) in lines.iter_mut().enumerate() {
        if let Some(name) = selected_lines.get(&(index + 1)) {
            *line = format!(": # EasyAlias imported alias {}", name);
        }
    }
    lines.join("\n")
}

// Startup command: prepare the private files, connect the detected shell, and
// load persisted entries if config.json already exists.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    let config_exists = config_file()?.exists();
    let import_was_handled = import_marker_file()?.exists();
    let import_candidates = if !config_exists && !import_was_handled {
        scan_shell_aliases(&setup)?
    } else {
        Vec::new()
    };

    if !config_exists && !import_was_handled && import_candidates.is_empty() {
        mark_import_handled()?;
    }

    ensure_shell_source(&setup)?;
    app_state(load_config_aliases()?, &setup, import_candidates)
}

// Save command: persist structured JSON and regenerate the shell-owned output
// file as one operation from the frontend's current list.
#[tauri::command]
fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;

    write_alias_files(&aliases)?;
    app_state(aliases, &setup, Vec::new())
}

// Manually rescan the detected shell startup file when Import is opened from
// the header. This ignores the first-start marker so aliases added later remain
// importable, while already managed names are excluded from the selection.
#[tauri::command]
fn scan_shell_import() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;

    let aliases = load_config_aliases()?;
    let existing_names: HashSet<&str> = aliases.iter().map(|alias| alias.name.as_str()).collect();
    let import_candidates = scan_shell_aliases(&setup)?
        .into_iter()
        .filter(|candidate| !existing_names.contains(candidate.name.as_str()))
        .collect();

    app_state(aliases, &setup, import_candidates)
}

#[tauri::command]
fn dismiss_shell_import() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;
    mark_import_handled()?;
    app_state(load_config_aliases()?, &setup, Vec::new())
}

#[tauri::command]
fn import_shell_aliases(
    selected_ids: Vec<String>,
    timestamp: String,
) -> Result<ImportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one alias to import.".to_string());
    }
    if timestamp.trim().is_empty() {
        return Err("Import timestamp is missing.".to_string());
    }

    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;

    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let selected: Vec<ShellAliasCandidate> = scan_shell_aliases(&setup)?
        .into_iter()
        .filter(|candidate| selected_id_set.contains(candidate.id.as_str()))
        .collect();
    if selected.len() != selected_id_set.len() {
        return Err(format!(
            "Some aliases changed in {}. Reopen EasyAlias and try again.",
            display_home_path(setup.config_file.clone())?
        ));
    }

    let mut aliases = load_config_aliases()?;
    let mut names: HashSet<String> = aliases.iter().map(|alias| alias.name.clone()).collect();
    for candidate in &selected {
        if !names.insert(candidate.name.clone()) {
            return Err(format!("Alias \"{}\" already exists.", candidate.name));
        }
    }

    let import_id = unix_timestamp()?;
    for candidate in &selected {
        aliases.push(AliasEntry {
            id: format!("imported-{}-{}", import_id, candidate.line_number),
            name: candidate.name.clone(),
            path: String::new(),
            action: "custom".to_string(),
            custom_command: Some(candidate.command.clone()),
            command_preview: candidate.command.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        });
    }
    aliases.sort_by(|left, right| left.name.cmp(&right.name));

    let shell_content = read_text_or_empty(&setup.config_file)?;
    let selected_lines: HashMap<usize, &str> = selected
        .iter()
        .map(|candidate| (candidate.line_number, candidate.name.as_str()))
        .collect();
    let next_shell_content = replace_imported_alias_lines(&shell_content, &selected_lines);
    let backup_path = next_shell_backup_file(&setup)?;

    fs::write(&backup_path, &shell_content)
        .map_err(|error| format!("{} could not be written: {}", backup_path.display(), error))?;
    write_alias_files(&aliases)?;
    fs::write(&setup.config_file, next_shell_content).map_err(|error| {
        format!(
            "{} could not be updated: {}",
            setup.config_file.display(),
            error
        )
    })?;
    mark_import_handled()?;

    Ok(ImportResult {
        state: app_state(aliases, &setup, Vec::new())?,
        imported_count: selected.len(),
        backup_file: display_home_path(backup_path)?,
    })
}

fn main() {
    // dialog = native file/folder picker; opener = GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            load_aliases,
            save_aliases,
            scan_shell_import,
            dismiss_shell_import,
            import_shell_aliases
        ])
        .run(tauri::generate_context!())
        .expect("error while running EasyAlias");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    struct TemporaryHome {
        path: PathBuf,
        previous_home: Option<OsString>,
        previous_shell: Option<OsString>,
    }

    impl TemporaryHome {
        fn create() -> Self {
            let path = env::temp_dir().join(format!(
                "easyalias-linux-import-test-{}-{}",
                std::process::id(),
                unix_timestamp().unwrap()
            ));
            fs::create_dir_all(&path).unwrap();
            let previous_home = env::var_os("HOME");
            let previous_shell = env::var_os("SHELL");
            env::set_var("HOME", &path);
            env::set_var("SHELL", "/bin/bash");
            Self {
                path,
                previous_home,
                previous_shell,
            }
        }
    }

    impl Drop for TemporaryHome {
        fn drop(&mut self) {
            if let Some(value) = &self.previous_home {
                env::set_var("HOME", value);
            } else {
                env::remove_var("HOME");
            }
            if let Some(value) = &self.previous_shell {
                env::set_var("SHELL", value);
            } else {
                env::remove_var("SHELL");
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parses_only_safe_single_line_aliases() {
        let alias = parse_shell_alias_line("alias ll='ls -lah'", 3).unwrap();
        assert_eq!(alias.name, "ll");
        assert_eq!(alias.command, "ls -lah");
        assert!(parse_shell_alias_line("  alias nested='echo no'", 4).is_none());
        assert!(parse_shell_alias_line("alias -g pipe='| grep'", 5).is_none());
        assert!(parse_shell_alias_line("alias a='one' b='two'", 6).is_none());
    }

    #[test]
    fn skips_repeated_names() {
        let aliases = find_shell_aliases(
            "alias gs='git status'\nalias ll='ls -lah'\nalias gs='git status --short'\n",
        );
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name, "ll");
    }

    #[test]
    fn first_start_import_uses_detected_shell_and_creates_backup() {
        let temporary_home = TemporaryHome::create();
        let bashrc = temporary_home.path.join(".bashrc");
        fs::write(&bashrc, "alias legacy='echo legacy'\nexport TEST=1\n").unwrap();

        let initial = load_aliases().unwrap();
        assert_eq!(initial.shell_name, "bash");
        assert_eq!(initial.import_candidates.len(), 1);

        let result = import_shell_aliases(
            vec![initial.import_candidates[0].id.clone()],
            "2026-07-18T10:00:00.000Z".to_string(),
        )
        .unwrap();

        assert_eq!(result.imported_count, 1);
        assert!(temporary_home
            .path
            .join(result.backup_file.trim_start_matches("~/"))
            .exists());
        assert!(fs::read_to_string(&bashrc)
            .unwrap()
            .contains(": # EasyAlias imported alias legacy"));
        assert!(fs::read_to_string(aliases_file().unwrap())
            .unwrap()
            .contains("alias legacy='echo legacy'"));
    }
}
