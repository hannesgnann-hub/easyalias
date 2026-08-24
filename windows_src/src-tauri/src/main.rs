use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, Write},
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

// Simple legacy command files discovered in user-owned PATH directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandFileCandidate {
    id: String,
    name: String,
    command: String,
    source_file: String,
    #[serde(skip)]
    source_path: PathBuf,
}

// State returned to the frontend on load/save. Besides aliases, it contains
// display paths and setup status for the UI header.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppState {
    aliases: Vec<AliasEntry>,
    config_file: String,
    // Directory containing generated commands such as test1.cmd.
    command_dir: String,
    // Absolute command_dir value, shown when the user needs to restart Terminal.
    path_entry: String,
    // True when command_dir is already visible through User PATH or process PATH.
    path_configured: bool,
    import_candidates: Vec<CommandFileCandidate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    state: AppState,
    imported_count: usize,
    backup_dir: String,
    warning: Option<String>,
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

// One automation run gets one persistent cmd.exe process, so `cd` and `set`
// environment variables carry over between steps exactly as they would in a
// real Command Prompt window. A background thread streams its merged
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

const APP_ALIAS_NAME: &str = "easya";
const IMPORT_MARKER_CONTENT: &str = "legacy command import prompt handled\n";
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

fn trash_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("trash.json"))
}

fn automations_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("automations.json"))
}

fn import_marker_file() -> Result<PathBuf, String> {
    Ok(app_dir()?.join(".cmd-import-v1"))
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("System time could not be read: {}", error))
}

fn next_import_backup_dir() -> Result<PathBuf, String> {
    let timestamp = unix_timestamp()?;
    for suffix in 0..1000 {
        let name = if suffix == 0 {
            format!("import-backup-{}", timestamp)
        } else {
            format!("import-backup-{}-{}", timestamp, suffix)
        };
        let candidate = app_dir()?.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("A unique command import backup directory could not be created.".to_string())
}

// First-run setup: create ~/.easyalias and the command bin directory. The bin
// directory is where Windows finds aliases once it is present in User PATH.
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

// PATH is a semicolon-separated list on Windows. Comparing paths as plain
// strings is enough here after trimming quotes, trailing slashes, and case.
fn path_contains_command_dir(path_value: &str) -> Result<bool, String> {
    let bin = command_dir()?;
    let needle = normalize_path(&bin.display().to_string());

    Ok(path_value
        .split(';')
        .any(|entry| normalize_path(entry) == needle))
}

// `reg query HKCU\Environment /v Path` returns localized console text around
// the actual value. The stable part is the line that starts with Path and then
// includes a registry type such as REG_SZ or REG_EXPAND_SZ.
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

// Read the persisted user PATH, not only the current process PATH. The current
// process may be stale after setx/reg changes, while HKCU\Environment is what
// future terminals will inherit.
fn user_path_value() -> String {
    if cfg!(test) || !cfg!(windows) {
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

fn mark_import_handled() -> Result<(), String> {
    let path = import_marker_file()?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

// Expand values such as %USERPROFILE% in persisted PATH entries. Unknown
// variables remain untouched and therefore naturally fail the directory check.
fn expand_percent_variables(value: &str) -> String {
    let mut result = String::new();
    let mut remainder = value;

    while let Some(start) = remainder.find('%') {
        result.push_str(&remainder[..start]);
        let after_start = &remainder[start + 1..];
        let Some(end) = after_start.find('%') else {
            result.push_str(&remainder[start..]);
            return result;
        };
        let variable = &after_start[..end];
        if let Some(expanded) = env::var_os(variable) {
            result.push_str(&expanded.to_string_lossy());
        } else {
            result.push('%');
            result.push_str(variable);
            result.push('%');
        }
        remainder = &after_start[end + 1..];
    }

    result.push_str(remainder);
    result
}

fn path_is_within(path: &Path, parent: &Path) -> bool {
    let path = normalize_path(&path.display().to_string());
    let parent = normalize_path(&parent.display().to_string());
    path == parent
        || path
            .strip_prefix(&parent)
            .is_some_and(|suffix| suffix.starts_with('\\') || suffix.starts_with('/'))
}

// Legacy alias files must contain exactly one executable command. Standard
// echo/comment lines are ignored; location-dependent batch syntax is skipped.
fn parse_legacy_command_script(content: &str) -> Option<String> {
    let mut command: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let without_at = trimmed.strip_prefix('@').unwrap_or(trimmed).trim();
        let lower = without_at.to_ascii_lowercase();
        if lower == "echo off"
            || lower.starts_with("rem ")
            || lower == "rem"
            || without_at.starts_with("::")
        {
            continue;
        }

        if without_at.starts_with(':')
            || lower == "setlocal"
            || lower == "endlocal"
            || lower.contains("%~dp0")
            || lower.contains("%~f0")
            || lower.contains("%0")
        {
            return None;
        }

        if command.is_some() {
            return None;
        }
        command = Some(without_at.to_string());
    }

    command.filter(|value| !value.trim().is_empty())
}

fn legacy_path_value() -> String {
    let user_path = user_path_value();
    let process_path = env::var("PATH").unwrap_or_default();
    if user_path.trim().is_empty() {
        process_path
    } else if process_path.trim().is_empty() || process_path == user_path {
        user_path
    } else {
        format!("{};{}", user_path, process_path)
    }
}

fn scan_legacy_command_files() -> Result<Vec<CommandFileCandidate>, String> {
    let home = home_dir()?;
    let home = home.canonicalize().unwrap_or(home);
    let managed_bin = command_dir()?;
    let managed_bin = managed_bin.canonicalize().unwrap_or(managed_bin);
    let mut seen_directories = HashSet::new();
    let mut candidates = Vec::new();

    for entry in legacy_path_value().split(';') {
        let entry = expand_percent_variables(entry.trim().trim_matches('"'));
        if entry.trim().is_empty() {
            continue;
        }

        let directory = PathBuf::from(entry);
        let canonical = match directory.canonicalize() {
            Ok(path) => path,
            Err(_) => continue,
        };
        let directory_key = normalize_path(&canonical.display().to_string());
        if !path_is_within(&canonical, &home)
            || normalize_path(&managed_bin.display().to_string()) == directory_key
            || !seen_directories.insert(directory_key)
        {
            continue;
        }

        let entries = match fs::read_dir(&canonical) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for item in entries.flatten() {
            let path = item.path();
            if !path.is_file() {
                continue;
            }
            let supported_extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
                });
            if !supported_extension {
                continue;
            }

            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if !validate_alias_name(name) || name.eq_ignore_ascii_case(APP_ALIAS_NAME) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(command) = parse_legacy_command_script(&content) else {
                continue;
            };

            candidates.push(CommandFileCandidate {
                id: normalize_path(&path.display().to_string()),
                name: name.to_string(),
                command,
                source_file: display_home_path(path.clone())?,
                source_path: path,
            });
        }
    }

    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for candidate in &candidates {
        *name_counts
            .entry(candidate.name.to_ascii_lowercase())
            .or_default() += 1;
    }
    candidates
        .retain(|candidate| name_counts.get(&candidate.name.to_ascii_lowercase()) == Some(&1));
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    candidates.truncate(200);
    Ok(candidates)
}

// Status for the UI. We accept either persisted User PATH or current process
// PATH because the app may be launched after PATH is already refreshed.
fn path_configured() -> bool {
    path_contains_command_dir(&user_path_value()).unwrap_or(false)
        || env::var("PATH")
            .ok()
            .and_then(|path| path_contains_command_dir(&path).ok())
            .unwrap_or(false)
}

fn persist_user_path(next_path: &str) -> Result<(), String> {
    if cfg!(test) || !cfg!(windows) {
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

// Append EasyAlias' bin directory to User PATH once. Existing PATH entries stay
// untouched, and duplicate EasyAlias entries are avoided by path_contains_command_dir.
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

// Escaping mirrors the frontend so both preview and generated files agree.
// Percent signs need special care because `%NAME%` expands env vars in .cmd.
fn escape_cmd_double_quoted(value: &str) -> String {
    value.replace('%', "%%").replace('"', "\"\"")
}

// Convert app paths to cmd.exe arguments. This intentionally uses
// %USERPROFILE% instead of PowerShell's $HOME because the generated files run
// under cmd.exe.
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

// Rebuild commandPreview from structured fields. This lets older configs from
// the first PowerShell-based Windows prototype migrate automatically to cmd.exe
// commands on load/save without asking the user to recreate aliases.
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

// The generated file is intentionally tiny: @echo off plus the command preview.
// Keeping the file plain makes it easy to inspect with `type name.cmd`.
fn render_cmd_script(alias: &AliasEntry) -> Result<String, String> {
    validate_alias_entry(alias)?;

    Ok(format!("@echo off\r\n{}\r\n", alias.command_preview))
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
        if !names.insert(alias.name.to_ascii_lowercase()) {
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

// Convenience command so typing `easya` can reopen the installed app from cmd.
// The install location can vary, so the script tries common per-user and
// Program Files paths before falling back to Windows' app resolution.
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

// Regenerate the command directory from the structured config:
// - remove stale .cmd files for aliases that were deleted or renamed
// - keep easya.cmd unless the user creates an alias named easya
// - write one fresh .cmd file per alias
fn write_command_scripts(aliases: &[AliasEntry]) -> Result<(), String> {
    let bin = command_dir()?;
    fs::create_dir_all(&bin)
        .map_err(|error| format!("{} could not be created: {}", bin.display(), error))?;

    let mut expected_names = HashSet::new();
    for alias in aliases {
        validate_alias_entry(alias)?;
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
fn app_state(
    aliases: Vec<AliasEntry>,
    import_candidates: Vec<CommandFileCandidate>,
) -> Result<AppState, String> {
    Ok(AppState {
        aliases,
        config_file: display_home_path(config_file()?)?,
        command_dir: display_home_path(command_dir()?)?,
        path_entry: command_dir()?.display().to_string(),
        path_configured: path_configured(),
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
    let aliases = serde_json::from_str::<Vec<AliasEntry>>(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {}", error))?;
    Ok(normalize_aliases(aliases))
}

fn write_alias_data(aliases: &[AliasEntry]) -> Result<(), String> {
    let config = serde_json::to_string_pretty(aliases)
        .map_err(|error| format!("Aliases could not be serialized: {}", error))?;
    write_command_scripts(aliases)?;
    let config_path = config_file()?;
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

// Resolves an automation's working directory. "~" and "~/..." expand against
// USERPROFILE, matching the tilde convention already used for alias paths
// elsewhere in this app. Deliberately not canonicalized: canonicalize() on
// Windows can return an extended-length `\\?\` path, and cmd.exe's `cd`/
// CreateProcess working-directory handling for those prefixes is unreliable
// across Windows versions, so the plain (validated) path is used as-is.
fn automation_working_directory(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    let path = if trimmed == "~" {
        home_dir()?
    } else if let Some(relative) = trimmed.strip_prefix("~/") {
        home_dir()?.join(relative.replace('/', "\\"))
    } else if let Some(relative) = trimmed.strip_prefix("~\\") {
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

    Ok(path)
}

fn limited_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(MAX_AUTOMATION_OUTPUT_CHARS)
        .collect()
}

// Spawns the one persistent cmd.exe an automation run drives its steps
// through, with a background thread streaming its merged stdout/stderr into
// a channel so `execute_in_session` can read command output synchronously.
// `/Q` suppresses cmd.exe echoing each line it reads from stdin back out.
fn spawn_automation_session(working_directory: &Path) -> Result<AutomationSessionHandle, String> {
    let mut child = Command::new("cmd")
        .arg("/Q")
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
// and `set` variables from earlier steps are still in effect. Foreground
// commands wait for a completion sentinel carrying %ERRORLEVEL%; background
// commands only wait for confirmation that the job was started (`start /B`
// has no simple single-line way to report a PID, so background steps always
// report `process_id: None`), so a long-running dev server does not block
// the next step. The command is wrapped in parentheses so `2>&1` redirects
// the whole command line, including any internal `&&`/`|`, into stdout.
fn execute_in_session(
    session: &mut AutomationSessionHandle,
    command: &str,
    background: bool,
) -> Result<AutomationCommandResult, String> {
    let send_error = |error: std::io::Error| format!("Command could not be sent: {}", error);
    let recv_error = || "Automation session ended unexpectedly.".to_string();

    if background {
        write!(
            session.stdin,
            "start \"\" /B cmd /C \"{}\" >nul 2>&1\r\n",
            command
        )
        .map_err(send_error)?;
        write!(session.stdin, "echo {}none\r\n", AUTOMATION_BG_MARKER).map_err(send_error)?;
        session.stdin.flush().map_err(send_error)?;

        loop {
            let line = session.output_rx.recv().map_err(|_| recv_error())?;
            if line.trim_end_matches('\r').starts_with(AUTOMATION_BG_MARKER) {
                return Ok(AutomationCommandResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    process_id: None,
                });
            }
        }
    }

    write!(session.stdin, "({}) 2>&1\r\n", command).map_err(send_error)?;
    write!(session.stdin, "echo {}%ERRORLEVEL%\r\n", AUTOMATION_DONE_MARKER).map_err(send_error)?;
    session.stdin.flush().map_err(send_error)?;

    let mut collected = String::new();
    loop {
        let line = session.output_rx.recv().map_err(|_| recv_error())?;
        let trimmed_line = line.trim_end_matches('\r');
        if let Some(code) = trimmed_line.strip_prefix(AUTOMATION_DONE_MARKER) {
            return Ok(AutomationCommandResult {
                exit_code: code.trim().parse::<i32>().ok(),
                stdout: limited_output(collected.trim_end().as_bytes()),
                stderr: String::new(),
                process_id: None,
            });
        }
        collected.push_str(trimmed_line);
        collected.push('\n');
    }
}

// Called by the frontend when the app starts.
// Also performs first-run file and User PATH setup.
#[tauri::command]
fn load_aliases() -> Result<AppState, String> {
    ensure_app_files()?;
    let config_exists = config_file()?.exists();
    let import_was_handled = import_marker_file()?.exists();
    let import_candidates = if !config_exists && !import_was_handled {
        scan_legacy_command_files()?
    } else {
        Vec::new()
    };
    if !config_exists && !import_was_handled && import_candidates.is_empty() {
        mark_import_handled()?;
    }

    ensure_path_contains_command_dir()?;
    let aliases = load_config_aliases()?;
    write_command_scripts(&aliases)?;
    app_state(aliases, import_candidates)
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

    write_alias_data(&aliases)?;
    app_state(aliases, Vec::new())
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

// Ends an automation run's session, either because the run finished or
// because the user clicked Stop. Killing this specific process interrupts a
// stuck foreground command without touching background jobs it already
// started with `start /B`, which keep running detached.
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
    write_alias_data(&aliases)?;

    Ok(TrashMutationResult {
        state: app_state(aliases, Vec::new())?,
        trash,
    })
}

// Restore into active storage before removing the recoverable copy. A name
// conflict is rejected to avoid silently replacing a newer alias.
#[tauri::command]
fn restore_trash_alias(id: String) -> Result<TrashMutationResult, String> {
    let mut trash = load_trash_entries()?;
    let index = trash
        .iter()
        .position(|entry| entry.alias.id == id)
        .ok_or_else(|| "Deleted alias no longer exists.".to_string())?;
    let alias = trash[index].alias.clone();
    let mut aliases = load_config_aliases()?;

    if aliases
        .iter()
        .any(|existing| existing.name.eq_ignore_ascii_case(&alias.name))
    {
        return Err(format!(
            "Alias \"{}\" already exists. Rename or delete the active alias before restoring.",
            alias.name
        ));
    }
    if aliases.iter().any(|existing| existing.id == alias.id) {
        return Err("An active alias already uses this id.".to_string());
    }

    aliases.push(alias);
    aliases.sort_by_key(|alias| alias.name.to_ascii_lowercase());
    write_alias_data(&aliases)?;
    trash.remove(index);
    write_trash_entries(&trash)?;

    Ok(TrashMutationResult {
        state: app_state(aliases, Vec::new())?,
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

// Merge selected backup entries by alias name. Windows command names are case
// insensitive, so conflicts are compared in lowercase before replacement.
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

    selected = normalize_aliases(selected);
    let mut aliases = load_config_aliases()?;
    let selected_names: HashSet<String> = selected
        .iter()
        .map(|alias| alias.name.to_ascii_lowercase())
        .collect();
    let replaced_count = aliases
        .iter()
        .filter(|alias| selected_names.contains(&alias.name.to_ascii_lowercase()))
        .count();
    aliases.retain(|alias| !selected_names.contains(&alias.name.to_ascii_lowercase()));

    for alias in &mut selected {
        // New ids avoid collisions when a backup is imported more than once.
        alias.id = format!("backup-{}-{}", unix_timestamp()?, alias.id);
        alias.updated_at = imported_at.clone();
    }
    let imported_count = selected.len();
    aliases.extend(selected);
    aliases.sort_by_key(|alias| alias.name.to_ascii_lowercase());
    validate_alias_collection(&aliases)?;
    write_alias_data(&aliases)?;

    Ok(BackupImportResult {
        state: app_state(aliases, Vec::new())?,
        imported_count,
        replaced_count,
    })
}

// Manually scan user-owned PATH folders when Import is opened from the header.
// This intentionally ignores the first-start marker so command files created
// later remain importable. Windows command names are case-insensitive, so names
// already managed by EasyAlias are filtered with lowercase comparisons.
#[tauri::command]
fn scan_command_file_import() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_path_contains_command_dir()?;

    let aliases = load_config_aliases()?;
    let existing_names: HashSet<String> = aliases
        .iter()
        .map(|alias| alias.name.to_ascii_lowercase())
        .collect();
    let import_candidates = scan_legacy_command_files()?
        .into_iter()
        .filter(|candidate| !existing_names.contains(&candidate.name.to_ascii_lowercase()))
        .collect();

    app_state(aliases, import_candidates)
}

#[tauri::command]
fn dismiss_command_file_import() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_path_contains_command_dir()?;
    mark_import_handled()?;
    app_state(load_config_aliases()?, Vec::new())
}

#[tauri::command]
fn import_command_files(
    selected_ids: Vec<String>,
    timestamp: String,
) -> Result<ImportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one command file to import.".to_string());
    }
    if timestamp.trim().is_empty() {
        return Err("Import timestamp is missing.".to_string());
    }

    ensure_app_files()?;
    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let selected: Vec<CommandFileCandidate> = scan_legacy_command_files()?
        .into_iter()
        .filter(|candidate| selected_id_set.contains(candidate.id.as_str()))
        .collect();
    if selected.len() != selected_id_set.len() {
        return Err("Some command files changed. Reopen EasyAlias and try again.".to_string());
    }

    ensure_path_contains_command_dir()?;
    let mut aliases = load_config_aliases()?;
    let mut names: HashSet<String> = aliases
        .iter()
        .map(|alias| alias.name.to_ascii_lowercase())
        .collect();
    for candidate in &selected {
        if !names.insert(candidate.name.to_ascii_lowercase()) {
            return Err(format!("Alias \"{}\" already exists.", candidate.name));
        }
    }

    let import_id = unix_timestamp()?;
    for (index, candidate) in selected.iter().enumerate() {
        aliases.push(AliasEntry {
            id: format!("imported-{}-{}", import_id, index),
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

    let backup_dir = next_import_backup_dir()?;
    fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("{} could not be created: {}", backup_dir.display(), error))?;
    for candidate in &selected {
        let file_name = candidate
            .source_path
            .file_name()
            .ok_or_else(|| format!("{} has no file name.", candidate.source_file))?;
        let backup_file = backup_dir.join(file_name);
        fs::copy(&candidate.source_path, &backup_file).map_err(|error| {
            format!(
                "{} could not be backed up to {}: {}",
                candidate.source_path.display(),
                backup_file.display(),
                error
            )
        })?;
    }

    write_alias_data(&aliases)?;
    let mut removal_failures = Vec::new();
    for candidate in &selected {
        if let Err(error) = fs::remove_file(&candidate.source_path) {
            removal_failures.push(format!("{}: {}", candidate.source_file, error));
        }
    }
    mark_import_handled()?;

    let warning = if removal_failures.is_empty() {
        None
    } else {
        Some(format!(
            "Imported successfully, but these original files could not be removed: {}",
            removal_failures.join("; ")
        ))
    };

    Ok(ImportResult {
        state: app_state(aliases, Vec::new())?,
        imported_count: selected.len(),
        backup_dir: display_home_path(backup_dir)?,
        warning,
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
            scan_command_file_import,
            dismiss_command_file_import,
            import_command_files
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

    struct TemporaryProfile {
        path: PathBuf,
        user_profile: Option<OsString>,
        home: Option<OsString>,
        path_value: Option<OsString>,
    }

    impl TemporaryProfile {
        fn create() -> Self {
            let path = env::temp_dir().join(format!(
                "easyalias-windows-import-test-{}-{}",
                std::process::id(),
                unix_timestamp().unwrap()
            ));
            fs::create_dir_all(&path).unwrap();
            let user_profile = env::var_os("USERPROFILE");
            let home = env::var_os("HOME");
            let path_value = env::var_os("PATH");
            env::set_var("USERPROFILE", &path);
            env::set_var("HOME", &path);
            Self {
                path,
                user_profile,
                home,
                path_value,
            }
        }
    }

    impl Drop for TemporaryProfile {
        fn drop(&mut self) {
            for (name, value) in [
                ("USERPROFILE", &self.user_profile),
                ("HOME", &self.home),
                ("PATH", &self.path_value),
            ] {
                if let Some(value) = value {
                    env::set_var(name, value);
                } else {
                    env::remove_var(name);
                }
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
    fn parses_only_simple_command_files() {
        assert_eq!(
            parse_legacy_command_script("@echo off\r\ngit status --short %*\r\n"),
            Some("git status --short %*".to_string())
        );
        assert!(parse_legacy_command_script("echo one\necho two\n").is_none());
        assert!(parse_legacy_command_script("@echo off\ncall %~dp0tool.cmd %*\n").is_none());
        assert!(parse_legacy_command_script("@echo off\n:label\n").is_none());
    }

    #[test]
    fn old_alias_json_defaults_to_not_favorite() {
        let legacy = r#"{
            "id":"legacy-1","name":"ll","path":"","action":"custom",
            "customCommand":"dir /a","commandPreview":"dir /a",
            "createdAt":"2026-07-01T12:00:00.000Z","updatedAt":"2026-07-01T12:00:00.000Z"
        }"#;

        let alias: AliasEntry = serde_json::from_str(legacy).unwrap();
        assert!(!alias.favorite);
    }

    #[test]
    fn deleted_alias_can_be_restored_from_trash() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _profile = TemporaryProfile::create();
        ensure_app_files().unwrap();
        write_alias_data(&[test_alias("one", "ll", "dir /a")]).unwrap();

        let deleted = move_alias_to_trash("one".to_string()).unwrap();
        assert!(deleted.state.aliases.is_empty());
        assert_eq!(deleted.trash.len(), 1);
        assert_eq!(deleted.trash[0].alias.name, "ll");
        assert!(!command_file("ll").unwrap().exists());

        let restored = restore_trash_alias("one".to_string()).unwrap();
        assert_eq!(restored.state.aliases.len(), 1);
        assert!(restored.trash.is_empty());
        assert!(fs::read_to_string(command_file("ll").unwrap())
            .unwrap()
            .contains("dir /a"));
    }

    #[test]
    fn expired_trash_entries_are_removed_automatically() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let _profile = TemporaryProfile::create();
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
    fn first_start_import_backs_up_and_moves_command_file() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let profile = TemporaryProfile::create();
        let legacy_dir = profile.path.join("aliases");
        fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_file = legacy_dir.join("gst.cmd");
        fs::write(&legacy_file, "@echo off\r\ngit status --short %*\r\n").unwrap();
        env::set_var("PATH", legacy_dir.display().to_string());

        let initial = load_aliases().unwrap();
        assert_eq!(initial.import_candidates.len(), 1);
        assert_eq!(initial.import_candidates[0].name, "gst");

        let result = import_command_files(
            vec![initial.import_candidates[0].id.clone()],
            "2026-07-18T10:00:00.000Z".to_string(),
        )
        .unwrap();

        assert_eq!(result.imported_count, 1);
        assert!(result.warning.is_none());
        assert!(!legacy_file.exists());
        assert!(profile
            .path
            .join(result.backup_dir.trim_start_matches("~/"))
            .join("gst.cmd")
            .exists());
        assert!(fs::read_to_string(command_file("gst").unwrap())
            .unwrap()
            .contains("git status --short %*"));
        assert_eq!(load_config_aliases().unwrap().len(), 1);
    }

    #[test]
    fn backup_export_contains_only_selected_aliases() {
        let _home_lock = HOME_LOCK.lock().unwrap();
        let profile = TemporaryProfile::create();
        ensure_app_files().unwrap();
        write_alias_data(&[
            test_alias("one", "ll", "dir /a"),
            test_alias("two", "gs", "git status"),
        ])
        .unwrap();
        let destination = profile.path.join("selected.json");

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
        let profile = TemporaryProfile::create();
        ensure_app_files().unwrap();
        write_alias_data(&[
            test_alias("current-ll", "ll", "dir"),
            test_alias("keep", "gs", "git status"),
        ])
        .unwrap();
        let backup_path = profile.path.join("restore.json");
        let backup = AliasBackup {
            format: BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION,
            exported_at: "2026-08-13T18:30:00.000Z".to_string(),
            aliases: vec![
                test_alias("backup-ll", "LL", "dir /a"),
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
            .any(|alias| alias.name == "LL" && alias.command_preview == "dir /a"));
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

    // These two spawn a real cmd.exe, so they only run on Windows (there is no
    // `cmd` binary to spawn when `cargo test` runs on macOS/Linux). They
    // reproduce the same scenario as the macOS session tests: a `cd` step
    // followed by a command that only succeeds from inside that subdirectory,
    // and a backgrounded command that must not block the next step.
    #[cfg(target_os = "windows")]
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

        let cwd_result = execute_in_session(&mut session, "echo %CD%", false).unwrap();
        assert_eq!(cwd_result.exit_code, Some(0));
        assert!(
            cwd_result.stdout.trim().to_lowercase().ends_with("\\mac_src"),
            "expected %CD% to report the subdirectory, got: {:?}",
            cwd_result.stdout
        );

        let _ = session.child.kill();
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn automation_session_background_command_does_not_block_next_step() {
        let base = env::temp_dir().join(format!(
            "easyalias-automation-session-bg-test-{}",
            unix_timestamp().unwrap()
        ));
        fs::create_dir_all(&base).unwrap();

        let mut session = spawn_automation_session(&base).unwrap();

        let bg_result = execute_in_session(&mut session, "ping 127.0.0.1 -n 6", true).unwrap();
        assert_eq!(bg_result.exit_code, None);

        let echo_result = execute_in_session(&mut session, "echo still-here", false).unwrap();
        assert_eq!(echo_result.exit_code, Some(0));
        assert_eq!(echo_result.stdout.trim(), "still-here");

        let _ = session.child.kill();
        let _ = fs::remove_dir_all(&base);
    }
}
