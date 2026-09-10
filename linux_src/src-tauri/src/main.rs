use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    str::FromStr,
    sync::{mpsc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

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
// backwards-compatible. deleted_at is Unix time, which keeps retention locale-independent.
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
    #[serde(default)]
    favorite: bool,
    // A free-text label the UI groups and filters automations by. Empty
    // means "ungrouped"; older automations.json files without this field
    // default to that.
    #[serde(default)]
    group: String,
    // Optional global keyboard shortcut (Tauri accelerator string, e.g.
    // "CmdOrCtrl+Shift+L") that runs this automation from anywhere while
    // EasyAlias is running. Older automations.json files default to None.
    #[serde(default)]
    hotkey: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationTrashEntry {
    automation: Automation,
    deleted_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomationTrashMutationResult {
    automations: Vec<Automation>,
    trash: Vec<AutomationTrashEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AutomationBackup {
    format: String,
    version: u32,
    exported_at: String,
    automations: Vec<Automation>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomationBackupImportResult {
    automations: Vec<Automation>,
    imported_count: usize,
    replaced_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomationCommandResult {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    process_id: Option<u32>,
}

// One automation run gets one persistent shell process (bash or zsh, matching
// the user's detected login shell), so `cd` and exported environment
// variables carry over between steps exactly as they would in a real
// terminal. A background thread streams its merged stdout/stderr line by
// line into `output_rx`; commands are matched to their output by writing a
// unique sentinel after each one.
struct AutomationSessionHandle {
    child: Child,
    stdin: ChildStdin,
    output_rx: mpsc::Receiver<String>,
}

#[derive(Default)]
struct AutomationSessions(Mutex<HashMap<String, AutomationSessionHandle>>);

// A timed automation schedules an existing Automation to run at a wall-clock
// time, or at that day's actual sunrise/sunset, optionally on specific
// weekdays, through the OS's own scheduler (a systemd --user timer on
// Linux) so it fires even while EasyAlias is not running. `days` uses
// lowercase three-letter abbreviations ("mon".."sun"); empty means every
// day. Run history is intentionally just the most recent attempt - there is
// no UI for a full log, only "did the last run work".
//
// Clock-time entries each get their own exact-fire systemd timer (unchanged
// from before). Sunrise/sunset entries instead ride along on one shared
// periodic "sun checker" timer, since their fire time shifts by roughly a
// minute a day and so can't be baked into a fixed OnCalendar expression;
// `last_triggered_date` stops that periodic check from firing an entry more
// than once on the same calendar day.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimedAutomation {
    id: String,
    automation_id: String,
    #[serde(default = "default_trigger_kind")]
    trigger_kind: String,
    #[serde(default)]
    time: String,
    #[serde(default)]
    days: Vec<String>,
    #[serde(default = "default_true")]
    enabled: bool,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    last_run_at: Option<u64>,
    #[serde(default)]
    last_run_status: Option<String>,
    #[serde(default)]
    last_run_output: Option<String>,
    #[serde(default)]
    last_triggered_date: Option<String>,
}

fn default_trigger_kind() -> String {
    "clock".to_string()
}

fn default_true() -> bool {
    true
}

const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

// Sunrise/sunset triggers only need an approximate location - close enough
// that "sunrise" fires within a few minutes of the real one - not a precise
// street address, so this is a small fixed table of region keys mapped to a
// representative city's coordinates, picked from a dropdown instead of
// typing exact latitude/longitude.
const SUN_REGIONS: &[(&str, &str, f64, f64)] = &[
    ("us-pacific", "US Pacific (Los Angeles)", 34.05, -118.24),
    ("us-mountain", "US Mountain (Denver)", 39.74, -104.99),
    ("us-central", "US Central (Chicago)", 41.88, -87.63),
    ("us-eastern", "US Eastern (New York)", 40.71, -74.01),
    ("uk-ireland", "UK & Ireland (London)", 51.51, -0.13),
    ("eu-west", "EU West (Paris)", 48.86, 2.35),
    ("eu-central", "EU Central (Berlin)", 52.52, 13.40),
    ("eu-east", "EU East (Kyiv)", 50.45, 30.52),
    ("asia-east", "East Asia (Tokyo)", 35.68, 139.69),
    ("asia-south", "South Asia (Delhi)", 28.61, 77.21),
    ("australia", "Australia (Sydney)", -33.87, 151.21),
];

fn sun_region_coordinates(region: &str) -> Option<(f64, f64)> {
    SUN_REGIONS
        .iter()
        .find(|(key, _, _, _)| *key == region)
        .map(|(_, _, latitude, longitude)| (*latitude, *longitude))
}

// A single global setting (not per-automation - the user has one physical
// location) that all sunrise/sunset timed automations share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SunLocationSetting {
    region: String,
}

fn default_sun_location() -> SunLocationSetting {
    SunLocationSetting {
        region: "eu-central".to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SunRegionOption {
    value: String,
    label: String,
}

// App-wide preferences, stored in ~/.easyalias/settings.json. Every field has
// a default so an older or partial file still loads, and new fields can be
// added later without a migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    // "system" follows the OS light/dark setting; "light"/"dark" force one.
    #[serde(default = "default_theme")]
    theme: String,
    // What happens when an automation's global hotkey is pressed:
    // "window" brings EasyAlias forward and shows the run view with live
    // output; "background" runs it headlessly with only a short toast.
    #[serde(default = "default_hotkey_behavior")]
    hotkey_behavior: String,
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_hotkey_behavior() -> String {
    "window".to_string()
}

fn default_app_settings() -> AppSettings {
    AppSettings {
        theme: default_theme(),
        hotkey_behavior: default_hotkey_behavior(),
    }
}

const THEME_VALUES: [&str; 3] = ["system", "light", "dark"];
const HOTKEY_BEHAVIOR_VALUES: [&str; 2] = ["window", "background"];

const AUTOMATION_DONE_MARKER: &str = "__EASYALIAS_AUTOMATION_DONE__";
const AUTOMATION_BG_MARKER: &str = "__EASYALIAS_AUTOMATION_BG__";
const AUTOMATION_BACKUP_FORMAT: &str = "easyalias-automation-backup";
const AUTOMATION_BACKUP_VERSION: u32 = 1;
const MAX_AUTOMATIONS: usize = 200;
const MAX_AUTOMATION_STEPS: usize = 100;
const MAX_AUTOMATION_COMMAND_BYTES: usize = 16 * 1024;
const MAX_AUTOMATION_OUTPUT_CHARS: usize = 20_000;
const MAX_WAIT_SECONDS: u64 = 24 * 60 * 60;
// How often the shared sunrise/sunset checker job re-evaluates every
// sun-triggered timed automation. Tighter than a cosmetic theme check would
// need, since this actually drives real automation runs.
const SUN_CHECK_INTERVAL_SECONDS: u64 = 300;

fn default_command_behavior() -> String {
    "wait".to_string()
}

// EasyAlias owns ~/.easyalias/aliases.sh and adds only these small integration
// lines to the active shell's startup file.
const SOURCE_LINE: &str = "source ~/.easyalias/aliases.sh";
const APP_ALIAS_NAME: &str = "easya";
const APP_ALIAS_LINE: &str = "alias easya='setsid -f easyalias >/dev/null 2>&1'";
const IMPORT_MARKER_CONTENT: &str = "shell alias import prompt handled\n";
const BACKUP_FORMAT: &str = "easyalias-backup";
const BACKUP_VERSION: u32 = 1;
const MAX_BACKUP_BYTES: u64 = 5 * 1024 * 1024;
const MAX_BACKUP_ALIASES: usize = 5000;
const TRASH_RETENTION_SECONDS: u64 = 30 * 24 * 60 * 60;

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

fn trash_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("trash.json"))
}

fn automations_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("automations.json"))
}

fn automation_trash_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("automations-trash.json"))
}

fn timed_automations_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("timed-automations.json"))
}

fn systemd_user_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".config/systemd/user"))
}

fn sun_location_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("sun-location.json"))
}

fn settings_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("settings.json"))
}

fn import_marker_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".shell-import-v1"))
}

// Automations run through the same shell EasyAlias already detected for
// aliases (bash or zsh), so `cd`/exports behave exactly like the user's own
// terminal instead of defaulting to a shell they may not use.
fn automation_shell_binary() -> Result<&'static str, String> {
    Ok(match shell_setup()?.name.as_str() {
        "zsh" => "/bin/zsh",
        _ => "/bin/bash",
    })
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
        validate_alias_entry(alias)?;

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

fn write_trash_entries(entries: &[TrashEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Trash could not be serialized: {}", error))?;
    let path = trash_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
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
        if automation.group.chars().count() > 60 {
            return Err(format!(
                "The group label for \"{}\" must be at most 60 characters.",
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

fn validate_automation_backup_collection(automations: &[Automation]) -> Result<(), String> {
    validate_automations(automations)?;

    let mut names = HashSet::new();
    for automation in automations {
        if !names.insert(automation.name.as_str()) {
            return Err(format!(
                "Duplicate automation name in backup: {}",
                automation.name
            ));
        }
    }

    Ok(())
}

fn read_automation_backup(path: &Path) -> Result<AutomationBackup, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("{} could not be inspected: {}", path.display(), error))?;
    if !metadata.is_file() {
        return Err("Choose an EasyAlias automation JSON backup file.".to_string());
    }
    if metadata.len() > MAX_BACKUP_BYTES {
        return Err("The backup is larger than 5 MB.".to_string());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let backup: AutomationBackup = serde_json::from_str(&content)
        .map_err(|error| format!("This is not a valid EasyAlias automation backup: {}", error))?;

    if backup.format != AUTOMATION_BACKUP_FORMAT || backup.version != AUTOMATION_BACKUP_VERSION {
        return Err("This EasyAlias automation backup format is not supported.".to_string());
    }
    if backup.exported_at.trim().is_empty() {
        return Err("The backup has no export timestamp.".to_string());
    }
    validate_automation_backup_collection(&backup.automations)?;

    Ok(backup)
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

fn parse_time_of_day(value: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("\"{}\" is not a valid time (expected HH:MM).", value));
    }
    let hour = parts[0]
        .parse::<u32>()
        .map_err(|_| format!("\"{}\" is not a valid time (expected HH:MM).", value))?;
    let minute = parts[1]
        .parse::<u32>()
        .map_err(|_| format!("\"{}\" is not a valid time (expected HH:MM).", value))?;
    if hour > 23 || minute > 59 {
        return Err(format!("\"{}\" is not a valid time (expected HH:MM).", value));
    }
    Ok((hour, minute))
}

fn validate_timed_automation(entry: &TimedAutomation, automations: &[Automation]) -> Result<(), String> {
    if entry.id.trim().is_empty() {
        return Err("Every timed automation needs an id.".to_string());
    }
    if !automations
        .iter()
        .any(|automation| automation.id == entry.automation_id)
    {
        return Err("Choose an automation to schedule.".to_string());
    }
    match entry.trigger_kind.as_str() {
        "clock" => {
            parse_time_of_day(&entry.time)?;
        }
        "sunrise" | "sunset" => {
            let location = load_sun_location()?;
            if sun_region_coordinates(&location.region).is_none() {
                return Err("Choose a region for sunrise/sunset scheduling.".to_string());
            }
        }
        other => return Err(format!("\"{}\" is not a valid trigger.", other)),
    }
    for day in &entry.days {
        if !WEEKDAYS.contains(&day.as_str()) {
            return Err(format!("\"{}\" is not a valid weekday.", day));
        }
    }
    Ok(())
}

fn load_sun_location() -> Result<SunLocationSetting, String> {
    ensure_app_files()?;
    let path = sun_location_file()?;
    if !path.exists() {
        return Ok(default_sun_location());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let setting: SunLocationSetting = serde_json::from_str(&content)
        .map_err(|error| format!("sun-location.json is not valid EasyAlias JSON: {}", error))?;
    Ok(setting)
}

fn write_sun_location(setting: &SunLocationSetting) -> Result<(), String> {
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(setting)
        .map_err(|error| format!("Location could not be serialized: {}", error))?;
    let path = sun_location_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn load_app_settings() -> Result<AppSettings, String> {
    ensure_app_files()?;
    let path = settings_file()?;
    if !path.exists() {
        return Ok(default_app_settings());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let settings: AppSettings = serde_json::from_str(&content)
        .map_err(|error| format!("settings.json is not valid EasyAlias JSON: {}", error))?;
    Ok(settings)
}

fn write_app_settings(settings: &AppSettings) -> Result<(), String> {
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("Settings could not be serialized: {}", error))?;
    let path = settings_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn load_timed_automation_entries() -> Result<Vec<TimedAutomation>, String> {
    ensure_app_files()?;
    let path = timed_automations_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let entries: Vec<TimedAutomation> = serde_json::from_str(&content)
        .map_err(|error| format!("timed-automations.json is not valid EasyAlias JSON: {}", error))?;
    Ok(entries)
}

fn write_timed_automation_entries(entries: &[TimedAutomation]) -> Result<(), String> {
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Timed automations could not be serialized: {}", error))?;
    let path = timed_automations_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

// The system's current local wall-clock time. Shells out to `date` instead
// of adding a timezone-aware date/time crate dependency, matching how the
// rest of this file already shells out to systemctl rather than depending
// on a crate for OS integration.
fn current_local_time() -> Result<(u32, u32), String> {
    let output = Command::new("date")
        .arg("+%H:%M")
        .output()
        .map_err(|error| format!("Local time could not be read: {}", error))?;
    if !output.status.success() {
        return Err("Local time could not be read.".to_string());
    }
    parse_time_of_day(String::from_utf8_lossy(&output.stdout).trim())
}

fn today_date_string() -> Result<String, String> {
    let output = Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .map_err(|error| format!("Today's date could not be read: {}", error))?;
    if !output.status.success() {
        return Err("Today's date could not be read.".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn day_of_year() -> Result<u32, String> {
    let output = Command::new("date")
        .arg("+%j")
        .output()
        .map_err(|error| format!("Day of year could not be read: {}", error))?;
    if !output.status.success() {
        return Err("Day of year could not be read.".to_string());
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("Day of year could not be parsed: {}", error))
}

// `%a` is locale-dependent ("Lun" in French, "Mo" in German, ...); forcing
// LC_ALL=C keeps it in English so it matches the lowercase WEEKDAYS keys
// regardless of the system's configured locale.
fn current_weekday_abbrev() -> Result<String, String> {
    let output = Command::new("date")
        .env("LC_ALL", "C")
        .arg("+%a")
        .output()
        .map_err(|error| format!("Weekday could not be read: {}", error))?;
    if !output.status.success() {
        return Err("Weekday could not be read.".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_lowercase())
}

// Parses `date +%z` output ("+0200", "-0530", ...) into a UTC offset in
// minutes. Extracted as its own pure function so it can be unit tested
// without depending on the system's actual timezone.
fn parse_utc_offset(text: &str) -> Result<i64, String> {
    let trimmed = text.trim();
    if trimmed.len() != 5 || !(trimmed.starts_with('+') || trimmed.starts_with('-')) {
        return Err(format!("\"{}\" is not a valid UTC offset (expected +HHMM).", text));
    }
    let sign: i64 = if trimmed.starts_with('-') { -1 } else { 1 };
    let hours: i64 = trimmed[1..3]
        .parse()
        .map_err(|_| format!("\"{}\" is not a valid UTC offset (expected +HHMM).", text))?;
    let minutes: i64 = trimmed[3..5]
        .parse()
        .map_err(|_| format!("\"{}\" is not a valid UTC offset (expected +HHMM).", text))?;
    Ok(sign * (hours * 60 + minutes))
}

fn system_utc_offset_minutes() -> Result<i64, String> {
    let output = Command::new("date")
        .arg("+%z")
        .output()
        .map_err(|error| format!("UTC offset could not be read: {}", error))?;
    if !output.status.success() {
        return Err("UTC offset could not be read.".to_string());
    }
    parse_utc_offset(String::from_utf8_lossy(&output.stdout).trim())
}

// The classic "Sunrise/Sunset Algorithm" (Almanac for Computers, 1990, as
// popularized by edwilliams.org/sunrise_sunset_algorithm.htm). Pure and
// self-contained so it can be unit tested against known reference values
// without needing a real date or timezone. Returns the event as UTC
// minutes-since-midnight, or `None` when the sun does not rise/set that day
// at that latitude (polar day/night).
fn sun_event_utc_minutes(day_of_year: u32, latitude: f64, longitude: f64, is_sunrise: bool) -> Option<f64> {
    const ZENITH: f64 = 90.833; // official zenith for sunrise/sunset (includes refraction + solar radius)
    let to_radians = std::f64::consts::PI / 180.0;
    let to_degrees = 180.0 / std::f64::consts::PI;

    let lng_hour = longitude / 15.0;
    let hour_anchor = if is_sunrise { 6.0 } else { 18.0 };
    let t = day_of_year as f64 + ((hour_anchor - lng_hour) / 24.0);

    let mean_anomaly = (0.9856 * t) - 3.289;

    let mut true_longitude = mean_anomaly
        + (1.916 * (mean_anomaly * to_radians).sin())
        + (0.020 * (2.0 * mean_anomaly * to_radians).sin())
        + 282.634;
    true_longitude = ((true_longitude % 360.0) + 360.0) % 360.0;

    let mut right_ascension = to_degrees * (0.91764 * (true_longitude * to_radians).tan()).atan();
    right_ascension = ((right_ascension % 360.0) + 360.0) % 360.0;
    let longitude_quadrant = (true_longitude / 90.0).floor() * 90.0;
    let ascension_quadrant = (right_ascension / 90.0).floor() * 90.0;
    right_ascension += longitude_quadrant - ascension_quadrant;
    right_ascension /= 15.0;

    let sin_declination = 0.39782 * (true_longitude * to_radians).sin();
    let cos_declination = sin_declination.asin().cos();

    let cos_hour_angle = ((ZENITH * to_radians).cos() - (sin_declination * (latitude * to_radians).sin()))
        / (cos_declination * (latitude * to_radians).cos());
    if !(-1.0..=1.0).contains(&cos_hour_angle) {
        return None;
    }

    let hour_angle_degrees = if is_sunrise {
        360.0 - to_degrees * cos_hour_angle.acos()
    } else {
        to_degrees * cos_hour_angle.acos()
    };
    let hour_angle = hour_angle_degrees / 15.0;

    let local_mean_time = hour_angle + right_ascension - (0.06571 * t) - 6.622;

    let mut utc_hours = local_mean_time - lng_hour;
    utc_hours = ((utc_hours % 24.0) + 24.0) % 24.0;

    Some(utc_hours * 60.0)
}

// Combines a UTC sun-event time with a UTC offset to get local (hour, minute).
fn sun_event_local_time(utc_minutes: f64, utc_offset_minutes: i64) -> (u32, u32) {
    let local = utc_minutes + utc_offset_minutes as f64;
    let normalized = (((local % 1440.0) + 1440.0) % 1440.0).round() as i64 % 1440;
    ((normalized / 60) as u32, (normalized % 60) as u32)
}

// Resolves what time a timed automation should fire at *today*: a fixed
// clock time, or today's actual sunrise/sunset for the configured region.
// `None` for a sun trigger means the sun does not rise/set there today
// (polar day/night) - the caller should just skip it for today.
fn resolve_trigger_time_today(
    entry: &TimedAutomation,
    today_day_of_year: u32,
    utc_offset_minutes: i64,
) -> Result<Option<(u32, u32)>, String> {
    match entry.trigger_kind.as_str() {
        "clock" => Ok(Some(parse_time_of_day(&entry.time)?)),
        "sunrise" | "sunset" => {
            let location = load_sun_location()?;
            let (latitude, longitude) = sun_region_coordinates(&location.region)
                .ok_or_else(|| format!("\"{}\" is not a known region.", location.region))?;
            let is_sunrise = entry.trigger_kind == "sunrise";
            match sun_event_utc_minutes(today_day_of_year, latitude, longitude, is_sunrise) {
                Some(utc_minutes) => Ok(Some(sun_event_local_time(utc_minutes, utc_offset_minutes))),
                None => Ok(None),
            }
        }
        other => Err(format!("\"{}\" is not a valid trigger.", other)),
    }
}

fn timed_automation_unit_name(id: &str) -> String {
    format!("easyalias-timed-{}", id)
}

fn timed_automation_service_path(id: &str) -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.service", timed_automation_unit_name(id))))
}

fn timed_automation_timer_path(id: &str) -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.timer", timed_automation_unit_name(id))))
}

fn weekday_to_systemd(day: &str) -> &'static str {
    match day {
        "mon" => "Mon",
        "tue" => "Tue",
        "wed" => "Wed",
        "thu" => "Thu",
        "fri" => "Fri",
        "sat" => "Sat",
        "sun" => "Sun",
        _ => "Mon",
    }
}

fn run_systemctl_user(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("systemctl").arg("--user").args(args).output()
}

// Every day: "*-*-* HH:MM:00". Specific weekdays: "Mon,Fri *-*-* HH:MM:00".
// Pulled out as its own function so the expression can be checked without
// needing a real systemd instance to run against.
fn timed_automation_on_calendar(entry: &TimedAutomation) -> Result<String, String> {
    let (hour, minute) = parse_time_of_day(&entry.time)?;
    if entry.days.is_empty() {
        Ok(format!("*-*-* {:02}:{:02}:00", hour, minute))
    } else {
        let days = entry
            .days
            .iter()
            .map(|day| weekday_to_systemd(day))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!("{} *-*-* {:02}:{:02}:00", days, hour, minute))
    }
}

// Removes any existing systemd --user timer/service pair for this timed
// automation, regardless of whether one is currently enabled - safe to call
// even if nothing was ever scheduled. Called both on delete and before every
// re-schedule, since an edited unit file needs a daemon-reload to take
// effect.
fn unschedule_timed_automation_linux(id: &str) -> Result<(), String> {
    let unit = format!("{}.timer", timed_automation_unit_name(id));
    let _ = run_systemctl_user(&["disable", "--now", &unit]);

    let service_path = timed_automation_service_path(id)?;
    let timer_path = timed_automation_timer_path(id)?;
    let mut removed_any = false;
    if service_path.exists() {
        fs::remove_file(&service_path).map_err(|error| {
            format!("{} could not be removed: {}", service_path.display(), error)
        })?;
        removed_any = true;
    }
    if timer_path.exists() {
        fs::remove_file(&timer_path).map_err(|error| {
            format!("{} could not be removed: {}", timer_path.display(), error)
        })?;
        removed_any = true;
    }
    if removed_any {
        let _ = run_systemctl_user(&["daemon-reload"]);
    }
    Ok(())
}

// Writes a oneshot systemd --user service (invokes this same executable with
// `--run-timed-automation <id>`) plus a timer unit with the configured
// OnCalendar expression, then enables it. This is what lets a timed
// automation fire even when EasyAlias itself is not open - the systemd user
// instance, not the app, owns the clock. Requires a running systemd --user
// session (the default on virtually all modern desktop distros).
fn schedule_timed_automation_linux(entry: &TimedAutomation) -> Result<(), String> {
    unschedule_timed_automation_linux(&entry.id)?;
    if !entry.enabled {
        return Ok(());
    }

    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;

    let unit_dir = systemd_user_dir()?;
    fs::create_dir_all(&unit_dir)
        .map_err(|error| format!("{} could not be created: {}", unit_dir.display(), error))?;

    let service = format!(
        "[Unit]\nDescription=EasyAlias timed automation {id}\n\n[Service]\nType=oneshot\nExecStart={exe} --run-timed-automation {id}\n",
        id = entry.id,
        exe = exe.display()
    );
    let service_path = timed_automation_service_path(&entry.id)?;
    fs::write(&service_path, service)
        .map_err(|error| format!("{} could not be written: {}", service_path.display(), error))?;

    let on_calendar = timed_automation_on_calendar(entry)?;

    let timer = format!(
        "[Unit]\nDescription=EasyAlias timed automation {id} schedule\n\n[Timer]\nOnCalendar={calendar}\nPersistent=false\n\n[Install]\nWantedBy=timers.target\n",
        id = entry.id,
        calendar = on_calendar
    );
    let timer_path = timed_automation_timer_path(&entry.id)?;
    fs::write(&timer_path, timer)
        .map_err(|error| format!("{} could not be written: {}", timer_path.display(), error))?;

    let reload = run_systemctl_user(&["daemon-reload"])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !reload.status.success() {
        return Err(format!(
            "systemctl daemon-reload failed: {}",
            String::from_utf8_lossy(&reload.stderr).trim()
        ));
    }

    let unit = format!("{}.timer", timed_automation_unit_name(&entry.id));
    let enable = run_systemctl_user(&["enable", "--now", &unit])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !enable.status.success() {
        return Err(format!(
            "systemctl enable failed: {}",
            String::from_utf8_lossy(&enable.stderr).trim()
        ));
    }

    Ok(())
}

fn sun_timed_automations_checker_unit_name() -> &'static str {
    "easyalias-sun-timed-automations"
}

fn sun_timed_automations_checker_service_path() -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.service", sun_timed_automations_checker_unit_name())))
}

fn sun_timed_automations_checker_timer_path() -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.timer", sun_timed_automations_checker_unit_name())))
}

// Removes the shared systemd --user timer/service pair that periodically
// checks sunrise/sunset timed automations, if one is currently enabled -
// safe to call even if nothing was ever scheduled.
fn unschedule_sun_timed_automations_checker_linux() -> Result<(), String> {
    let unit = format!("{}.timer", sun_timed_automations_checker_unit_name());
    let _ = run_systemctl_user(&["disable", "--now", &unit]);

    let service_path = sun_timed_automations_checker_service_path()?;
    let timer_path = sun_timed_automations_checker_timer_path()?;
    let mut removed_any = false;
    if service_path.exists() {
        fs::remove_file(&service_path).map_err(|error| {
            format!("{} could not be removed: {}", service_path.display(), error)
        })?;
        removed_any = true;
    }
    if timer_path.exists() {
        fs::remove_file(&timer_path).map_err(|error| {
            format!("{} could not be removed: {}", timer_path.display(), error)
        })?;
        removed_any = true;
    }
    if removed_any {
        let _ = run_systemctl_user(&["daemon-reload"]);
    }
    Ok(())
}

// Writes a oneshot systemd --user service (invokes this same executable
// with `--check-sun-timed-automations`) plus a timer unit that fires every
// SUN_CHECK_INTERVAL_SECONDS and once shortly after boot (OnBootSec), with
// Persistent=true so a check missed while suspended runs as soon as the
// session is back. One shared job serves every sunrise/sunset timed
// automation, since none of them can be baked into a fixed OnCalendar
// expression the way a clock-time one can.
fn schedule_sun_timed_automations_checker_linux() -> Result<(), String> {
    unschedule_sun_timed_automations_checker_linux()?;

    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;

    let unit_dir = systemd_user_dir()?;
    fs::create_dir_all(&unit_dir)
        .map_err(|error| format!("{} could not be created: {}", unit_dir.display(), error))?;

    let service = format!(
        "[Unit]\nDescription=EasyAlias sunrise/sunset timed automations check\n\n[Service]\nType=oneshot\nExecStart={exe} --check-sun-timed-automations\n",
        exe = exe.display()
    );
    let service_path = sun_timed_automations_checker_service_path()?;
    fs::write(&service_path, service)
        .map_err(|error| format!("{} could not be written: {}", service_path.display(), error))?;

    let timer = format!(
        "[Unit]\nDescription=EasyAlias sunrise/sunset timed automations timer\n\n[Timer]\nOnBootSec=60\nOnUnitActiveSec={interval}\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        interval = SUN_CHECK_INTERVAL_SECONDS
    );
    let timer_path = sun_timed_automations_checker_timer_path()?;
    fs::write(&timer_path, timer)
        .map_err(|error| format!("{} could not be written: {}", timer_path.display(), error))?;

    let reload = run_systemctl_user(&["daemon-reload"])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !reload.status.success() {
        return Err(format!(
            "systemctl daemon-reload failed: {}",
            String::from_utf8_lossy(&reload.stderr).trim()
        ));
    }

    let unit = format!("{}.timer", sun_timed_automations_checker_unit_name());
    let enable = run_systemctl_user(&["enable", "--now", &unit])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !enable.status.success() {
        return Err(format!(
            "systemctl enable failed: {}",
            String::from_utf8_lossy(&enable.stderr).trim()
        ));
    }

    Ok(())
}

// Keeps the shared checker job's scheduled state in sync with whether any
// enabled sunrise/sunset timed automation currently exists - called after
// every save/delete since any of those can change the answer.
fn sync_sun_timed_automations_checker_linux(entries: &[TimedAutomation]) -> Result<(), String> {
    let needed = entries
        .iter()
        .any(|entry| entry.enabled && (entry.trigger_kind == "sunrise" || entry.trigger_kind == "sunset"));
    if needed {
        schedule_sun_timed_automations_checker_linux()
    } else {
        unschedule_sun_timed_automations_checker_linux()
    }
}

// Runs every step of `automation` sequentially through one persistent shell
// session, exactly like an interactive run, but with no frontend to report
// progress to - only the final outcome is recorded by the caller.
fn run_automation_steps_headless(automation: &Automation) -> Result<(), String> {
    let working_directory = automation_working_directory(&automation.path)?;
    let mut session = spawn_automation_session(&working_directory)?;

    for (index, step) in automation.steps.iter().enumerate() {
        if step.kind == "wait" {
            thread::sleep(std::time::Duration::from_secs(step.seconds));
            continue;
        }

        match execute_in_session(&mut session, &step.command, step.behavior == "background") {
            Ok(result) if step.behavior == "background" || result.exit_code == Some(0) => {}
            Ok(result) => {
                let _ = session.child.kill();
                return Err(format!(
                    "Step {} failed (exit code {}): {}",
                    index + 1,
                    result
                        .exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    limited_output(result.stdout.as_bytes())
                ));
            }
            Err(error) => {
                let _ = session.child.kill();
                return Err(error);
            }
        }
    }

    let _ = session.child.kill();
    Ok(())
}

// Entry point for `--run-timed-automation <id>`: no Tauri runtime, no
// window, just load the schedule and the automation it points to, run it,
// and record the outcome so the app can show "last run" next time it opens.
fn run_timed_automation_headless(id: &str) -> Result<(), String> {
    let mut entries = load_timed_automation_entries()?;
    let index = entries
        .iter()
        .position(|entry| entry.id == id)
        .ok_or_else(|| "Timed automation no longer exists.".to_string())?;
    let automation_id = entries[index].automation_id.clone();

    let automations = load_automation_entries()?;
    let automation = automations
        .into_iter()
        .find(|automation| automation.id == automation_id)
        .ok_or_else(|| "Automation no longer exists.".to_string())?;

    let run_result = run_automation_steps_headless(&automation);

    entries[index].last_run_at = unix_timestamp().ok();
    match &run_result {
        Ok(()) => {
            entries[index].last_run_status = Some("success".to_string());
            entries[index].last_run_output = None;
        }
        Err(message) => {
            entries[index].last_run_status = Some("error".to_string());
            entries[index].last_run_output = Some(message.chars().take(500).collect());
        }
    }
    write_timed_automation_entries(&entries)?;

    run_result
}

// Entry point for `--check-sun-timed-automations`: no Tauri runtime, no
// window. Runs on the shared periodic checker (unlike clock-time entries,
// which each get their own exact-fire systemd timer). For every enabled
// sunrise/sunset entry that hasn't already fired today and whose target
// time for today has passed, runs it and records the outcome, same as the
// exact-time path.
fn check_sun_timed_automations() -> Result<(), String> {
    let mut entries = load_timed_automation_entries()?;
    let automations = load_automation_entries()?;
    let now = current_local_time()?;
    let now_minutes = now.0 * 60 + now.1;
    let today = today_date_string()?;
    let today_day_of_year = day_of_year()?;
    let today_weekday = current_weekday_abbrev()?;
    let utc_offset_minutes = system_utc_offset_minutes()?;
    let mut changed = false;

    for index in 0..entries.len() {
        let entry = entries[index].clone();
        if !entry.enabled {
            continue;
        }
        if entry.trigger_kind != "sunrise" && entry.trigger_kind != "sunset" {
            continue;
        }
        if entry.last_triggered_date.as_deref() == Some(today.as_str()) {
            continue;
        }
        if !entry.days.is_empty() && !entry.days.iter().any(|day| day == &today_weekday) {
            continue;
        }

        let target = match resolve_trigger_time_today(&entry, today_day_of_year, utc_offset_minutes) {
            Ok(Some(target)) => target,
            Ok(None) => continue, // polar day/night - no event today
            Err(_) => continue,   // don't let one bad entry block the rest of the batch
        };
        if now_minutes < (target.0 * 60 + target.1) {
            continue;
        }

        let automation = match automations.iter().find(|item| item.id == entry.automation_id) {
            Some(automation) => automation,
            None => continue,
        };

        let run_result = run_automation_steps_headless(automation);
        entries[index].last_triggered_date = Some(today.clone());
        entries[index].last_run_at = unix_timestamp().ok();
        match &run_result {
            Ok(()) => {
                entries[index].last_run_status = Some("success".to_string());
                entries[index].last_run_output = None;
            }
            Err(message) => {
                entries[index].last_run_status = Some("error".to_string());
                entries[index].last_run_output = Some(message.chars().take(500).collect());
            }
        }
        changed = true;
    }

    if changed {
        write_timed_automation_entries(&entries)?;
    }

    Ok(())
}

fn write_automation_trash_entries(entries: &[AutomationTrashEntry]) -> Result<(), String> {
    // Trash is a history, not an active collection. Validate every workflow,
    // but do not apply the active-list count or cross-entry uniqueness limits.
    for entry in entries {
        validate_automations(std::slice::from_ref(&entry.automation))?;
    }
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Automation Trash could not be serialized: {}", error))?;
    let path = automation_trash_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

fn load_automation_trash_entries() -> Result<Vec<AutomationTrashEntry>, String> {
    ensure_app_files()?;
    let path = automation_trash_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let mut entries: Vec<AutomationTrashEntry> =
        serde_json::from_str(&content).map_err(|error| {
            format!(
                "automations-trash.json is not valid EasyAlias JSON: {}",
                error
            )
        })?;
    for entry in &entries {
        validate_automations(std::slice::from_ref(&entry.automation))?;
    }
    let now = unix_timestamp()?;
    let original_len = entries.len();
    entries.retain(|entry| now.saturating_sub(entry.deleted_at) < TRASH_RETENTION_SECONDS);
    entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));

    if entries.len() != original_len {
        write_automation_trash_entries(&entries)?;
    }

    Ok(entries)
}

// Resolves an automation's working directory. "~" and "~/..." expand against
// HOME, matching the tilde convention already used for alias paths elsewhere
// in this app.
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

// Spawns the one persistent shell an automation run drives its steps
// through, with a background thread streaming its merged stdout/stderr into
// a channel so `execute_in_session` can read command output synchronously.
fn spawn_automation_session(working_directory: &Path) -> Result<AutomationSessionHandle, String> {
    let mut child = Command::new(automation_shell_binary()?)
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

#[tauri::command]
fn load_automations() -> Result<Vec<Automation>, String> {
    load_automation_entries()
}

#[tauri::command]
fn save_automations(
    app: tauri::AppHandle,
    automations: Vec<Automation>,
) -> Result<Vec<Automation>, String> {
    write_automation_entries(&automations)?;
    // Keep OS hotkey registrations in step with automations.json - this also
    // frees the binding of an automation deleted from the editor list.
    register_all_automation_hotkeys(&app);
    Ok(automations)
}

#[tauri::command]
fn list_automation_trash() -> Result<Vec<AutomationTrashEntry>, String> {
    load_automation_trash_entries()
}

// Keep the recoverable copy before updating active storage. This mirrors the
// alias trash behavior and avoids losing a workflow if the second write fails.
#[tauri::command]
fn move_automation_to_trash(
    app: tauri::AppHandle,
    id: String,
) -> Result<AutomationTrashMutationResult, String> {
    let result = move_automation_to_trash_inner(&id)?;
    // Free the deleted automation's global hotkey (if any).
    register_all_automation_hotkeys(&app);
    Ok(result)
}

fn move_automation_to_trash_inner(id: &str) -> Result<AutomationTrashMutationResult, String> {
    let mut automations = load_automation_entries()?;
    let index = automations
        .iter()
        .position(|automation| automation.id == id)
        .ok_or_else(|| "Automation no longer exists.".to_string())?;
    let automation = automations.remove(index);
    let mut trash = load_automation_trash_entries()?;
    trash.retain(|entry| entry.automation.id != automation.id);
    trash.push(AutomationTrashEntry {
        automation,
        deleted_at: unix_timestamp()?,
    });
    trash.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));

    write_automation_trash_entries(&trash)?;
    write_automation_entries(&automations)?;

    Ok(AutomationTrashMutationResult { automations, trash })
}

// Restore into active storage first. Name and id conflicts are rejected so a
// deleted workflow never silently replaces a newer active workflow.
#[tauri::command]
fn restore_trash_automation(
    app: tauri::AppHandle,
    id: String,
) -> Result<AutomationTrashMutationResult, String> {
    let result = restore_trash_automation_inner(&id)?;
    // Re-arm the restored automation's global hotkey (if any).
    register_all_automation_hotkeys(&app);
    Ok(result)
}

fn restore_trash_automation_inner(id: &str) -> Result<AutomationTrashMutationResult, String> {
    let mut trash = load_automation_trash_entries()?;
    let index = trash
        .iter()
        .position(|entry| entry.automation.id == id)
        .ok_or_else(|| "Deleted automation no longer exists.".to_string())?;
    let automation = trash[index].automation.clone();
    let mut automations = load_automation_entries()?;

    if automations
        .iter()
        .any(|existing| existing.id == automation.id)
    {
        return Err("An active automation already uses this id.".to_string());
    }
    if automations.iter().any(|existing| {
        existing
            .name
            .trim()
            .eq_ignore_ascii_case(automation.name.trim())
    }) {
        return Err(format!(
            "Automation \"{}\" already exists. Rename or delete the active automation before restoring.",
            automation.name
        ));
    }

    automations.push(automation);
    automations.sort_by(|left, right| {
        right
            .favorite
            .cmp(&left.favorite)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    write_automation_entries(&automations)?;
    trash.remove(index);
    write_automation_trash_entries(&trash)?;

    Ok(AutomationTrashMutationResult { automations, trash })
}

#[tauri::command]
fn permanently_delete_trash_automation(id: String) -> Result<Vec<AutomationTrashEntry>, String> {
    let mut trash = load_automation_trash_entries()?;
    let original_len = trash.len();
    trash.retain(|entry| entry.automation.id != id);
    if trash.len() == original_len {
        return Err("Deleted automation no longer exists.".to_string());
    }
    write_automation_trash_entries(&trash)?;
    Ok(trash)
}

#[tauri::command]
fn empty_automation_trash() -> Result<Vec<AutomationTrashEntry>, String> {
    ensure_app_files()?;
    write_automation_trash_entries(&[])?;
    Ok(Vec::new())
}

// Automation backups use their own envelope so they cannot be confused with
// alias backups. Only the workflows selected in the review dialog are written.
#[tauri::command]
fn export_automation_backup(
    selected_ids: Vec<String>,
    destination: String,
    exported_at: String,
) -> Result<BackupExportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one automation to export.".to_string());
    }
    if destination.trim().is_empty() {
        return Err("Choose where to save the backup.".to_string());
    }
    if exported_at.trim().is_empty() {
        return Err("Export timestamp is missing.".to_string());
    }

    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let selected: Vec<Automation> = load_automation_entries()?
        .into_iter()
        .filter(|automation| selected_id_set.contains(automation.id.as_str()))
        .collect();
    if selected.len() != selected_id_set.len() {
        return Err("Some automations changed. Reopen Export and try again.".to_string());
    }
    validate_automation_backup_collection(&selected)?;

    let backup = AutomationBackup {
        format: AUTOMATION_BACKUP_FORMAT.to_string(),
        version: AUTOMATION_BACKUP_VERSION,
        exported_at,
        automations: selected,
    };
    let json = serde_json::to_string_pretty(&backup)
        .map_err(|error| format!("Backup could not be serialized: {}", error))?;
    let path = PathBuf::from(destination);
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))?;

    Ok(BackupExportResult {
        file: path.display().to_string(),
        exported_count: backup.automations.len(),
    })
}

#[tauri::command]
fn inspect_automation_backup(path: String) -> Result<Vec<Automation>, String> {
    Ok(read_automation_backup(Path::new(&path))?.automations)
}

// Import matches workflows by name. Replacing the complete workflow makes a
// backup restore deterministic, while fresh ids prevent collisions with data
// that was created after the backup.
#[tauri::command]
fn import_automation_backup(
    app: tauri::AppHandle,
    path: String,
    selected_ids: Vec<String>,
    imported_at: String,
) -> Result<AutomationBackupImportResult, String> {
    let result = import_automation_backup_inner(path, selected_ids, imported_at)?;
    // Imported automations may carry their own hotkeys - register them now.
    register_all_automation_hotkeys(&app);
    Ok(result)
}

fn import_automation_backup_inner(
    path: String,
    selected_ids: Vec<String>,
    imported_at: String,
) -> Result<AutomationBackupImportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one automation to import.".to_string());
    }
    if imported_at.trim().is_empty() {
        return Err("Import timestamp is missing.".to_string());
    }

    let backup = read_automation_backup(Path::new(&path))?;
    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let mut selected: Vec<Automation> = backup
        .automations
        .into_iter()
        .filter(|automation| selected_id_set.contains(automation.id.as_str()))
        .collect();
    if selected.len() != selected_id_set.len() {
        return Err("Some selected automations are no longer present in the backup.".to_string());
    }

    let mut current = load_automation_entries()?;
    let selected_names: HashSet<String> = selected
        .iter()
        .map(|automation| automation.name.clone())
        .collect();
    let replaced_count = current
        .iter()
        .filter(|automation| selected_names.contains(&automation.name))
        .count();
    current.retain(|automation| !selected_names.contains(&automation.name));

    let timestamp = unix_timestamp()?;
    for (automation_index, automation) in selected.iter_mut().enumerate() {
        automation.id = format!(
            "automation-backup-{}-{}-{}",
            timestamp, automation_index, automation.id
        );
        automation.updated_at = imported_at.clone();
        for (step_index, step) in automation.steps.iter_mut().enumerate() {
            step.id = format!(
                "automation-backup-step-{}-{}-{}-{}",
                timestamp, automation_index, step_index, step.id
            );
        }
    }

    let imported_count = selected.len();
    current.extend(selected);
    current.sort_by(|left, right| {
        right
            .favorite
            .cmp(&left.favorite)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    write_automation_entries(&current)?;

    Ok(AutomationBackupImportResult {
        automations: current,
        imported_count,
        replaced_count,
    })
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
fn list_timed_automations() -> Result<Vec<TimedAutomation>, String> {
    load_timed_automation_entries()
}

// Validates against the current automation list, persists, and re-syncs the
// OS schedule so it always matches what was just saved - editing the
// trigger/time/days/enabled state takes effect immediately, not just after
// the app restarts. Clock-time entries each get their own exact-fire
// systemd timer; sunrise/sunset entries instead ride the shared checker
// timer, (re)synced against the full list since it serves every such entry
// at once.
#[tauri::command]
fn save_timed_automation(entry: TimedAutomation) -> Result<Vec<TimedAutomation>, String> {
    let automations = load_automation_entries()?;
    validate_timed_automation(&entry, &automations)?;

    let mut entries = load_timed_automation_entries()?;
    match entries.iter().position(|existing| existing.id == entry.id) {
        Some(index) => entries[index] = entry.clone(),
        None => entries.push(entry.clone()),
    }
    write_timed_automation_entries(&entries)?;

    if entry.trigger_kind == "clock" {
        schedule_timed_automation_linux(&entry)?;
    } else {
        // Not a clock entry (any more) - remove a stale individual job left
        // over from switching this entry away from a clock trigger.
        unschedule_timed_automation_linux(&entry.id)?;
    }
    sync_sun_timed_automations_checker_linux(&entries)?;

    Ok(entries)
}

#[tauri::command]
fn delete_timed_automation(id: String) -> Result<Vec<TimedAutomation>, String> {
    let mut entries = load_timed_automation_entries()?;
    let original_len = entries.len();
    entries.retain(|entry| entry.id != id);
    if entries.len() == original_len {
        return Err("Timed automation no longer exists.".to_string());
    }
    write_timed_automation_entries(&entries)?;
    unschedule_timed_automation_linux(&id)?;
    sync_sun_timed_automations_checker_linux(&entries)?;
    Ok(entries)
}

#[tauri::command]
fn list_sun_regions() -> Vec<SunRegionOption> {
    SUN_REGIONS
        .iter()
        .map(|(key, label, _, _)| SunRegionOption {
            value: key.to_string(),
            label: label.to_string(),
        })
        .collect()
}

#[tauri::command]
fn load_sun_location_state() -> Result<SunLocationSetting, String> {
    load_sun_location()
}

#[tauri::command]
fn save_sun_location(setting: SunLocationSetting) -> Result<SunLocationSetting, String> {
    if sun_region_coordinates(&setting.region).is_none() {
        return Err(format!("\"{}\" is not a known region.", setting.region));
    }
    write_sun_location(&setting)?;
    Ok(setting)
}

#[tauri::command]
fn load_settings() -> Result<AppSettings, String> {
    load_app_settings()
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<AppSettings, String> {
    if !THEME_VALUES.contains(&settings.theme.as_str()) {
        return Err(format!("\"{}\" is not a valid theme.", settings.theme));
    }
    if !HOTKEY_BEHAVIOR_VALUES.contains(&settings.hotkey_behavior.as_str()) {
        return Err(format!(
            "\"{}\" is not a valid hotkey behavior.",
            settings.hotkey_behavior
        ));
    }
    write_app_settings(&settings)?;
    Ok(settings)
}

// Assigns, replaces, or clears (accelerator = None) an automation's global
// keyboard shortcut. The OS registration is attempted before anything is
// persisted, so a clash with the system or another app leaves the stored
// automation untouched and its previous shortcut still active.
#[tauri::command]
fn set_automation_hotkey(
    app: tauri::AppHandle,
    id: String,
    accelerator: Option<String>,
) -> Result<Vec<Automation>, String> {
    let mut automations = load_automation_entries()?;
    let index = automations
        .iter()
        .position(|automation| automation.id == id)
        .ok_or_else(|| "Automation no longer exists.".to_string())?;

    let previous = automations[index].hotkey.clone();
    let previous_shortcut = previous.as_deref().and_then(parse_shortcut);
    let global_shortcut = app.global_shortcut();

    match accelerator {
        Some(raw) => {
            let acc = raw.trim().to_string();
            let shortcut = parse_shortcut(&acc)
                .ok_or_else(|| format!("\"{}\" is not a valid keyboard shortcut.", acc))?;

            if let Some(other) = automations.iter().find(|automation| {
                automation.id != id
                    && automation
                        .hotkey
                        .as_deref()
                        .and_then(parse_shortcut)
                        .map(|existing| existing == shortcut)
                        .unwrap_or(false)
            }) {
                return Err(format!(
                    "That shortcut is already assigned to \"{}\".",
                    other.name.trim()
                ));
            }

            if let Some(old) = previous_shortcut {
                let _ = global_shortcut.unregister(old);
            }
            if let Err(error) = global_shortcut.register(shortcut) {
                if let Some(old) = previous_shortcut {
                    let _ = global_shortcut.register(old);
                }
                return Err(format!(
                    "That shortcut is already in use by the system or another app ({}).",
                    error
                ));
            }
            automations[index].hotkey = Some(acc);
        }
        None => {
            if let Some(old) = previous_shortcut {
                let _ = global_shortcut.unregister(old);
            }
            automations[index].hotkey = None;
        }
    }

    write_automation_entries(&automations)?;
    Ok(automations)
}

#[tauri::command]
fn list_trash() -> Result<Vec<TrashEntry>, String> {
    load_trash_entries()
}

// Write the recoverable copy first. If updating active alias files fails, the
// alias can exist in both places temporarily, but user data is never lost.
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

// Name conflicts are rejected so restoring never silently replaces a newer alias.
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
            favorite: false,
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

// Parse an accelerator string ("CmdOrCtrl+Shift+L") into a Shortcut, returning
// None for anything malformed so callers can skip a bad entry instead of
// failing the whole batch.
fn parse_shortcut(accelerator: &str) -> Option<Shortcut> {
    let trimmed = accelerator.trim();
    if trimmed.is_empty() {
        return None;
    }
    Shortcut::from_str(trimmed).ok()
}

// (Re)registers every automation's global hotkey with the OS. Called once at
// startup and again after any change that can add, remove, or free a binding
// (assign/clear, delete, trash restore, backup import). Bad or already-taken
// combos are logged and skipped - one must never block the rest.
fn register_all_automation_hotkeys(app: &tauri::AppHandle) {
    let global_shortcut = app.global_shortcut();
    let _ = global_shortcut.unregister_all();

    let automations = match load_automation_entries() {
        Ok(automations) => automations,
        Err(error) => {
            eprintln!(
                "Automations could not be loaded for hotkey registration: {}",
                error
            );
            return;
        }
    };

    for automation in &automations {
        let Some(accelerator) = automation.hotkey.as_deref() else {
            continue;
        };
        match parse_shortcut(accelerator) {
            Some(shortcut) => {
                if let Err(error) = global_shortcut.register(shortcut) {
                    eprintln!(
                        "Hotkey \"{}\" for automation \"{}\" could not be registered: {}",
                        accelerator, automation.name, error
                    );
                }
            }
            None => eprintln!(
                "Hotkey \"{}\" for automation \"{}\" is not a valid shortcut.",
                accelerator, automation.name
            ),
        }
    }
}

// Fires when any registered global hotkey is pressed. Matches it back to the
// owning automation and either surfaces the run window or runs the automation
// headlessly, per the user's settings. All real work happens on a spawned
// thread so the shortcut callback returns immediately.
fn handle_global_shortcut(app: &tauri::AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }

    let automations = match load_automation_entries() {
        Ok(automations) => automations,
        Err(_) => return,
    };
    let Some(automation) = automations.into_iter().find(|automation| {
        automation
            .hotkey
            .as_deref()
            .and_then(parse_shortcut)
            .map(|existing| &existing == shortcut)
            .unwrap_or(false)
    }) else {
        return;
    };

    let behavior = load_app_settings()
        .map(|settings| settings.hotkey_behavior)
        .unwrap_or_else(|_| default_hotkey_behavior());
    let app = app.clone();

    thread::spawn(move || {
        if behavior == "background" {
            let result = run_automation_steps_headless(&automation);
            let (ok, message) = match &result {
                Ok(()) => (true, String::new()),
                Err(error) => (false, error.clone()),
            };
            let _ = app.emit(
                "automation-hotkey-result",
                serde_json::json!({
                    "name": automation.name,
                    "ok": ok,
                    "message": message,
                }),
            );
        } else {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit("automation-hotkey-fired", automation.id.clone());
        }
    });
}

fn main() {
    // systemd invokes this same executable to fire a timed automation, with
    // no window and no Tauri runtime - handle that before anything else
    // touches the GUI, then exit without ever starting the app.
    let args: Vec<String> = env::args().collect();
    if args.len() >= 3 && args[1] == "--run-timed-automation" {
        let exit_code = match run_timed_automation_headless(&args[2]) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Timed automation {} failed: {}", args[2], error);
                1
            }
        };
        std::process::exit(exit_code);
    }
    if args.len() >= 2 && args[1] == "--check-sun-timed-automations" {
        let exit_code = match check_sun_timed_automations() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Sunrise/sunset timed automations check failed: {}", error);
                1
            }
        };
        std::process::exit(exit_code);
    }

    // dialog = native file/folder picker; opener = GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    handle_global_shortcut(app, shortcut, event.state());
                })
                .build(),
        )
        .manage(AutomationSessions::default())
        .setup(|app| {
            // Bring every saved automation hotkey live, even when the window
            // starts hidden - the shortcuts must work without focusing the app.
            register_all_automation_hotkeys(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_aliases,
            save_aliases,
            load_automations,
            save_automations,
            set_automation_hotkey,
            list_automation_trash,
            move_automation_to_trash,
            restore_trash_automation,
            permanently_delete_trash_automation,
            empty_automation_trash,
            export_automation_backup,
            inspect_automation_backup,
            import_automation_backup,
            start_automation_session,
            run_session_command,
            stop_automation_session,
            list_timed_automations,
            save_timed_automation,
            delete_timed_automation,
            list_sun_regions,
            load_sun_location_state,
            save_sun_location,
            load_settings,
            save_settings,
            list_trash,
            move_alias_to_trash,
            restore_trash_alias,
            permanently_delete_trash_alias,
            empty_trash,
            scan_shell_import,
            dismiss_shell_import,
            import_shell_aliases,
            export_alias_backup,
            inspect_alias_backup,
            import_alias_backup
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
    fn first_start_import_uses_detected_shell_and_creates_backup() {
        let _home_lock = HOME_LOCK.lock().unwrap();
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

    fn test_automation(id: &str, name: &str, command: &str) -> Automation {
        Automation {
            id: id.to_string(),
            name: name.to_string(),
            path: "~/Projects".to_string(),
            steps: vec![AutomationStep {
                id: format!("{}-step", id),
                kind: "command".to_string(),
                command: command.to_string(),
                seconds: 0,
                behavior: "wait".to_string(),
            }],
            favorite: false,
            group: String::new(),
            hotkey: None,
            created_at: "2026-08-25T12:00:00.000Z".to_string(),
            updated_at: "2026-08-25T12:00:00.000Z".to_string(),
        }
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
            favorite: false,
            group: "Backend".to_string(),
            hotkey: None,
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
            favorite: false,
            group: String::new(),
            hotkey: None,
            created_at: "2026-08-24T18:00:00.000Z".to_string(),
            updated_at: "2026-08-24T18:00:00.000Z".to_string(),
        };

        let validation_error = validate_automations(&[automation]).unwrap_err();
        assert!(validation_error.contains("between 1 second and 24 hours"));
    }

    #[test]
    fn rejects_a_group_label_over_the_length_limit() {
        let mut automation = test_automation("grouped", "Grouped", "echo hi");
        automation.group = "g".repeat(61);

        let validation_error = validate_automations(&[automation]).unwrap_err();
        assert!(validation_error.contains("group label"));
    }

    // Reproduces the reported macOS bug this session model was built to fix: a
    // "cd mac_src" step followed by a command that only succeeds from inside
    // that subdirectory. Each step used to run in its own fresh shell, so the
    // `cd` had no effect on the next step.
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

    #[test]
    fn parses_valid_and_rejects_invalid_times() {
        assert_eq!(parse_time_of_day("09:30").unwrap(), (9, 30));
        assert_eq!(parse_time_of_day("00:00").unwrap(), (0, 0));
        assert_eq!(parse_time_of_day("23:59").unwrap(), (23, 59));
        assert!(parse_time_of_day("24:00").is_err());
        assert!(parse_time_of_day("09:60").is_err());
        assert!(parse_time_of_day("not-a-time").is_err());
    }

    fn test_timed_automation(id: &str, automation_id: &str, time: &str) -> TimedAutomation {
        TimedAutomation {
            id: id.to_string(),
            automation_id: automation_id.to_string(),
            trigger_kind: "clock".to_string(),
            time: time.to_string(),
            days: Vec::new(),
            enabled: true,
            created_at: "2026-08-24T18:00:00.000Z".to_string(),
            updated_at: "2026-08-24T18:00:00.000Z".to_string(),
            last_run_at: None,
            last_run_status: None,
            last_run_output: None,
            last_triggered_date: None,
        }
    }

    #[test]
    fn validates_timed_automation_against_its_target() {
        let automation = test_automation("devstart", "DevStart", "echo hi");
        let mut entry = test_timed_automation("timed-1", "devstart", "09:00");
        entry.days = vec!["mon".to_string(), "wed".to_string()];
        assert!(validate_timed_automation(&entry, &[automation.clone()]).is_ok());

        entry.automation_id = "missing".to_string();
        assert!(validate_timed_automation(&entry, &[automation.clone()])
            .unwrap_err()
            .contains("Choose an automation"));

        entry.automation_id = "devstart".to_string();
        entry.days = vec!["someday".to_string()];
        assert!(validate_timed_automation(&entry, &[automation])
            .unwrap_err()
            .contains("weekday"));
    }

    #[test]
    fn builds_the_expected_oncalendar_expression() {
        let mut entry = test_timed_automation("timed-1", "devstart", "09:05");
        assert_eq!(
            timed_automation_on_calendar(&entry).unwrap(),
            "*-*-* 09:05:00"
        );

        entry.days = vec!["mon".to_string(), "fri".to_string()];
        assert_eq!(
            timed_automation_on_calendar(&entry).unwrap(),
            "Mon,Fri *-*-* 09:05:00"
        );
    }

    #[test]
    fn rejects_an_unknown_trigger_kind() {
        let automation = test_automation("devstart", "DevStart", "echo hi");
        let mut entry = test_timed_automation("timed-1", "devstart", "09:00");
        entry.trigger_kind = "moonrise".to_string();
        assert!(validate_timed_automation(&entry, &[automation])
            .unwrap_err()
            .contains("not a valid trigger"));
    }

    // At the equator, day length stays close to 12 hours year-round
    // regardless of season - a solid sanity check for the algorithm that
    // does not depend on an external reference table.
    #[test]
    fn sunrise_and_sunset_are_roughly_12_hours_apart_at_the_equator() {
        for day in [1, 80, 172, 264, 355] {
            let sunrise = sun_event_utc_minutes(day, 0.0, 0.0, true).unwrap();
            let sunset = sun_event_utc_minutes(day, 0.0, 0.0, false).unwrap();
            let day_length = sunset - sunrise;
            assert!(
                (day_length - 720.0).abs() < 20.0,
                "day {} expected ~720 min of daylight at the equator, got {}",
                day,
                day_length
            );
        }
    }

    #[test]
    fn sun_never_sets_near_the_pole_in_local_summer() {
        // Just inside the Arctic Circle around the summer solstice: the sun
        // should not set at all (midnight sun), so cos(hour angle) falls
        // outside [-1, 1] and the function returns None.
        assert!(sun_event_utc_minutes(172, 78.0, 0.0, false).is_none());
    }

    #[test]
    fn sun_never_rises_near_the_pole_in_local_winter() {
        // Same location, opposite solstice - polar night.
        assert!(sun_event_utc_minutes(355, 78.0, 0.0, true).is_none());
    }

    #[test]
    fn converts_utc_sun_event_to_local_time_with_wraparound() {
        assert_eq!(sun_event_local_time(360.0, 120), (8, 0)); // 06:00 UTC + 2h = 08:00
        assert_eq!(sun_event_local_time(30.0, -120), (22, 30)); // 00:30 UTC - 2h wraps to the previous day, 22:30
        assert_eq!(sun_event_local_time(1430.0, 60), (0, 50)); // 23:50 UTC + 1h wraps past midnight to 00:50
    }

    #[test]
    fn parses_valid_and_rejects_invalid_utc_offsets() {
        assert_eq!(parse_utc_offset("+0200").unwrap(), 120);
        assert_eq!(parse_utc_offset("-0530").unwrap(), -330);
        assert_eq!(parse_utc_offset("+0000").unwrap(), 0);
        assert!(parse_utc_offset("0200").is_err());
        assert!(parse_utc_offset("+02:00").is_err());
        assert!(parse_utc_offset("garbage").is_err());
    }

    #[test]
    fn resolves_clock_trigger_time_directly() {
        let entry = test_timed_automation("timed-1", "devstart", "14:30");
        assert_eq!(resolve_trigger_time_today(&entry, 80, 0).unwrap(), Some((14, 30)));
    }

    #[test]
    fn resolves_sunrise_trigger_using_the_configured_region() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let home = TemporaryHome::create();

        write_sun_location(&SunLocationSetting {
            region: "eu-central".to_string(),
        })
        .unwrap();
        let mut entry = test_timed_automation("timed-1", "devstart", "");
        entry.trigger_kind = "sunrise".to_string();
        let resolved = resolve_trigger_time_today(&entry, 172, 120).unwrap();
        assert!(resolved.is_some());

        drop(home);
    }

    #[test]
    fn check_sun_timed_automations_skips_an_entry_already_triggered_today() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let home = TemporaryHome::create();

        write_sun_location(&SunLocationSetting {
            region: "eu-central".to_string(),
        })
        .unwrap();
        let automation = test_automation("devstart", "DevStart", "echo hi");
        write_automation_entries(&[automation]).unwrap();

        let mut entry = test_timed_automation("timed-1", "devstart", "");
        entry.trigger_kind = "sunrise".to_string();
        entry.last_triggered_date = Some(today_date_string().unwrap());
        write_timed_automation_entries(&[entry]).unwrap();

        check_sun_timed_automations().unwrap();

        let entries = load_timed_automation_entries().unwrap();
        assert_eq!(
            entries[0].last_run_at, None,
            "an entry already triggered today should not run again"
        );

        drop(home);
    }

    #[test]
    fn check_sun_timed_automations_skips_a_disabled_entry() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let home = TemporaryHome::create();

        write_sun_location(&SunLocationSetting {
            region: "eu-central".to_string(),
        })
        .unwrap();
        let automation = test_automation("devstart", "DevStart", "echo hi");
        write_automation_entries(&[automation]).unwrap();

        let mut entry = test_timed_automation("timed-1", "devstart", "");
        entry.trigger_kind = "sunrise".to_string();
        entry.enabled = false;
        write_timed_automation_entries(&[entry]).unwrap();

        check_sun_timed_automations().unwrap();

        let entries = load_timed_automation_entries().unwrap();
        assert_eq!(entries[0].last_run_at, None, "a disabled entry should never run");
        assert_eq!(entries[0].last_triggered_date, None);

        drop(home);
    }

    #[test]
    fn app_settings_round_trip_and_default_when_missing() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();

        let defaults = load_app_settings().unwrap();
        assert_eq!(defaults.theme, "system");
        assert_eq!(defaults.hotkey_behavior, "window");

        write_app_settings(&AppSettings {
            theme: "dark".to_string(),
            hotkey_behavior: "background".to_string(),
        })
        .unwrap();

        let reloaded = load_app_settings().unwrap();
        assert_eq!(reloaded.theme, "dark");
        assert_eq!(reloaded.hotkey_behavior, "background");
    }

    #[test]
    fn save_settings_rejects_unknown_values() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();

        assert!(save_settings(AppSettings {
            theme: "sepia".to_string(),
            hotkey_behavior: "window".to_string(),
        })
        .unwrap_err()
        .contains("not a valid theme"));

        assert!(save_settings(AppSettings {
            theme: "light".to_string(),
            hotkey_behavior: "silent".to_string(),
        })
        .unwrap_err()
        .contains("not a valid hotkey behavior"));
    }

    #[test]
    fn partial_settings_file_fills_in_defaults() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _temporary_home = TemporaryHome::create();
        ensure_app_files().unwrap();
        fs::write(settings_file().unwrap(), "{\"theme\":\"light\"}\n").unwrap();

        let settings = load_app_settings().unwrap();
        assert_eq!(settings.theme, "light");
        assert_eq!(settings.hotkey_behavior, "window");
    }

    #[test]
    fn automation_hotkey_survives_a_json_round_trip() {
        let mut automation = test_automation("hk", "Hotkeyed", "echo hi");
        automation.hotkey = Some("CmdOrCtrl+Shift+L".to_string());

        let json = serde_json::to_string(&automation).unwrap();
        let restored: Automation = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.hotkey.as_deref(), Some("CmdOrCtrl+Shift+L"));

        let legacy = json.replace(",\"hotkey\":\"CmdOrCtrl+Shift+L\"", "");
        let restored_legacy: Automation = serde_json::from_str(&legacy).unwrap();
        assert_eq!(restored_legacy.hotkey, None);
    }

    #[test]
    fn parse_shortcut_accepts_accelerators_and_rejects_junk() {
        assert!(parse_shortcut("CmdOrCtrl+Shift+L").is_some());
        assert!(parse_shortcut("  Alt+F4  ").is_some());
        assert!(parse_shortcut("").is_none());
        assert!(parse_shortcut("not a shortcut").is_none());
    }
}
