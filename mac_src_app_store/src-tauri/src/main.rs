use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

#[cfg(target_os = "macos")]
use objc2::runtime::Bool;
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSData, NSError, NSString, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions, NSURL,
};

// Must match the frontend AliasEntry shape. serde keeps Rust field names
// idiomatic while exposing camelCase JSON to TypeScript.
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

// Only conservative, single-line aliases are offered for migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZshrcAliasCandidate {
    id: String,
    name: String,
    command: String,
    line_number: usize,
}

// State sent to the WebView. The Store edition exposes connection state rather
// than assuming unrestricted access to the user's home directory.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppState {
    aliases: Vec<AliasEntry>,
    config_file: String,
    alias_target: String,
    zshrc_path: Option<String>,
    zshrc_connected: bool,
    managed_block_present: bool,
    connection_error: Option<String>,
    import_candidates: Vec<ZshrcAliasCandidate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    state: AppState,
    imported_count: usize,
    backup_file: String,
}

// Portable backups use a versioned envelope rather than exposing the internal
// container config directly. This keeps restores compatible across editions.
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

const MANAGED_BLOCK_START: &str = "# >>> EasyAlias managed aliases >>>";
const MANAGED_BLOCK_END: &str = "# <<< EasyAlias managed aliases <<<";
const IMPORT_MARKER_CONTENT: &str = "zshrc import prompt handled\n";
const RESERVED_ALIAS_NAME: &str = "easya";
const BACKUP_FORMAT: &str = "easyalias-backup";
const BACKUP_VERSION: u32 = 1;
const MAX_BACKUP_BYTES: u64 = 5 * 1024 * 1024;
const MAX_BACKUP_ALIASES: usize = 5000;

// Tauri resolves app_data_dir inside the macOS App Sandbox container. All
// structured data, bookmarks, and backups stay there.
fn app_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|error| format!("The EasyAlias app data directory is unavailable: {error}"))
}

fn config_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("config.json"))
}

fn bookmark_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("zshrc.bookmark"))
}

fn import_marker_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join(".zshrc-import-v1"))
}

fn backups_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_dir(app)?.join("backups"))
}

fn ensure_app_files(app: &AppHandle) -> Result<(), String> {
    let directory = app_dir(app)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {error}", directory.display()))?;

    let backups = backups_dir(app)?;
    fs::create_dir_all(&backups)
        .map_err(|error| format!("{} could not be created: {error}", backups.display()))
}

fn read_text_or_empty(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("{} could not be read: {error}", path.display())),
    }
}

fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("System time could not be read: {error}"))
}

fn next_backup_file(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = backups_dir(app)?;
    let timestamp = unix_timestamp()?;

    for suffix in 0..1000 {
        let file_name = if suffix == 0 {
            format!("zshrc-{timestamp}.backup")
        } else {
            format!("zshrc-{timestamp}-{suffix}.backup")
        };
        let candidate = directory.join(file_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("A unique .zshrc backup name could not be created.".to_string())
}

fn write_backup(app: &AppHandle, content: &str) -> Result<PathBuf, String> {
    let path = next_backup_file(app)?;
    fs::write(&path, content)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok(path)
}

// Alias names become shell identifiers, so the accepted character set is
// intentionally strict.
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
        .map_err(|error| format!("{} could not be inspected: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err("Choose an EasyAlias JSON backup file.".to_string());
    }
    if metadata.len() > MAX_BACKUP_BYTES {
        return Err("The backup is larger than 5 MB.".to_string());
    }

    let content = fs::read_to_string(path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let backup: AliasBackup = serde_json::from_str(&content)
        .map_err(|error| format!("This is not a valid EasyAlias backup: {error}"))?;
    if backup.format != BACKUP_FORMAT || backup.version != BACKUP_VERSION {
        return Err("This EasyAlias backup format is not supported.".to_string());
    }
    if backup.exported_at.trim().is_empty() {
        return Err("The backup has no export timestamp.".to_string());
    }
    validate_alias_collection(&backup.aliases)?;
    Ok(backup)
}

fn single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn render_managed_block(aliases: &[AliasEntry]) -> Result<String, String> {
    let mut lines = vec![
        MANAGED_BLOCK_START.to_string(),
        "# Managed by EasyAlias. Edit these aliases in the app.".to_string(),
    ];

    for alias in aliases {
        validate_alias_entry(alias)?;

        lines.push(format!(
            "alias {}={}",
            alias.name,
            single_quote(&alias.command_preview)
        ));
    }

    lines.push(MANAGED_BLOCK_END.to_string());
    Ok(lines.join("\n"))
}

// Remove the previous EasyAlias block while rejecting malformed or duplicated
// markers. Refusing ambiguous input prevents accidental .zshrc damage.
fn without_managed_block(content: &str) -> Result<String, String> {
    let had_trailing_newline = content.ends_with('\n');
    let mut output = Vec::new();
    let mut inside_block = false;
    let mut block_seen = false;

    for line in content.lines() {
        match line.trim() {
            MANAGED_BLOCK_START => {
                if inside_block || block_seen {
                    return Err("The .zshrc contains multiple EasyAlias blocks.".to_string());
                }
                inside_block = true;
                block_seen = true;
            }
            MANAGED_BLOCK_END => {
                if !inside_block {
                    return Err(
                        "The .zshrc contains an unmatched EasyAlias end marker.".to_string()
                    );
                }
                inside_block = false;
            }
            _ if !inside_block => output.push(line),
            _ => {}
        }
    }

    if inside_block {
        return Err("The .zshrc contains an EasyAlias block without an end marker.".to_string());
    }

    let mut result = output.join("\n");
    if had_trailing_newline && !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

fn update_managed_block(content: &str, aliases: &[AliasEntry]) -> Result<String, String> {
    let base = without_managed_block(content)?;
    let block = render_managed_block(aliases)?;
    let mut result = base.trim_end_matches('\n').to_string();

    if !result.is_empty() {
        result.push_str("\n\n");
    }
    result.push_str(&block);
    result.push('\n');
    Ok(result)
}

fn managed_block_present(content: &str) -> bool {
    let mut start_seen = false;
    let mut end_seen = false;

    for line in content.lines() {
        if line.trim() == MANAGED_BLOCK_START {
            start_seen = true;
        } else if line.trim() == MANAGED_BLOCK_END {
            end_seen = true;
        }
    }

    start_seen && end_seen && without_managed_block(content).is_ok()
}

// Decode one shell word without executing zsh.
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
    if !validate_alias_name(name) || name == RESERVED_ALIAS_NAME {
        return None;
    }

    let command = decode_alias_value(assignment[equals_index + 1..].trim_start())?;
    Some(ZshrcAliasCandidate {
        id: format!("zshrc-line-{line_number}"),
        name: name.to_string(),
        command,
        line_number,
    })
}

fn find_zshrc_aliases(content: &str) -> Result<Vec<ZshrcAliasCandidate>, String> {
    // Validate marker structure first, then scan the original lines so ids keep
    // referring to the real .zshrc line numbers even if a user moves the block.
    without_managed_block(content)?;
    let mut inside_block = false;
    let parsed: Vec<ZshrcAliasCandidate> = content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if line.trim() == MANAGED_BLOCK_START {
                inside_block = true;
                return None;
            }
            if line.trim() == MANAGED_BLOCK_END {
                inside_block = false;
                return None;
            }
            if inside_block {
                return None;
            }
            parse_zshrc_alias_line(line, index + 1)
        })
        .collect();
    let mut name_counts: HashMap<String, usize> = HashMap::new();

    for candidate in &parsed {
        *name_counts.entry(candidate.name.clone()).or_default() += 1;
    }

    Ok(parsed
        .into_iter()
        .filter(|candidate| name_counts.get(&candidate.name) == Some(&1))
        .collect())
}

fn replace_imported_alias_lines(content: &str, selected_lines: &HashMap<usize, &str>) -> String {
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();

    for (index, line) in lines.iter_mut().enumerate() {
        if let Some(name) = selected_lines.get(&(index + 1)) {
            *line = format!(": # EasyAlias imported alias {name}");
        }
    }

    lines.join("\n")
}

fn load_config_aliases(app: &AppHandle) -> Result<Vec<AliasEntry>, String> {
    let path = config_file(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {error}"))
}

fn write_config_aliases(app: &AppHandle, aliases: &[AliasEntry]) -> Result<(), String> {
    let config = serde_json::to_string_pretty(aliases)
        .map_err(|error| format!("Aliases could not be serialized: {error}"))?;
    let path = config_file(app)?;
    fs::write(&path, format!("{config}\n"))
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

fn mark_import_handled(app: &AppHandle) -> Result<(), String> {
    let path = import_marker_file(app)?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

fn validate_selected_zshrc(path: &Path) -> Result<(), String> {
    if path.file_name().and_then(|name| name.to_str()) != Some(".zshrc") {
        return Err("Please choose a file named .zshrc.".to_string());
    }
    if !path.is_file() {
        return Err(format!("{} is not a file.", path.display()));
    }
    fs::File::options()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("{} is not readable and writable: {error}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn foundation_error(error: &NSError) -> String {
    error.localizedDescription().to_string()
}

#[cfg(target_os = "macos")]
fn create_bookmark_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let path = path
        .to_str()
        .ok_or_else(|| "The selected .zshrc path is not valid UTF-8.".to_string())?;
    let path = NSString::from_str(path);
    let url = NSURL::fileURLWithPath(&path);
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope,
            None,
            None,
        )
        .map_err(|error| {
            format!(
                "macOS could not create persistent access for .zshrc: {}",
                foundation_error(&error)
            )
        })?;
    Ok(data.to_vec())
}

#[cfg(target_os = "macos")]
fn resolve_bookmark(
    app: &AppHandle,
) -> Result<(objc2::rc::Retained<NSURL>, PathBuf, bool), String> {
    let path = bookmark_file(app)?;
    let bytes = fs::read(&path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let data = NSData::with_bytes(&bytes);
    let mut is_stale = Bool::NO;
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data,
            NSURLBookmarkResolutionOptions::WithSecurityScope,
            None,
            &mut is_stale,
        )
    }
    .map_err(|error| {
        format!(
            "The saved .zshrc permission could not be restored: {}",
            foundation_error(&error)
        )
    })?;
    let resolved_path = url
        .path()
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| "The saved .zshrc bookmark has no file path.".to_string())?;
    Ok((url, resolved_path, is_stale.as_bool()))
}

#[cfg(target_os = "macos")]
fn refresh_bookmark(app: &AppHandle, url: &NSURL) -> Result<(), String> {
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope,
            None,
            None,
        )
        .map_err(|error| {
            format!(
                "The .zshrc permission could not be refreshed: {}",
                foundation_error(&error)
            )
        })?;
    let path = bookmark_file(app)?;
    fs::write(&path, data.to_vec())
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

#[cfg(target_os = "macos")]
fn with_zshrc_access<T>(
    app: &AppHandle,
    operation: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let (url, path, is_stale) = resolve_bookmark(app)?;
    let access_started = unsafe { url.startAccessingSecurityScopedResource() };
    if !access_started {
        return Err(
            "macOS did not grant access to the saved .zshrc. Choose the file again.".to_string(),
        );
    }

    let result = if is_stale {
        refresh_bookmark(app, &url).and_then(|_| operation(&path))
    } else {
        operation(&path)
    };
    unsafe { url.stopAccessingSecurityScopedResource() };
    result
}

#[cfg(not(target_os = "macos"))]
fn create_bookmark_bytes(_path: &Path) -> Result<Vec<u8>, String> {
    Err("The App Store edition only supports macOS.".to_string())
}

#[cfg(not(target_os = "macos"))]
fn with_zshrc_access<T>(
    _app: &AppHandle,
    _operation: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    Err("The App Store edition only supports macOS.".to_string())
}

fn connection_status(app: &AppHandle) -> (bool, Option<String>, bool, Option<String>) {
    if bookmark_file(app).map_or(true, |path| !path.exists()) {
        return (false, None, false, None);
    }

    match with_zshrc_access(app, |path| {
        let content = read_text_or_empty(path)?;
        Ok((path.display().to_string(), managed_block_present(&content)))
    }) {
        Ok((path, block_present)) => (true, Some(path), block_present, None),
        Err(error) => (false, None, false, Some(error)),
    }
}

fn app_state(
    app: &AppHandle,
    aliases: Vec<AliasEntry>,
    import_candidates: Vec<ZshrcAliasCandidate>,
) -> Result<AppState, String> {
    let (connected, zshrc_path, block_present, connection_error) = connection_status(app);
    let alias_target = zshrc_path
        .as_ref()
        .map(|path| format!("{path} (managed block)"))
        .unwrap_or_else(|| "Choose .zshrc to connect".to_string());

    Ok(AppState {
        aliases,
        config_file: config_file(app)?.display().to_string(),
        alias_target,
        zshrc_path,
        zshrc_connected: connected,
        managed_block_present: block_present,
        connection_error,
        import_candidates,
    })
}

fn scan_import_candidates(
    app: &AppHandle,
    aliases: &[AliasEntry],
) -> Result<Vec<ZshrcAliasCandidate>, String> {
    let existing_names: HashSet<&str> = aliases.iter().map(|alias| alias.name.as_str()).collect();
    with_zshrc_access(app, |path| {
        let content = read_text_or_empty(path)?;
        Ok(find_zshrc_aliases(&content)?
            .into_iter()
            .filter(|candidate| !existing_names.contains(candidate.name.as_str()))
            .collect())
    })
}

#[tauri::command]
fn load_aliases(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let aliases = load_config_aliases(&app)?;
    let config_exists = config_file(&app)?.exists();
    let import_was_handled = import_marker_file(&app)?.exists();
    let connected = connection_status(&app).0;
    let import_candidates = if connected && !config_exists && !import_was_handled {
        scan_import_candidates(&app, &aliases)?
    } else {
        Vec::new()
    };
    if connected && !config_exists && !import_was_handled && import_candidates.is_empty() {
        mark_import_handled(&app)?;
    }
    app_state(&app, aliases, import_candidates)
}

// Called immediately after NSOpenPanel returns a user-selected .zshrc. The
// security-scoped bookmark is stored before the temporary panel permission ends.
#[tauri::command]
fn connect_zshrc(app: AppHandle, path: String) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let selected_path = PathBuf::from(path);
    validate_selected_zshrc(&selected_path)?;

    let bookmark = create_bookmark_bytes(&selected_path)?;
    let bookmark_path = bookmark_file(&app)?;
    fs::write(&bookmark_path, bookmark)
        .map_err(|error| format!("{} could not be written: {error}", bookmark_path.display()))?;

    let aliases = load_config_aliases(&app)?;
    let update_result = with_zshrc_access(&app, |zshrc_path| {
        validate_selected_zshrc(zshrc_path)?;
        let content = read_text_or_empty(zshrc_path)?;
        write_backup(&app, &content)?;
        let next_content = update_managed_block(&content, &aliases)?;
        fs::write(zshrc_path, next_content)
            .map_err(|error| format!("{} could not be updated: {error}", zshrc_path.display()))
    });

    if let Err(error) = update_result {
        let _ = fs::remove_file(&bookmark_path);
        return Err(error);
    }

    let config_exists = config_file(&app)?.exists();
    let import_was_handled = import_marker_file(&app)?.exists();
    let import_candidates = if !config_exists && !import_was_handled {
        scan_import_candidates(&app, &aliases)?
    } else {
        Vec::new()
    };
    if !config_exists && !import_was_handled && import_candidates.is_empty() {
        mark_import_handled(&app)?;
    }
    app_state(&app, aliases, import_candidates)
}

#[tauri::command]
fn save_aliases(app: AppHandle, aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    render_managed_block(&aliases)?;
    with_zshrc_access(&app, |path| {
        let content = read_text_or_empty(path)?;
        let next_content = update_managed_block(&content, &aliases)?;
        fs::write(path, next_content)
            .map_err(|error| format!("{} could not be updated: {error}", path.display()))
    })?;
    write_config_aliases(&app, &aliases)?;
    app_state(&app, aliases, Vec::new())
}

#[tauri::command]
fn export_alias_backup(
    app: AppHandle,
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
    let aliases = load_config_aliases(&app)?;
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
        .map_err(|error| format!("Backup could not be serialized: {error}"))?;
    let path = PathBuf::from(destination);
    fs::write(&path, format!("{json}\n"))
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;

    Ok(BackupExportResult {
        file: path.display().to_string(),
        exported_count: backup.aliases.len(),
    })
}

#[tauri::command]
fn inspect_alias_backup(path: String) -> Result<Vec<AliasEntry>, String> {
    Ok(read_backup(Path::new(&path))?.aliases)
}

#[tauri::command]
fn import_alias_backup(
    app: AppHandle,
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

    let mut aliases = load_config_aliases(&app)?;
    let selected_names: HashSet<String> = selected.iter().map(|alias| alias.name.clone()).collect();
    let replaced_count = aliases
        .iter()
        .filter(|alias| selected_names.contains(&alias.name))
        .count();
    aliases.retain(|alias| !selected_names.contains(&alias.name));
    let import_batch = unix_timestamp()?;
    for alias in &mut selected {
        alias.id = format!("backup-{import_batch}-{}", alias.id);
        alias.updated_at = imported_at.clone();
    }
    let imported_count = selected.len();
    aliases.extend(selected);
    aliases.sort_by(|left, right| left.name.cmp(&right.name));
    validate_alias_collection(&aliases)?;

    with_zshrc_access(&app, |zshrc_path| {
        let content = read_text_or_empty(zshrc_path)?;
        let next_content = update_managed_block(&content, &aliases)?;
        fs::write(zshrc_path, next_content)
            .map_err(|error| format!("{} could not be updated: {error}", zshrc_path.display()))
    })?;
    write_config_aliases(&app, &aliases)?;

    Ok(BackupImportResult {
        state: app_state(&app, aliases, Vec::new())?,
        imported_count,
        replaced_count,
    })
}

// A deliberate disconnect removes the block from the old file before dropping
// its bookmark. Structured aliases remain in the container for reconnection.
#[tauri::command]
fn disconnect_zshrc(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let aliases = load_config_aliases(&app)?;
    let path = bookmark_file(&app)?;

    if path.exists() {
        with_zshrc_access(&app, |zshrc_path| {
            let content = read_text_or_empty(zshrc_path)?;
            if managed_block_present(&content) {
                write_backup(&app, &content)?;
                let next_content = without_managed_block(&content)?;
                fs::write(zshrc_path, next_content).map_err(|error| {
                    format!("{} could not be updated: {error}", zshrc_path.display())
                })?;
            }
            Ok(())
        })?;
        fs::remove_file(&path)
            .map_err(|error| format!("{} could not be removed: {error}", path.display()))?;
    }

    app_state(&app, aliases, Vec::new())
}

#[tauri::command]
fn scan_zshrc_import(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let aliases = load_config_aliases(&app)?;
    let import_candidates = scan_import_candidates(&app, &aliases)?;
    app_state(&app, aliases, import_candidates)
}

#[tauri::command]
fn dismiss_zshrc_import(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    mark_import_handled(&app)?;
    app_state(&app, load_config_aliases(&app)?, Vec::new())
}

#[tauri::command]
fn import_zshrc_aliases(
    app: AppHandle,
    selected_ids: Vec<String>,
    timestamp: String,
) -> Result<ImportResult, String> {
    if selected_ids.is_empty() {
        return Err("Select at least one alias to import.".to_string());
    }
    if timestamp.trim().is_empty() {
        return Err("Import timestamp is missing.".to_string());
    }

    ensure_app_files(&app)?;
    let selected_id_set: HashSet<&str> = selected_ids.iter().map(String::as_str).collect();
    let mut aliases = load_config_aliases(&app)?;
    let mut imported_count = 0;
    let mut backup_path = PathBuf::new();

    with_zshrc_access(&app, |zshrc_path| {
        let content = read_text_or_empty(zshrc_path)?;
        let candidates = find_zshrc_aliases(&content)?;
        let selected: Vec<ZshrcAliasCandidate> = candidates
            .into_iter()
            .filter(|candidate| selected_id_set.contains(candidate.id.as_str()))
            .collect();

        if selected.len() != selected_id_set.len() {
            return Err("Some aliases changed in .zshrc. Reopen Import and try again.".to_string());
        }

        let mut names: HashSet<String> = aliases.iter().map(|alias| alias.name.clone()).collect();
        for candidate in &selected {
            if !names.insert(candidate.name.clone()) {
                return Err(format!("Alias \"{}\" already exists.", candidate.name));
            }
        }

        let import_id = unix_timestamp()?;
        for candidate in &selected {
            aliases.push(AliasEntry {
                id: format!("imported-{import_id}-{}", candidate.line_number),
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

        let selected_lines: HashMap<usize, &str> = selected
            .iter()
            .map(|candidate| (candidate.line_number, candidate.name.as_str()))
            .collect();
        let without_imported_lines = replace_imported_alias_lines(&content, &selected_lines);
        let next_content = update_managed_block(&without_imported_lines, &aliases)?;

        backup_path = write_backup(&app, &content)?;
        fs::write(zshrc_path, next_content)
            .map_err(|error| format!("{} could not be updated: {error}", zshrc_path.display()))?;
        imported_count = selected.len();
        Ok(())
    })?;

    write_config_aliases(&app, &aliases)?;
    mark_import_handled(&app)?;
    Ok(ImportResult {
        state: app_state(&app, aliases, Vec::new())?,
        imported_count,
        backup_file: backup_path.display().to_string(),
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            load_aliases,
            connect_zshrc,
            disconnect_zshrc,
            save_aliases,
            export_alias_backup,
            inspect_alias_backup,
            import_alias_backup,
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

    fn alias(name: &str, command: &str) -> AliasEntry {
        AliasEntry {
            id: name.to_string(),
            name: name.to_string(),
            path: String::new(),
            action: "custom".to_string(),
            custom_command: Some(command.to_string()),
            command_preview: command.to_string(),
            favorite: false,
            created_at: "2026-07-26T12:00:00.000Z".to_string(),
            updated_at: "2026-07-26T12:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn parses_aliases_outside_the_managed_block() {
        let content = format!(
            "alias legacy='echo legacy'\n{}\nalias managed='echo managed'\n{}\n",
            MANAGED_BLOCK_START, MANAGED_BLOCK_END
        );
        let candidates = find_zshrc_aliases(&content).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "legacy");
    }

    #[test]
    fn updates_one_managed_block_without_touching_other_lines() {
        let original = "export PATH=/opt/bin:$PATH\n";
        let first = update_managed_block(original, &[alias("ll", "ls -lah")]).unwrap();
        let second = update_managed_block(&first, &[alias("gs", "git status")]).unwrap();

        assert!(second.starts_with(original));
        assert_eq!(second.matches(MANAGED_BLOCK_START).count(), 1);
        assert!(!second.contains("alias ll="));
        assert!(second.contains("alias gs='git status'"));
    }

    #[test]
    fn rejects_ambiguous_managed_markers() {
        let malformed = format!("{MANAGED_BLOCK_START}\nalias ll='ls'\n");
        assert!(without_managed_block(&malformed).is_err());
    }

    #[test]
    fn replaces_only_confirmed_import_lines() {
        let content = "alias ll='ls -lah'\nalias gs='git status'\n";
        let selected = HashMap::from([(2, "gs")]);

        assert_eq!(
            replace_imported_alias_lines(content, &selected),
            "alias ll='ls -lah'\n: # EasyAlias imported alias gs\n"
        );
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
    fn reads_versioned_portable_backup() {
        let path = std::env::temp_dir().join(format!(
            "easyalias-store-backup-test-{}.json",
            unix_timestamp().unwrap()
        ));
        let backup = AliasBackup {
            format: BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION,
            exported_at: "2026-08-14T12:00:00.000Z".to_string(),
            aliases: vec![alias("ll", "ls -lah")],
        };
        fs::write(&path, serde_json::to_string(&backup).unwrap()).unwrap();

        let restored = read_backup(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(restored.aliases.len(), 1);
        assert_eq!(restored.aliases[0].name, "ll");
    }
}
