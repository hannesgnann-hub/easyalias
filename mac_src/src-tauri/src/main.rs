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

// A conservative, single-line alias found in ~/.zshrc. The line number is part
// of the id so the backend can rescan and verify the user's selection on import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZshrcAliasCandidate {
    id: String,
    name: String,
    command: String,
    line_number: usize,
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
    import_candidates: Vec<ZshrcAliasCandidate>,
}

// Import returns the updated state together with the backup location so the UI
// can tell the user exactly where the original ~/.zshrc was preserved.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    state: AppState,
    imported_count: usize,
    backup_file: String,
}

// EasyAlias owns ~/.easyalias/aliases.zsh and only adds a source line to ~/.zshrc.
// This keeps the user's existing shell config mostly untouched.
const SOURCE_LINE: &str = "source ~/.easyalias/aliases.zsh";
const APP_ALIAS_NAME: &str = "easya";
const APP_ALIAS_LINE: &str = "alias easya='open /Applications/EasyAlias.app'";
const IMPORT_MARKER_CONTENT: &str = "zshrc import prompt handled\n";

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

fn import_marker_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".zshrc-import-v1"))
}

fn zshrc_file() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".zshrc"))
}

// A missing startup file is a valid first-run state. Every other read error is
// surfaced so EasyAlias can never overwrite an unreadable ~/.zshrc as if empty.
fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {}", path.display(), error)),
    }
}

// Decode one shell word without running zsh. This supports the common quoted
// and escaped forms used by alias declarations while rejecting extra words.
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

// Parse only unindented, one-line aliases with one assignment. Skipping shell
// options, indented conditional aliases, and multi-assignment lines keeps the
// first-run migration intentionally predictable.
fn parse_zshrc_alias_line(line: &str, line_number: usize) -> Option<ZshrcAliasCandidate> {
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
    Some(ZshrcAliasCandidate {
        id: format!("zshrc-line-{}", line_number),
        name: name.to_string(),
        command,
        line_number,
    })
}

fn find_zshrc_aliases(content: &str) -> Vec<ZshrcAliasCandidate> {
    let parsed: Vec<ZshrcAliasCandidate> = content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| parse_zshrc_alias_line(line, index + 1))
        .collect();
    let mut name_counts: HashMap<String, usize> = HashMap::new();

    for candidate in &parsed {
        *name_counts.entry(candidate.name.clone()).or_default() += 1;
    }

    // Repeated names depend on declaration order. Skip them rather than moving
    // only one definition and allowing an older hidden definition to resurface.
    parsed
        .into_iter()
        .filter(|candidate| name_counts.get(&candidate.name) == Some(&1))
        .collect()
}

fn scan_zshrc_aliases() -> Result<Vec<ZshrcAliasCandidate>, String> {
    let path = zshrc_file()?;
    let content = read_text_or_empty(&path)?;
    Ok(find_zshrc_aliases(&content))
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

fn next_zshrc_backup_file() -> Result<PathBuf, String> {
    let home = home_dir()?;
    let timestamp = unix_timestamp()?;

    for suffix in 0..1000 {
        let file_name = if suffix == 0 {
            format!(".zshrc.easyalias-backup-{}", timestamp)
        } else {
            format!(".zshrc.easyalias-backup-{}-{}", timestamp, suffix)
        };
        let candidate = home.join(file_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("A unique .zshrc backup name could not be created.".to_string())
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
    let content = read_text_or_empty(&path)?;

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
fn app_state(
    aliases: Vec<AliasEntry>,
    import_candidates: Vec<ZshrcAliasCandidate>,
) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        aliases_file: display_home_path(aliases_file()?)?,
        source_line: SOURCE_LINE.to_string(),
        zshrc_source_present: zshrc_source_present(),
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
    let aliases_zsh = render_aliases(aliases)?;

    let config_path = config_file()?;
    let aliases_path = aliases_file()?;

    fs::write(&aliases_path, aliases_zsh)
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

// Called by the frontend when the app starts.
// Also performs first-run file and .zshrc setup.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    ensure_app_files()?;
    let config_exists = config_file()?.exists();
    let import_was_handled = import_marker_file()?.exists();
    let import_candidates = if !config_exists && !import_was_handled {
        scan_zshrc_aliases()?
    } else {
        Vec::new()
    };

    if !config_exists && !import_was_handled && import_candidates.is_empty() {
        mark_import_handled()?;
    }

    ensure_zshrc_source()?;
    app_state(load_config_aliases()?, import_candidates)
}

// Called whenever aliases are created, edited, or deleted.
// Writes both config.json for the UI and aliases.zsh for zsh.
#[tauri::command]
fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    write_alias_files(&aliases)?;
    app_state(aliases, Vec::new())
}

// Manually rescan ~/.zshrc when the user opens Import from the header. Unlike
// the first-start prompt, this deliberately ignores the handled marker so more
// aliases added later can still be moved into EasyAlias. Already managed names
// are excluded to keep the selection importable as a group.
#[tauri::command]
fn scan_zshrc_import() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_zshrc_source()?;

    let aliases = load_config_aliases()?;
    let existing_names: HashSet<&str> = aliases.iter().map(|alias| alias.name.as_str()).collect();
    let import_candidates = scan_zshrc_aliases()?
        .into_iter()
        .filter(|candidate| !existing_names.contains(candidate.name.as_str()))
        .collect();

    app_state(aliases, import_candidates)
}

// Records that the one-time prompt was declined. No alias lines are changed.
#[tauri::command]
fn dismiss_zshrc_import() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_zshrc_source()?;
    mark_import_handled()?;
    app_state(load_config_aliases()?, Vec::new())
}

// Move selected aliases into EasyAlias. The backend rescans ~/.zshrc instead of
// trusting commands sent by the WebView, creates a backup, and replaces only the
// selected source lines with harmless zsh no-op markers.
#[tauri::command]
fn import_zshrc_aliases(
    selected_ids: Vec<String>,
    timestamp: String,
) -> Result<ImportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one alias to import.".to_string());
    }
    if timestamp.trim().is_empty() {
        return Err("Import timestamp is missing.".to_string());
    }

    ensure_app_files()?;
    ensure_zshrc_source()?;

    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let candidates = scan_zshrc_aliases()?;
    let selected: Vec<ZshrcAliasCandidate> = candidates
        .into_iter()
        .filter(|candidate| selected_id_set.contains(candidate.id.as_str()))
        .collect();

    if selected.len() != selected_id_set.len() {
        return Err(
            "Some aliases changed in ~/.zshrc. Reopen EasyAlias and try again.".to_string(),
        );
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

    let zshrc_path = zshrc_file()?;
    let zshrc_content = read_text_or_empty(&zshrc_path)?;
    let selected_lines: HashMap<usize, &str> = selected
        .iter()
        .map(|candidate| (candidate.line_number, candidate.name.as_str()))
        .collect();
    let next_zshrc_content = replace_imported_alias_lines(&zshrc_content, &selected_lines);

    let backup_path = next_zshrc_backup_file()?;
    fs::write(&backup_path, &zshrc_content)
        .map_err(|error| format!("{} could not be written: {}", backup_path.display(), error))?;

    write_alias_files(&aliases)?;
    fs::write(&zshrc_path, next_zshrc_content)
        .map_err(|error| format!("{} could not be updated: {}", zshrc_path.display(), error))?;
    mark_import_handled()?;

    Ok(ImportResult {
        state: app_state(aliases, Vec::new())?,
        imported_count: selected.len(),
        backup_file: display_home_path(backup_path)?,
    })
}

fn main() {
    // Register native plugins before exposing commands to the frontend.
    // dialog = file/folder picker, opener = open GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            load_aliases,
            save_aliases,
            scan_zshrc_import,
            dismiss_zshrc_import,
            import_zshrc_aliases
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
    }

    impl TemporaryHome {
        fn create() -> Self {
            let path = env::temp_dir().join(format!(
                "easyalias-import-test-{}-{}",
                std::process::id(),
                unix_timestamp().unwrap()
            ));
            fs::create_dir_all(&path).unwrap();
            let previous_home = env::var_os("HOME");
            env::set_var("HOME", &path);

            Self {
                path,
                previous_home,
            }
        }
    }

    impl Drop for TemporaryHome {
        fn drop(&mut self) {
            if let Some(previous_home) = &self.previous_home {
                env::set_var("HOME", previous_home);
            } else {
                env::remove_var("HOME");
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parses_common_alias_forms_without_running_zsh() {
        let single = parse_zshrc_alias_line("alias ll='ls -lah'", 4).unwrap();
        assert_eq!(single.name, "ll");
        assert_eq!(single.command, "ls -lah");
        assert_eq!(single.id, "zshrc-line-4");

        let double =
            parse_zshrc_alias_line(r#"alias project="cd \"$HOME/My Project\"""#, 8).unwrap();
        assert_eq!(double.command, "cd \"$HOME/My Project\"");

        let escaped = parse_zshrc_alias_line(r"alias notes=open\ ~/notes.txt", 12).unwrap();
        assert_eq!(escaped.command, "open ~/notes.txt");
    }

    #[test]
    fn skips_aliases_that_are_unsafe_to_move_automatically() {
        assert!(parse_zshrc_alias_line("  alias nested='echo nested'", 1).is_none());
        assert!(parse_zshrc_alias_line("alias -g pipe='| grep'", 2).is_none());
        assert!(parse_zshrc_alias_line("alias one='echo one' two='echo two'", 3).is_none());
        assert!(parse_zshrc_alias_line("alias easya='open something-else'", 4).is_none());
        assert!(parse_zshrc_alias_line("alias broken='missing quote", 5).is_none());
    }

    #[test]
    fn skips_repeated_alias_names() {
        let content = "alias gs='git status'\nalias ll='ls -lah'\nalias gs='git status --short'\n";
        let candidates = find_zshrc_aliases(content);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "ll");
    }

    #[test]
    fn replaces_only_confirmed_lines_and_preserves_file_shape() {
        let content = "export PATH=/opt/bin:$PATH\nalias ll='ls -lah'\nalias gs='git status'\n";
        let selected = HashMap::from([(2, "ll")]);

        assert_eq!(
            replace_imported_alias_lines(content, &selected),
            "export PATH=/opt/bin:$PATH\n: # EasyAlias imported alias ll\nalias gs='git status'\n"
        );
    }

    #[test]
    fn first_start_import_creates_backup_and_managed_files() {
        let temporary_home = TemporaryHome::create();
        let zshrc_path = temporary_home.path.join(".zshrc");
        fs::write(
            &zshrc_path,
            "alias legacy='echo legacy'\nexport PATH=/opt/bin:$PATH\n",
        )
        .unwrap();

        let initial_state = load_aliases().unwrap();
        assert_eq!(initial_state.import_candidates.len(), 1);
        assert_eq!(initial_state.import_candidates[0].name, "legacy");

        let result = import_zshrc_aliases(
            vec![initial_state.import_candidates[0].id.clone()],
            "2026-07-17T12:00:00.000Z".to_string(),
        )
        .unwrap();

        assert_eq!(result.imported_count, 1);
        assert!(temporary_home
            .path
            .join(result.backup_file.trim_start_matches("~/"))
            .exists());
        assert!(fs::read_to_string(&zshrc_path)
            .unwrap()
            .contains(": # EasyAlias imported alias legacy"));
        assert!(fs::read_to_string(aliases_file().unwrap())
            .unwrap()
            .contains("alias legacy='echo legacy'"));
        assert_eq!(load_config_aliases().unwrap().len(), 1);
        assert!(import_marker_file().unwrap().exists());
    }
}
