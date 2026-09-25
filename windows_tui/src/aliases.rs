//! Alias storage, validation, rendering, trash, backups and their Tauri commands.

use crate::*;

// Alias names become command file names, so the accepted character set is strict.
pub(crate) fn validate_alias_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|char| char.is_ascii_alphanumeric() || char == '_' || char == '-')
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
        if !names.insert(alias.name.to_ascii_lowercase()) {
            return Err(format!("Duplicate alias name in backup: {}", alias.name));
        }
    }

    Ok(())
}

pub(crate) fn read_backup(path: &Path) -> Result<AliasBackup, String> {
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

// Build a complete AppState after load/save.
pub(crate) fn app_state(
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

pub(crate) fn load_config_aliases() -> Result<Vec<AliasEntry>, String> {
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

pub(crate) fn write_trash_entries(entries: &[TrashEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Trash could not be serialized: {}", error))?;
    let path = trash_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

// Reading the trash also enforces retention. Expired entries are removed from
// disk immediately, so they cannot reappear after an app restart.
pub(crate) fn load_trash_entries() -> Result<Vec<TrashEntry>, String> {
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

// Called by the frontend when the app starts.
// Also performs first-run file and User PATH setup.
pub(crate) fn load_aliases() -> Result<AppState, String> {
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
pub(crate) fn save_aliases(aliases: Vec<AliasEntry>) -> Result<AppState, String> {
    let aliases = normalize_aliases(aliases);
    let directory = app_dir()?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {}", directory.display(), error))?;

    ensure_path_contains_command_dir()?;

    write_alias_data(&aliases)?;
    app_state(aliases, Vec::new())
}

pub(crate) fn list_trash() -> Result<Vec<TrashEntry>, String> {
    load_trash_entries()
}

// Write the recoverable copy first. If updating the active alias files then
// fails, the alias may exist in both places, but user data is never lost.
pub(crate) fn move_alias_to_trash(id: String) -> Result<TrashMutationResult, String> {
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
pub(crate) fn restore_trash_alias(id: String) -> Result<TrashMutationResult, String> {
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

pub(crate) fn permanently_delete_trash_alias(id: String) -> Result<Vec<TrashEntry>, String> {
    let mut trash = load_trash_entries()?;
    let original_len = trash.len();
    trash.retain(|entry| entry.alias.id != id);
    if trash.len() == original_len {
        return Err("Deleted alias no longer exists.".to_string());
    }
    write_trash_entries(&trash)?;
    Ok(trash)
}

pub(crate) fn empty_trash() -> Result<Vec<TrashEntry>, String> {
    ensure_app_files()?;
    write_trash_entries(&[])?;
    Ok(Vec::new())
}

// Export only the aliases selected in the review dialog. The backend verifies
// that every requested id still exists before writing the portable JSON file.
pub(crate) fn export_alias_backup(
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
pub(crate) fn inspect_alias_backup(path: String) -> Result<Vec<AliasEntry>, String> {
    Ok(read_backup(Path::new(&path))?.aliases)
}

// Merge selected backup entries by alias name. Windows command names are case
// insensitive, so conflicts are compared in lowercase before replacement.
pub(crate) fn import_alias_backup(
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
