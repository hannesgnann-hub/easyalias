//! Alias storage, validation, rendering, trash, backups and their Tauri commands.

use crate::*;

// Alias names become shell identifiers, so the accepted character set is
// intentionally strict.
pub(crate) fn validate_alias_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
}

pub(crate) fn validate_alias_entry(alias: &AliasEntry) -> Result<(), String> {
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

pub(crate) fn validate_alias_collection(aliases: &[AliasEntry]) -> Result<(), String> {
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

pub(crate) fn read_backup(path: &Path) -> Result<AliasBackup, String> {
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

pub(crate) fn single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn load_config_aliases(app: &AppHandle) -> Result<Vec<AliasEntry>, String> {
    let path = config_file(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("config.json is not valid alias JSON: {error}"))
}

pub(crate) fn write_config_aliases(app: &AppHandle, aliases: &[AliasEntry]) -> Result<(), String> {
    let config = serde_json::to_string_pretty(aliases)
        .map_err(|error| format!("Aliases could not be serialized: {error}"))?;
    let path = config_file(app)?;
    fs::write(&path, format!("{config}\n"))
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub(crate) fn app_state(
    app: &AppHandle,
    aliases: Vec<AliasEntry>,
    import_candidates: Vec<ShellAliasCandidate>,
) -> Result<AppState, String> {
    let (connected, home_path, block_present, connection_error) = connection_status(app);
    let alias_target = if connected {
        format!("{} (managed blocks)", shell_config_file_display())
    } else {
        "Choose your Home folder to connect shell files".to_string()
    };

    Ok(AppState {
        aliases,
        config_file: config_file(app)?.display().to_string(),
        alias_target,
        home_path,
        home_connected: connected,
        shell_config_file: shell_config_file_display(),
        managed_block_present: block_present,
        connection_error,
        import_candidates,
    })
}

#[tauri::command]
pub(crate) fn load_aliases(app: AppHandle) -> Result<AppState, String> {
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

pub(crate) fn write_trash_entries(app: &AppHandle, entries: &[TrashEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Trash could not be serialized: {error}"))?;
    let path = trash_file(app)?;
    fs::write(&path, format!("{json}\n"))
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub(crate) fn retain_current_trash_entries(entries: &mut Vec<TrashEntry>, now: u64) -> bool {
    let original_len = entries.len();
    entries.retain(|entry| now.saturating_sub(entry.deleted_at) < TRASH_RETENTION_SECONDS);
    entries.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    entries.len() != original_len
}

// Reading the trash also enforces retention. Expired entries are removed from
// disk immediately, so they cannot reappear after an app restart.
pub(crate) fn load_trash_entries(app: &AppHandle) -> Result<Vec<TrashEntry>, String> {
    ensure_app_files(app)?;
    let path = trash_file(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let mut entries: Vec<TrashEntry> = serde_json::from_str(&content)
        .map_err(|error| format!("trash.json is not valid EasyAlias JSON: {error}"))?;

    if retain_current_trash_entries(&mut entries, unix_timestamp()?) {
        write_trash_entries(app, &entries)?;
    }

    Ok(entries)
}

#[tauri::command]
pub(crate) fn save_aliases(app: AppHandle, aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    write_active_aliases(&app, &aliases)?;
    app_state(&app, aliases, Vec::new())
}

pub(crate) fn write_active_aliases(app: &AppHandle, aliases: &[AliasEntry]) -> Result<(), String> {
    ensure_app_files(&app)?;
    render_managed_block(&aliases)?;
    with_home_access(&app, |home| {
        write_managed_shell_blocks(app, home, aliases, false)?;
        Ok(())
    })?;
    write_config_aliases(&app, &aliases)?;
    Ok(())
}

#[tauri::command]
pub(crate) fn list_trash(app: AppHandle) -> Result<Vec<TrashEntry>, String> {
    load_trash_entries(&app)
}

// Write the recoverable copy first. If updating the active alias files then
// fails, the alias may exist in both places, but user data is never lost.
#[tauri::command]
pub(crate) fn move_alias_to_trash(app: AppHandle, id: String) -> Result<TrashMutationResult, String> {
    let mut aliases = load_config_aliases(&app)?;
    let index = aliases
        .iter()
        .position(|alias| alias.id == id)
        .ok_or_else(|| "Alias no longer exists.".to_string())?;
    let alias = aliases.remove(index);
    let mut trash = load_trash_entries(&app)?;
    trash.retain(|entry| entry.alias.id != alias.id);
    trash.push(TrashEntry {
        alias,
        deleted_at: unix_timestamp()?,
    });
    trash.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));

    write_trash_entries(&app, &trash)?;
    write_active_aliases(&app, &aliases)?;

    Ok(TrashMutationResult {
        state: app_state(&app, aliases, Vec::new())?,
        trash,
    })
}

// Restore into active storage before removing the recoverable copy. A name
// conflict is rejected to avoid silently replacing a newer alias.
#[tauri::command]
pub(crate) fn restore_trash_alias(app: AppHandle, id: String) -> Result<TrashMutationResult, String> {
    let mut trash = load_trash_entries(&app)?;
    let index = trash
        .iter()
        .position(|entry| entry.alias.id == id)
        .ok_or_else(|| "Deleted alias no longer exists.".to_string())?;
    let alias = trash[index].alias.clone();
    let mut aliases = load_config_aliases(&app)?;

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
    write_active_aliases(&app, &aliases)?;
    trash.remove(index);
    write_trash_entries(&app, &trash)?;

    Ok(TrashMutationResult {
        state: app_state(&app, aliases, Vec::new())?,
        trash,
    })
}

#[tauri::command]
pub(crate) fn permanently_delete_trash_alias(app: AppHandle, id: String) -> Result<Vec<TrashEntry>, String> {
    let mut trash = load_trash_entries(&app)?;
    let original_len = trash.len();
    trash.retain(|entry| entry.alias.id != id);
    if trash.len() == original_len {
        return Err("Deleted alias no longer exists.".to_string());
    }
    write_trash_entries(&app, &trash)?;
    Ok(trash)
}

#[tauri::command]
pub(crate) fn empty_trash(app: AppHandle) -> Result<Vec<TrashEntry>, String> {
    ensure_app_files(&app)?;
    write_trash_entries(&app, &[])?;
    Ok(Vec::new())
}

// Export only the aliases selected in the review dialog. The backend verifies
// that every requested id still exists before writing the portable JSON file.
#[tauri::command]
pub(crate) fn export_alias_backup(
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
pub(crate) fn inspect_alias_backup(path: String) -> Result<Vec<AliasEntry>, String> {
    Ok(read_backup(Path::new(&path))?.aliases)
}

#[tauri::command]
pub(crate) fn import_alias_backup(
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

    with_home_access(&app, |home| {
        write_managed_shell_blocks(&app, home, &aliases, false)?;
        Ok(())
    })?;
    write_config_aliases(&app, &aliases)?;

    Ok(BackupImportResult {
        state: app_state(&app, aliases, Vec::new())?,
        imported_count,
        replaced_count,
    })
}
