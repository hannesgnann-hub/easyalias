use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

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
    #[serde(default)]
    favorite: bool,
    created_at: String,
    updated_at: String,
}

// A conservative, single-line alias found in a shell startup file. The source
// path and line number let the backend verify the selection again on import.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShellAliasCandidate {
    id: String,
    name: String,
    command: String,
    line_number: usize,
    source_file: String,
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
    shell_name: String,
    shell_config_file: String,
    shell_source_present: bool,
    import_candidates: Vec<ShellAliasCandidate>,
}

// Import returns the updated state together with the backup locations so the UI
// can tell the user where every changed startup file was preserved.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    state: AppState,
    imported_count: usize,
    backup_file: String,
}

// Portable backups are deliberately wrapped in a versioned envelope instead
// of exposing config.json directly. This gives future releases room to evolve
// the format while rejecting unrelated or malformed JSON files today.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AliasBackup {
    format: String,
    version: u32,
    exported_at: String,
    aliases: Vec<AliasEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupExportResult {
    file: String,
    exported_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupImportResult {
    state: AppState,
    imported_count: usize,
    replaced_count: usize,
}

// Deleted aliases live in a separate file so config.json remains fully
// backwards-compatible. deleted_at is Unix time, which makes the 30-day
// retention rule independent from locale and frontend date parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashEntry {
    alias: AliasEntry,
    deleted_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrashMutationResult {
    state: AppState,
    trash: Vec<TrashEntry>,
}

// Automations are intentionally stored separately from aliases. Each workflow
// has one working directory and an ordered list of commands or timed pauses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationStep {
    id: String,
    kind: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    seconds: u64,
    #[serde(default = "default_command_behavior")]
    behavior: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Automation {
    id: String,
    name: String,
    path: String,
    steps: Vec<AutomationStep>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomationCommandResult {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    process_id: Option<u32>,
}

// One automation run gets one persistent /bin/zsh process, so `cd` and
// exported environment variables carry over between steps exactly as they
// would in a real terminal. A background thread streams its merged
// stdout/stderr line by line into `output_rx`; commands are matched to their
// output by writing a unique sentinel after each one.
struct AutomationSessionHandle {
    child: Child,
    stdin: ChildStdin,
    output_rx: mpsc::Receiver<String>,
}

#[derive(Default)]
struct AutomationSessions(Mutex<HashMap<String, AutomationSessionHandle>>);

const AUTOMATION_DONE_MARKER: &str = "__EASYALIAS_AUTOMATION_DONE__";
const AUTOMATION_BG_MARKER: &str = "__EASYALIAS_AUTOMATION_BG__";

// Keep the established aliases.zsh path for backwards compatibility. The file
// contains syntax understood by both zsh and Bash, regardless of its extension.
const SOURCE_LINE: &str = "source ~/.easyalias/aliases.zsh";
const APP_ALIAS_NAME: &str = "easya";
const APP_ALIAS_LINE: &str = "alias easya='open /Applications/EasyAlias.app'";
const IMPORT_MARKER_CONTENT: &str = "shell alias import prompt handled\n";
const BACKUP_FORMAT: &str = "easyalias-backup";
const BACKUP_VERSION: u32 = 1;
const MAX_BACKUP_BYTES: u64 = 5 * 1024 * 1024;
const MAX_BACKUP_ALIASES: usize = 5000;
const TRASH_RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_AUTOMATIONS: usize = 200;
const MAX_AUTOMATION_STEPS: usize = 100;
const MAX_AUTOMATION_COMMAND_BYTES: usize = 16 * 1024;
const MAX_AUTOMATION_OUTPUT_CHARS: usize = 20_000;
const MAX_WAIT_SECONDS: u64 = 24 * 60 * 60;

fn default_command_behavior() -> String {
    "wait".to_string()
}

#[derive(Debug)]
struct ShellSetup {
    name: String,
    config_files: Vec<PathBuf>,
}

// Resolve the user's home directory without pulling in extra dependencies.
fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME could not be read.".to_string())
}

// macOS and editor terminals can use different shells while inheriting the same
// SHELL value. Connect both supported shells so a zsh login session and a Bash
// VS Code terminal see the same EasyAlias commands.
fn shell_setup() -> Result<ShellSetup, String> {
    let home = home_dir()?;

    Ok(ShellSetup {
        name: "zsh + Bash".to_string(),
        config_files: vec![
            home.join(".zshrc"),
            home.join(".bash_profile"),
            home.join(".bashrc"),
        ],
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
    Ok(app_dir()?.join("aliases.zsh"))
}

fn trash_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("trash.json"))
}

fn automations_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("automations.json"))
}

fn import_marker_file(_setup: &ShellSetup) -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".shell-import-v3"))
}

// A missing startup file is a valid first-run state. Every other read error is
// surfaced so EasyAlias can never overwrite an unreadable startup file as empty.
fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {}", path.display(), error)),
    }
}

// Decode one shell word without running a shell. This supports common quoted
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
        source_file: String::new(),
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

    // Repeated names depend on declaration order. Skip them rather than moving
    // only one definition and allowing an older hidden definition to resurface.
    parsed
        .into_iter()
        .filter(|candidate| name_counts.get(&candidate.name) == Some(&1))
        .collect()
}

fn scan_shell_aliases(setup: &ShellSetup) -> Result<Vec<ShellAliasCandidate>, String> {
    let mut candidates = Vec::new();

    for (file_index, path) in setup.config_files.iter().enumerate() {
        let source_file = display_home_path(path.clone())?;
        for mut candidate in find_shell_aliases(&read_text_or_empty(path)?) {
            candidate.id = format!("shell-{}-line-{}", file_index, candidate.line_number);
            candidate.source_file = source_file.clone();
            candidates.push(candidate);
        }
    }

    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for candidate in &candidates {
        *name_counts.entry(candidate.name.clone()).or_default() += 1;
    }

    // A Bash alias may be declared in both startup files. Skip ambiguous names
    // instead of importing one declaration and exposing another unexpectedly.
    Ok(candidates
        .into_iter()
        .filter(|candidate| name_counts.get(&candidate.name) == Some(&1))
        .collect())
}

fn mark_import_handled(setup: &ShellSetup) -> Result<(), String> {
    let path = import_marker_file(setup)?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("System time could not be read: {}", error))
}

fn next_shell_backup_file(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
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
        let candidate = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(backup_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("A unique shell configuration backup name could not be created.".to_string())
}

fn shell_source_present(setup: &ShellSetup) -> bool {
    setup.config_files.iter().all(|path| {
        fs::read_to_string(path)
            .ok()
            .map(|content| content.lines().any(|line| line.trim() == SOURCE_LINE))
            .unwrap_or(false)
    })
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

fn ensure_shell_file(path: &Path) -> Result<(), String> {
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

// Connect every supported startup file. Existing content is preserved
// byte-for-byte apart from missing EasyAlias blocks at the end.
fn ensure_shell_source(setup: &ShellSetup) -> Result<(), String> {
    for path in &setup.config_files {
        ensure_shell_file(path)?;
    }
    Ok(())
}

fn shell_config_file_display(setup: &ShellSetup) -> Result<String, String> {
    setup
        .config_files
        .iter()
        .cloned()
        .map(display_home_path)
        .collect::<Result<Vec<_>, _>>()
        .map(|paths| paths.join(" and "))
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

fn validate_alias_entry(alias: &AliasEntry) -> Result<(), String> {
    if alias.id.trim().is_empty() {
        return Err(format!("Alias \"{}\" has no id.", alias.name));
    }
    if !validate_alias_name(&alias.name) {
        return Err(format!("Invalid alias name: {}", alias.name));
    }
    if !matches!(
        alias.action.as_str(),
        "navigate" | "open" | "execute" | "compile_gradle" | "compile_maven" | "custom"
    ) {
        return Err(format!(
            "Alias \"{}\" has an unsupported action.",
            alias.name
        ));
    }
    if alias.command_preview.trim().is_empty() {
        return Err(format!("Alias {} has no command.", alias.name));
    }
    if alias.created_at.trim().is_empty() || alias.updated_at.trim().is_empty() {
        return Err(format!(
            "Alias \"{}\" has incomplete timestamps.",
            alias.name
        ));
    }

    Ok(())
}

fn validate_alias_collection(aliases: &[AliasEntry]) -> Result<(), String> {
    if aliases.len() > MAX_BACKUP_ALIASES {
        return Err(format!(
            "Backup contains more than {} aliases.",
            MAX_BACKUP_ALIASES
        ));
    }

    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for alias in aliases {
        validate_alias_entry(alias)?;
        if !ids.insert(alias.id.as_str()) {
            return Err(format!("Duplicate alias id in backup: {}", alias.id));
        }
        if !names.insert(alias.name.as_str()) {
            return Err(format!("Duplicate alias name in backup: {}", alias.name));
        }
    }

    Ok(())
}

fn read_backup(path: &Path) -> Result<AliasBackup, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("{} could not be inspected: {}", path.display(), error))?;
    if !metadata.is_file() {
        return Err("Choose an EasyAlias JSON backup file.".to_string());
    }
    if metadata.len() > MAX_BACKUP_BYTES {
        return Err("The backup is larger than 5 MB.".to_string());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let backup: AliasBackup = serde_json::from_str(&content)
        .map_err(|error| format!("This is not a valid EasyAlias backup: {}", error))?;

    if backup.format != BACKUP_FORMAT || backup.version != BACKUP_VERSION {
        return Err("This EasyAlias backup format is not supported.".to_string());
    }
    if backup.exported_at.trim().is_empty() {
        return Err("The backup has no export timestamp.".to_string());
    }
    validate_alias_collection(&backup.aliases)?;

    Ok(backup)
}

// Wrap a shell command in single quotes for a Bash/zsh alias assignment.
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
        validate_alias_entry(alias)?;

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
    setup: &ShellSetup,
    import_candidates: Vec<ShellAliasCandidate>,
) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        aliases_file: display_home_path(aliases_file()?)?,
        source_line: SOURCE_LINE.to_string(),
        shell_name: setup.name.clone(),
        shell_config_file: shell_config_file_display(setup)?,
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
    let aliases_zsh = render_aliases(aliases)?;

    let config_path = config_file()?;
    let aliases_path = aliases_file()?;

    fs::write(&aliases_path, aliases_zsh)
        .map_err(|error| format!("{} could not be written: {}", aliases_path.display(), error))?;
    fs::write(&config_path, format!("{}\n", config))
        .map_err(|error| format!("{} could not be written: {}", config_path.display(), error))
}

fn write_trash_entries(entries: &[TrashEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Trash could not be serialized: {}", error))?;
    let path = trash_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn validate_automations(automations: &[Automation]) -> Result<(), String> {
    if automations.len() > MAX_AUTOMATIONS {
        return Err(format!(
            "EasyAlias supports up to {} automations.",
            MAX_AUTOMATIONS
        ));
    }

    let mut automation_ids = HashSet::new();
    for automation in automations {
        let name = automation.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err("Every automation needs a name with at most 120 characters.".to_string());
        }
        if automation.id.trim().is_empty() || !automation_ids.insert(automation.id.as_str()) {
            return Err("Every automation needs a unique id.".to_string());
        }
        if automation.path.trim().is_empty() || automation.path.chars().count() > 4096 {
            return Err(format!(
                "Automation \"{}\" needs a valid working directory.",
                name
            ));
        }
        if automation.steps.is_empty() || automation.steps.len() > MAX_AUTOMATION_STEPS {
            return Err(format!(
                "Automation \"{}\" needs between 1 and {} steps.",
                name, MAX_AUTOMATION_STEPS
            ));
        }

        let mut step_ids = HashSet::new();
        for step in &automation.steps {
            if step.id.trim().is_empty() || !step_ids.insert(step.id.as_str()) {
                return Err(format!(
                    "Automation \"{}\" contains a duplicate step id.",
                    name
                ));
            }

            match step.kind.as_str() {
                "command" => {
                    if step.command.trim().is_empty()
                        || step.command.len() > MAX_AUTOMATION_COMMAND_BYTES
                    {
                        return Err(format!(
                            "Every command in \"{}\" must contain at most {} bytes.",
                            name, MAX_AUTOMATION_COMMAND_BYTES
                        ));
                    }
                    if !matches!(step.behavior.as_str(), "wait" | "background") {
                        return Err(format!(
                            "Automation \"{}\" has an invalid command mode.",
                            name
                        ));
                    }
                }
                "wait" => {
                    if step.seconds == 0 || step.seconds > MAX_WAIT_SECONDS {
                        return Err(format!(
                            "Wait steps in \"{}\" must be between 1 second and 24 hours.",
                            name
                        ));
                    }
                }
                _ => return Err(format!("Automation \"{}\" has an unknown step type.", name)),
            }
        }
    }

    Ok(())
}

fn load_automation_entries() -> Result<Vec<Automation>, String> {
    ensure_app_files()?;
    let path = automations_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let automations: Vec<Automation> = serde_json::from_str(&content)
        .map_err(|error| format!("automations.json is not valid EasyAlias JSON: {}", error))?;
    validate_automations(&automations)?;
    Ok(automations)
}

fn write_automation_entries(automations: &[Automation]) -> Result<(), String> {
    validate_automations(automations)?;
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(automations)
        .map_err(|error| format!("Automations could not be serialized: {}", error))?;
    let path = automations_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn automation_working_directory(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    let path = if trimmed == "~" {
        home_dir()?
    } else if let Some(relative) = trimmed.strip_prefix("~/") {
        home_dir()?.join(relative)
    } else {
        PathBuf::from(trimmed)
    };

    if !path.is_dir() {
        return Err(format!(
            "Working directory does not exist: {}",
            path.display()
        ));
    }

    path.canonicalize()
        .map_err(|error| format!("Working directory could not be opened: {}", error))
}

fn limited_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(MAX_AUTOMATION_OUTPUT_CHARS)
        .collect()
}

// Reading the trash also enforces retention. Expired entries are removed from
// disk immediately, so they cannot reappear after an app restart.
fn load_trash_entries() -> Result<Vec<TrashEntry>, String> {
    ensure_app_files()?;
    let path = trash_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let mut entries: Vec<TrashEntry> = serde_json::from_str(&content)
        .map_err(|error| format!("trash.json is not valid EasyAlias JSON: {}", error))?;
    let now = unix_timestamp()?;
    let original_len = entries.len();
    entries.retain(|entry| now.saturating_sub(entry.deleted_at) < TRASH_RETENTION_SECONDS);
    entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));

    if entries.len() != original_len {
        write_trash_entries(&entries)?;
    }

    Ok(entries)
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

// Called by the frontend when the app starts. Detect and connect the login
// shell before loading managed aliases or offering a first-run import.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    let config_exists = config_file()?.exists();
    let import_was_handled = import_marker_file(&setup)?.exists();
    let import_candidates = if !config_exists && !import_was_handled {
        scan_shell_aliases(&setup)?
    } else {
        Vec::new()
    };

    if !config_exists && !import_was_handled && import_candidates.is_empty() {
        mark_import_handled(&setup)?;
    }

    ensure_shell_source(&setup)?;
    app_state(load_config_aliases()?, &setup, import_candidates)
}

// Called whenever aliases are created, edited, or deleted.
// Writes both config.json for the UI and the shared shell alias file.
#[tauri::command]
fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;

    write_alias_files(&aliases)?;
    app_state(aliases, &setup, Vec::new())
}

#[tauri::command]
fn load_automations() -> Result<Vec<Automation>, String> {
    load_automation_entries()
}

#[tauri::command]
fn save_automations(automations: Vec<Automation>) -> Result<Vec<Automation>, String> {
    write_automation_entries(&automations)?;
    Ok(automations)
}

// Spawns the one persistent shell an automation run drives its steps
// through, with a background thread streaming its merged stdout/stderr into
// a channel so `execute_in_session` can read command output synchronously.
fn spawn_automation_session(working_directory: &Path) -> Result<AutomationSessionHandle, String> {
    let mut child = Command::new("/bin/zsh")
        .arg("-l")
        .current_dir(working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Shell session could not be started: {}", error))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Shell session has no input stream.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Shell session has no output stream.".to_string())?;

    let (sender, receiver) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(text) => {
                    if sender.send(text).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(AutomationSessionHandle {
        child,
        stdin,
        output_rx: receiver,
    })
}

// Runs one command inside an already-running automation session, so `cd`
// and exported variables from earlier steps are still in effect. Foreground
// commands wait for a completion sentinel carrying the exit code; background
// commands only wait for confirmation that the job was started, so a
// long-running dev server does not block the next step.
fn execute_in_session(
    session: &mut AutomationSessionHandle,
    command: &str,
    background: bool,
) -> Result<AutomationCommandResult, String> {
    let send_error = |error: std::io::Error| format!("Command could not be sent: {}", error);
    let recv_error = || "Automation session ended unexpectedly.".to_string();

    if background {
        writeln!(session.stdin, "{{ {} ; }} >/dev/null 2>&1 &", command).map_err(send_error)?;
        writeln!(session.stdin, "echo \"{}$!\"", AUTOMATION_BG_MARKER).map_err(send_error)?;
        session.stdin.flush().map_err(send_error)?;

        loop {
            let line = session.output_rx.recv().map_err(|_| recv_error())?;
            if let Some(pid) = line.strip_prefix(AUTOMATION_BG_MARKER) {
                return Ok(AutomationCommandResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    process_id: pid.trim().parse::<u32>().ok(),
                });
            }
        }
    }

    writeln!(session.stdin, "{{ {} ; }} 2>&1", command).map_err(send_error)?;
    writeln!(session.stdin, "echo \"{}$?\"", AUTOMATION_DONE_MARKER).map_err(send_error)?;
    session.stdin.flush().map_err(send_error)?;

    let mut collected = String::new();
    loop {
        let line = session.output_rx.recv().map_err(|_| recv_error())?;
        if let Some(code) = line.strip_prefix(AUTOMATION_DONE_MARKER) {
            return Ok(AutomationCommandResult {
                exit_code: code.trim().parse::<i32>().ok(),
                stdout: limited_output(collected.trim_end().as_bytes()),
                stderr: String::new(),
                process_id: None,
            });
        }
        collected.push_str(&line);
        collected.push('\n');
    }
}

// The frontend generates `session_id` (mirrors how entity ids are created
// elsewhere) and passes it back into every later call for this run.
#[tauri::command]
fn start_automation_session(
    session_id: String,
    path: String,
    sessions: tauri::State<AutomationSessions>,
) -> Result<(), String> {
    let working_directory = automation_working_directory(&path)?;
    let session = spawn_automation_session(&working_directory)?;

    let mut registry = sessions
        .0
        .lock()
        .map_err(|_| "Automation session lock was poisoned.".to_string())?;
    registry.insert(session_id, session);
    Ok(())
}

#[tauri::command]
async fn run_session_command(
    session_id: String,
    command: String,
    background: bool,
    app: tauri::AppHandle,
) -> Result<AutomationCommandResult, String> {
    if command.trim().is_empty() || command.len() > MAX_AUTOMATION_COMMAND_BYTES {
        return Err(format!(
            "Command must contain at most {} bytes.",
            MAX_AUTOMATION_COMMAND_BYTES
        ));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let sessions = app.state::<AutomationSessions>();
        let mut registry = sessions
            .0
            .lock()
            .map_err(|_| "Automation session lock was poisoned.".to_string())?;
        let session = registry
            .get_mut(&session_id)
            .ok_or_else(|| "Automation session is no longer running.".to_string())?;
        execute_in_session(session, &command, background)
    })
    .await
    .map_err(|error| format!("Automation worker failed: {}", error))?
}

// Ends an automation run's shell session, either because the run finished or
// because the user clicked Stop. Killing this specific process (not its
// process group) interrupts a stuck foreground command without touching
// background jobs it already started with `&`, which keep running detached.
#[tauri::command]
fn stop_automation_session(
    session_id: String,
    sessions: tauri::State<AutomationSessions>,
) -> Result<(), String> {
    let mut registry = sessions
        .0
        .lock()
        .map_err(|_| "Automation session lock was poisoned.".to_string())?;
    if let Some(mut session) = registry.remove(&session_id) {
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
    Ok(())
}

#[tauri::command]
fn list_trash() -> Result<Vec<TrashEntry>, String> {
    load_trash_entries()
}

// Write the recoverable copy first. If updating the active alias files then
// fails, the alias may exist in both places, but user data is never lost.
#[tauri::command]
fn move_alias_to_trash(id: String) -> Result<TrashMutationResult, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;
    let mut aliases = load_config_aliases()?;
    let index = aliases
        .iter()
        .position(|alias| alias.id == id)
        .ok_or_else(|| "Alias no longer exists.".to_string())?;
    let alias = aliases.remove(index);
    let mut trash = load_trash_entries()?;
    trash.retain(|entry| entry.alias.id != alias.id);
    trash.push(TrashEntry {
        alias,
        deleted_at: unix_timestamp()?,
    });
    trash.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));

    write_trash_entries(&trash)?;
    write_alias_files(&aliases)?;

    Ok(TrashMutationResult {
        state: app_state(aliases, &setup, Vec::new())?,
        trash,
    })
}

// Restore into active storage before removing the recoverable copy. A name
// conflict is rejected to avoid silently replacing a newer alias.
#[tauri::command]
fn restore_trash_alias(id: String) -> Result<TrashMutationResult, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;
    let mut trash = load_trash_entries()?;
    let index = trash
        .iter()
        .position(|entry| entry.alias.id == id)
        .ok_or_else(|| "Deleted alias no longer exists.".to_string())?;
    let alias = trash[index].alias.clone();
    let mut aliases = load_config_aliases()?;

    if aliases.iter().any(|existing| existing.name == alias.name) {
        return Err(format!(
            "Alias \"{}\" already exists. Rename or delete the active alias before restoring.",
            alias.name
        ));
    }
    if aliases.iter().any(|existing| existing.id == alias.id) {
        return Err("An active alias already uses this id.".to_string());
    }

    aliases.push(alias);
    aliases.sort_by(|left, right| left.name.cmp(&right.name));
    write_alias_files(&aliases)?;
    trash.remove(index);
    write_trash_entries(&trash)?;

    Ok(TrashMutationResult {
        state: app_state(aliases, &setup, Vec::new())?,
        trash,
    })
}

#[tauri::command]
fn permanently_delete_trash_alias(id: String) -> Result<Vec<TrashEntry>, String> {
    let mut trash = load_trash_entries()?;
    let original_len = trash.len();
    trash.retain(|entry| entry.alias.id != id);
    if trash.len() == original_len {
        return Err("Deleted alias no longer exists.".to_string());
    }
    write_trash_entries(&trash)?;
    Ok(trash)
}

#[tauri::command]
fn empty_trash() -> Result<Vec<TrashEntry>, String> {
    ensure_app_files()?;
    write_trash_entries(&[])?;
    Ok(Vec::new())
}

// Export only the aliases selected in the review dialog. The backend verifies
// that every requested id still exists before writing the portable JSON file.
#[tauri::command]
fn export_alias_backup(
    selected_ids: Vec<String>,
    destination: String,
    exported_at: String,
) -> Result<BackupExportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one alias to export.".to_string());
    }
    if destination.trim().is_empty() {
        return Err("Choose where to save the backup.".to_string());
    }
    if exported_at.trim().is_empty() {
        return Err("Export timestamp is missing.".to_string());
    }

    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let aliases = load_config_aliases()?;
    let selected: Vec<AliasEntry> = aliases
        .into_iter()
        .filter(|alias| selected_id_set.contains(alias.id.as_str()))
        .collect();
    if selected.len() != selected_id_set.len() {
        return Err("Some aliases changed. Reopen Export and try again.".to_string());
    }
    validate_alias_collection(&selected)?;

    let backup = AliasBackup {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at,
        aliases: selected,
    };
    let json = serde_json::to_string_pretty(&backup)
        .map_err(|error| format!("Backup could not be serialized: {}", error))?;
    let path = PathBuf::from(destination);
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))?;

    Ok(BackupExportResult {
        file: path.display().to_string(),
        exported_count: backup.aliases.len(),
    })
}

// Read and validate a backup before the frontend displays its entries. No app
// data is changed at this stage, so file selection and drop remain reversible.
#[tauri::command]
fn inspect_alias_backup(path: String) -> Result<Vec<AliasEntry>, String> {
    Ok(read_backup(Path::new(&path))?.aliases)
}

// Merge selected backup entries by alias name. Name conflicts intentionally
// replace the existing entry so a backup can restore an edited alias cleanly.
#[tauri::command]
fn import_alias_backup(
    path: String,
    selected_ids: Vec<String>,
    imported_at: String,
) -> Result<BackupImportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one alias to import.".to_string());
    }
    if imported_at.trim().is_empty() {
        return Err("Import timestamp is missing.".to_string());
    }

    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;
    let backup = read_backup(Path::new(&path))?;
    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let mut selected: Vec<AliasEntry> = backup
        .aliases
        .into_iter()
        .filter(|alias| selected_id_set.contains(alias.id.as_str()))
        .collect();
    if selected.len() != selected_id_set.len() {
        return Err("Some selected aliases are no longer present in the backup.".to_string());
    }

    let mut aliases = load_config_aliases()?;
    let selected_names: HashSet<String> = selected.iter().map(|alias| alias.name.clone()).collect();
    let replaced_count = aliases
        .iter()
        .filter(|alias| selected_names.contains(&alias.name))
        .count();
    aliases.retain(|alias| !selected_names.contains(&alias.name));

    for alias in &mut selected {
        // New ids avoid collisions when a backup is imported more than once.
        alias.id = format!("backup-{}-{}", unix_timestamp()?, alias.id);
        alias.updated_at = imported_at.clone();
    }
    let imported_count = selected.len();
    aliases.extend(selected);
    aliases.sort_by(|left, right| left.name.cmp(&right.name));
    validate_alias_collection(&aliases)?;
    write_alias_files(&aliases)?;

    Ok(BackupImportResult {
        state: app_state(aliases, &setup, Vec::new())?,
        imported_count,
        replaced_count,
    })
}

// Manually rescan all supported shell startup files when Import is opened. This
// ignores the first-run marker so aliases added later remain importable.
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

// Records that the one-time prompt was declined. No alias lines are changed.
#[tauri::command]
fn dismiss_shell_import() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;
    mark_import_handled(&setup)?;
    app_state(load_config_aliases()?, &setup, Vec::new())
}

// Move selected aliases into EasyAlias. The backend rescans startup files
// instead of trusting commands from the WebView and backs up every file changed.
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
    let candidates = scan_shell_aliases(&setup)?;
    let selected: Vec<ShellAliasCandidate> = candidates
        .into_iter()
        .filter(|candidate| selected_id_set.contains(candidate.id.as_str()))
        .collect();

    if selected.len() != selected_id_set.len() {
        return Err(format!(
            "Some aliases changed in {}. Reopen EasyAlias and try again.",
            shell_config_file_display(&setup)?
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
            id: format!("imported-{}-{}", import_id, candidate.id),
            name: candidate.name.clone(),
            path: String::new(),
            action: "custom".to_string(),
            custom_command: Some(candidate.command.clone()),
            command_preview: candidate.command.clone(),
            favorite: false,
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
        });
    }
    aliases.sort_by(|left, right| left.name.cmp(&right.name));

    let mut updates = Vec::new();
    let mut backup_files = Vec::new();
    for config_path in &setup.config_files {
        let source_file = display_home_path(config_path.clone())?;
        let selected_lines: HashMap<usize, &str> = selected
            .iter()
            .filter(|candidate| candidate.source_file == source_file)
            .map(|candidate| (candidate.line_number, candidate.name.as_str()))
            .collect();
        if selected_lines.is_empty() {
            continue;
        }

        let content = read_text_or_empty(config_path)?;
        let next_content = replace_imported_alias_lines(&content, &selected_lines);
        let backup_path = next_shell_backup_file(config_path)?;
        fs::write(&backup_path, &content).map_err(|error| {
            format!("{} could not be written: {}", backup_path.display(), error)
        })?;
        backup_files.push(display_home_path(backup_path)?);
        updates.push((config_path.clone(), next_content));
    }

    write_alias_files(&aliases)?;
    for (path, content) in updates {
        fs::write(&path, content)
            .map_err(|error| format!("{} could not be updated: {}", path.display(), error))?;
    }
    mark_import_handled(&setup)?;

    Ok(ImportResult {
        state: app_state(aliases, &setup, Vec::new())?,
        imported_count: selected.len(),
        backup_file: backup_files.join(", "),
    })
}

fn main() {
    // Register native plugins before exposing commands to the frontend.
    // dialog = file/folder picker, opener = open GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AutomationSessions::default())
        .invoke_handler(tauri::generate_handler![
            load_aliases,
            save_aliases,
            load_automations,
            save_automations,
            start_automation_session,
            run_session_command,
            stop_automation_session,
            list_trash,
            move_alias_to_trash,
            restore_trash_alias,
            permanently_delete_trash_alias,
            empty_trash,
            export_alias_backup,
            inspect_alias_backup,
            import_alias_backup,
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
    use std::sync::Mutex;

    static HOME_LOCK: Mutex<()> = Mutex::new(());

    struct TemporaryHome {
        path: PathBuf,
        previous_home: Option<OsString>,
        previous_shell: Option<OsString>,
    }

    impl TemporaryHome {
        fn create() -> Self {
            Self::create_with_shell("/bin/zsh")
        }

        fn create_with_shell(shell: &str) -> Self {
            let path = env::temp_dir().join(format!(
                "easyalias-import-test-{}-{}",
                std::process::id(),
                unix_timestamp().unwrap()
            ));
            fs::create_dir_all(&path).unwrap();
            let previous_home = env::var_os("HOME");
            let previous_shell = env::var_os("SHELL");
            env::set_var("HOME", &path);
            env::set_var("SHELL", shell);

            Self {
                path,
                previous_home,
                previous_shell,
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
            if let Some(previous_shell) = &self.previous_shell {
                env::set_var("SHELL", previous_shell);
            } else {
                env::remove_var("SHELL");
            }
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_alias(id: &str, name: &str, command: &str) -> AliasEntry {
        AliasEntry {
            id: id.to_string(),
            name: name.to_string(),
            path: String::new(),
            action: "custom".to_string(),
            custom_command: Some(command.to_string()),
            command_preview: command.to_string(),
            favorite: false,
            created_at: "2026-08-13T18:00:00.000Z".to_string(),
            updated_at: "2026-08-13T18:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn parses_common_alias_forms_without_running_a_shell() {
        let single = parse_shell_alias_line("alias ll='ls -lah'", 4).unwrap();
        assert_eq!(single.name, "ll");
        assert_eq!(single.command, "ls -lah");
        assert_eq!(single.id, "shell-line-4");

        let double =
            parse_shell_alias_line(r#"alias project="cd \"$HOME/My Project\"""#, 8).unwrap();
        assert_eq!(double.command, "cd \"$HOME/My Project\"");

        let escaped = parse_shell_alias_line(r"alias notes=open\ ~/notes.txt", 12).unwrap();
        assert_eq!(escaped.command, "open ~/notes.txt");
    }

    #[test]
    fn skips_aliases_that_are_unsafe_to_move_automatically() {
        assert!(parse_shell_alias_line("  alias nested='echo nested'", 1).is_none());
        assert!(parse_shell_alias_line("alias -g pipe='| grep'", 2).is_none());
        assert!(parse_shell_alias_line("alias one='echo one' two='echo two'", 3).is_none());
        assert!(parse_shell_alias_line("alias easya='open something-else'", 4).is_none());
        assert!(parse_shell_alias_line("alias broken='missing quote", 5).is_none());
    }

    #[test]
    fn skips_repeated_alias_names() {
        let content = "alias gs='git status'\nalias ll='ls -lah'\nalias gs='git status --short'\n";
        let candidates = find_shell_aliases(content);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "ll");
    }

    #[test]
    fn old_alias_json_defaults_to_not_favorite() {
        let legacy = r#"{
            "id":"legacy-1",
            "name":"ll",
            "path":"",
            "action":"custom",
            "customCommand":"ls -lah",
            "commandPreview":"ls -lah",
            "createdAt":"2026-07-01T12:00:00.000Z",
            "updatedAt":"2026-07-01T12:00:00.000Z"
        }"#;

        let alias: AliasEntry = serde_json::from_str(legacy).unwrap();
        assert!(!alias.favorite);
    }

    #[test]
    fn deleted_alias_can_be_restored_from_trash() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();
        write_alias_files(&[test_alias("one", "ll", "ls -lah")]).unwrap();

        let deleted = move_alias_to_trash("one".to_string()).unwrap();
        assert!(deleted.state.aliases.is_empty());
        assert_eq!(deleted.trash.len(), 1);
        assert_eq!(deleted.trash[0].alias.name, "ll");
        assert!(!fs::read_to_string(aliases_file().unwrap())
            .unwrap()
            .contains("alias ll="));

        let restored = restore_trash_alias("one".to_string()).unwrap();
        assert_eq!(restored.state.aliases.len(), 1);
        assert!(restored.trash.is_empty());
        assert!(fs::read_to_string(aliases_file().unwrap())
            .unwrap()
            .contains("alias ll='ls -lah'"));
    }

    #[test]
    fn expired_trash_entries_are_removed_automatically() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();
        let now = unix_timestamp().unwrap();
        write_trash_entries(&[
            TrashEntry {
                alias: test_alias("expired", "old", "echo old"),
                deleted_at: now.saturating_sub(TRASH_RETENTION_SECONDS + 1),
            },
            TrashEntry {
                alias: test_alias("current", "new", "echo new"),
                deleted_at: now,
            },
        ])
        .unwrap();

        let trash = load_trash_entries().unwrap();
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].alias.id, "current");
        assert!(!fs::read_to_string(trash_file().unwrap())
            .unwrap()
            .contains("expired"));
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
        let _home_lock = HOME_LOCK.lock().unwrap();
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

        let result = import_shell_aliases(
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
        assert!(import_marker_file(&shell_setup().unwrap())
            .unwrap()
            .exists());
    }

    #[test]
    fn connects_zsh_and_bash_when_login_shell_is_zsh() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let temporary_home = TemporaryHome::create_with_shell("/bin/zsh");

        let state = load_aliases().unwrap();
        assert_eq!(state.shell_name, "zsh + Bash");
        assert!(state.shell_config_file.contains("~/.zshrc"));
        assert!(state.shell_config_file.contains("~/.bash_profile"));
        assert!(state.shell_config_file.contains("~/.bashrc"));
        assert!(state.shell_source_present);

        for file_name in [".zshrc", ".bash_profile", ".bashrc"] {
            let content = fs::read_to_string(temporary_home.path.join(file_name)).unwrap();
            assert!(content.contains(SOURCE_LINE));
            assert!(content.contains(APP_ALIAS_LINE));
        }
    }

    #[test]
    fn bash_imports_from_both_startup_files_with_separate_backups() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let temporary_home = TemporaryHome::create_with_shell("/bin/bash");
        fs::write(
            temporary_home.path.join(".bash_profile"),
            "alias loginonly='echo login'\n",
        )
        .unwrap();
        fs::write(
            temporary_home.path.join(".bashrc"),
            "alias interactive='echo interactive'\n",
        )
        .unwrap();

        let state = load_aliases().unwrap();
        assert_eq!(state.import_candidates.len(), 2);
        let ids = state
            .import_candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect();
        let result = import_shell_aliases(ids, "2026-08-14T00:00:00.000Z".to_string()).unwrap();

        assert_eq!(result.imported_count, 2);
        assert_eq!(result.backup_file.split(", ").count(), 2);
        assert!(
            fs::read_to_string(temporary_home.path.join(".bash_profile"))
                .unwrap()
                .contains(": # EasyAlias imported alias loginonly")
        );
        assert!(fs::read_to_string(temporary_home.path.join(".bashrc"))
            .unwrap()
            .contains(": # EasyAlias imported alias interactive"));
        assert!(result.backup_file.split(", ").all(|path| temporary_home
            .path
            .join(path.trim_start_matches("~/"))
            .exists()));
    }

    #[test]
    fn backup_export_contains_only_selected_aliases() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();
        write_alias_files(&[
            test_alias("one", "ll", "ls -lah"),
            test_alias("two", "gs", "git status"),
        ])
        .unwrap();
        let destination = temporary_home.path.join("selected.json");

        let result = export_alias_backup(
            vec!["two".to_string()],
            destination.display().to_string(),
            "2026-08-13T18:30:00.000Z".to_string(),
        )
        .unwrap();
        let backup = read_backup(&destination).unwrap();

        assert_eq!(result.exported_count, 1);
        assert_eq!(backup.aliases.len(), 1);
        assert_eq!(backup.aliases[0].name, "gs");
    }

    #[test]
    fn backup_import_replaces_name_conflicts_and_keeps_other_aliases() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();
        write_alias_files(&[
            test_alias("current-ll", "ll", "ls"),
            test_alias("keep", "gs", "git status"),
        ])
        .unwrap();
        let backup_path = temporary_home.path.join("restore.json");
        let backup = AliasBackup {
            format: BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION,
            exported_at: "2026-08-13T18:30:00.000Z".to_string(),
            aliases: vec![
                test_alias("backup-ll", "ll", "ls -lah"),
                test_alias("backup-dcu", "dcu", "docker compose up -d"),
            ],
        };
        fs::write(&backup_path, serde_json::to_string(&backup).unwrap()).unwrap();

        let result = import_alias_backup(
            backup_path.display().to_string(),
            vec!["backup-ll".to_string(), "backup-dcu".to_string()],
            "2026-08-13T19:00:00.000Z".to_string(),
        )
        .unwrap();

        assert_eq!(result.imported_count, 2);
        assert_eq!(result.replaced_count, 1);
        assert_eq!(result.state.aliases.len(), 3);
        assert!(result
            .state
            .aliases
            .iter()
            .any(|alias| alias.name == "ll" && alias.command_preview == "ls -lah"));
        assert!(result.state.aliases.iter().any(|alias| alias.name == "gs"));
        assert!(result.state.aliases.iter().any(|alias| alias.name == "dcu"));
    }

    #[test]
    fn accepts_a_valid_sequential_automation() {
        let automation = Automation {
            id: "devstart".to_string(),
            name: "DevStart".to_string(),
            path: "~/Projects/nava".to_string(),
            steps: vec![
                AutomationStep {
                    id: "compose".to_string(),
                    kind: "command".to_string(),
                    command: "docker compose up -d".to_string(),
                    seconds: 0,
                    behavior: "wait".to_string(),
                },
                AutomationStep {
                    id: "settle".to_string(),
                    kind: "wait".to_string(),
                    command: String::new(),
                    seconds: 10,
                    behavior: "wait".to_string(),
                },
            ],
            created_at: "2026-08-24T18:00:00.000Z".to_string(),
            updated_at: "2026-08-24T18:00:00.000Z".to_string(),
        };

        assert!(validate_automations(&[automation]).is_ok());
    }

    #[test]
    fn rejects_invalid_automation_steps() {
        let automation = Automation {
            id: "broken".to_string(),
            name: "Broken".to_string(),
            path: "~/Projects/nava".to_string(),
            steps: vec![AutomationStep {
                id: "wait".to_string(),
                kind: "wait".to_string(),
                command: String::new(),
                seconds: 0,
                behavior: "background".to_string(),
            }],
            created_at: "2026-08-24T18:00:00.000Z".to_string(),
            updated_at: "2026-08-24T18:00:00.000Z".to_string(),
        };

        let validation_error = validate_automations(&[automation]).unwrap_err();
        assert!(validation_error.contains("between 1 second and 24 hours"));
    }

    // Reproduces the reported bug: a "cd mac_src" step followed by a command
    // that only succeeds from inside that subdirectory. Each step used to run
    // in its own fresh shell, so the `cd` had no effect on the next step.
    #[test]
    fn automation_session_keeps_directory_change_across_steps() {
        let base = env::temp_dir().join(format!(
            "easyalias-automation-session-test-{}",
            unix_timestamp().unwrap()
        ));
        let subdir = base.join("mac_src");
        fs::create_dir_all(&subdir).unwrap();

        let mut session = spawn_automation_session(&base).unwrap();

        let cd_result = execute_in_session(&mut session, "cd mac_src", false).unwrap();
        assert_eq!(cd_result.exit_code, Some(0));

        let pwd_result = execute_in_session(&mut session, "pwd", false).unwrap();
        assert_eq!(pwd_result.exit_code, Some(0));
        assert!(
            pwd_result.stdout.trim().ends_with("/mac_src"),
            "expected pwd to report the subdirectory, got: {:?}",
            pwd_result.stdout
        );

        let _ = session.child.kill();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn automation_session_background_command_does_not_block_next_step() {
        let base = env::temp_dir().join(format!(
            "easyalias-automation-session-bg-test-{}",
            unix_timestamp().unwrap()
        ));
        fs::create_dir_all(&base).unwrap();

        let mut session = spawn_automation_session(&base).unwrap();

        let bg_result = execute_in_session(&mut session, "sleep 5", true).unwrap();
        assert!(bg_result.process_id.is_some());

        let echo_result = execute_in_session(&mut session, "echo still-here", false).unwrap();
        assert_eq!(echo_result.exit_code, Some(0));
        assert_eq!(echo_result.stdout.trim(), "still-here");

        let _ = session.child.kill();
        let _ = fs::remove_dir_all(&base);
    }
}
