//! Automation storage, validation, trash, backups and their Tauri commands.

use crate::*;

pub(crate) fn validate_automations(automations: &[Automation]) -> Result<(), String> {
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

pub(crate) fn validate_automation_backup_collection(automations: &[Automation]) -> Result<(), String> {
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

pub(crate) fn read_automation_backup(path: &Path) -> Result<AutomationBackup, String> {
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

pub(crate) fn load_automation_entries() -> Result<Vec<Automation>, String> {
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

pub(crate) fn write_automation_entries(automations: &[Automation]) -> Result<(), String> {
    validate_automations(automations)?;
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(automations)
        .map_err(|error| format!("Automations could not be serialized: {}", error))?;
    let path = automations_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

pub(crate) fn write_automation_trash_entries(entries: &[AutomationTrashEntry]) -> Result<(), String> {
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

pub(crate) fn load_automation_trash_entries() -> Result<Vec<AutomationTrashEntry>, String> {
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
pub(crate) fn automation_working_directory(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    let mut path = if trimmed == "~" {
        home_dir()?
    } else if let Some(relative) = trimmed.strip_prefix("~/") {
        home_dir()?.join(relative)
    } else {
        PathBuf::from(trimmed)
    };


    // A file path (e.g. a config file the automation only edits) is taken
    // to mean "run from the folder that contains it".
    if path.is_file() {
        if let Some(parent) = path.parent() {
            path = parent.to_path_buf();
        }
    }
    if !path.is_dir() {
        return Err(format!(
            "Working directory does not exist: {}",
            path.display()
        ));
    }

    path.canonicalize()
        .map_err(|error| format!("Working directory could not be opened: {}", error))
}

pub(crate) fn limited_output(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .take(MAX_AUTOMATION_OUTPUT_CHARS)
        .collect()
}

#[tauri::command]
pub(crate) fn load_automations() -> Result<Vec<Automation>, String> {
    load_automation_entries()
}

#[tauri::command]
pub(crate) fn save_automations(
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
pub(crate) fn list_automation_trash() -> Result<Vec<AutomationTrashEntry>, String> {
    load_automation_trash_entries()
}

// Keep the recoverable copy before updating active storage. This mirrors the
// alias trash behavior and avoids losing a workflow if the second write fails.
#[tauri::command]
pub(crate) fn move_automation_to_trash(
    app: tauri::AppHandle,
    id: String,
) -> Result<AutomationTrashMutationResult, String> {
    let result = move_automation_to_trash_inner(&id)?;
    // Free the deleted automation's global hotkey (if any).
    register_all_automation_hotkeys(&app);
    Ok(result)
}

pub(crate) fn move_automation_to_trash_inner(id: &str) -> Result<AutomationTrashMutationResult, String> {
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
pub(crate) fn restore_trash_automation(
    app: tauri::AppHandle,
    id: String,
) -> Result<AutomationTrashMutationResult, String> {
    let result = restore_trash_automation_inner(&id)?;
    // Re-arm the restored automation's global hotkey (if any).
    register_all_automation_hotkeys(&app);
    Ok(result)
}

pub(crate) fn restore_trash_automation_inner(id: &str) -> Result<AutomationTrashMutationResult, String> {
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
pub(crate) fn permanently_delete_trash_automation(id: String) -> Result<Vec<AutomationTrashEntry>, String> {
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
pub(crate) fn empty_automation_trash() -> Result<Vec<AutomationTrashEntry>, String> {
    ensure_app_files()?;
    write_automation_trash_entries(&[])?;
    Ok(Vec::new())
}

// Automation backups use their own envelope so they cannot be confused with
// alias backups. Only the workflows selected in the review dialog are written.
#[tauri::command]
pub(crate) fn export_automation_backup(
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
pub(crate) fn inspect_automation_backup(path: String) -> Result<Vec<Automation>, String> {
    Ok(read_automation_backup(Path::new(&path))?.automations)
}

// Import matches workflows by name. Replacing the complete workflow makes a
// backup restore deterministic, while fresh ids prevent collisions with data
// that was created after the backup.
#[tauri::command]
pub(crate) fn import_automation_backup(
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

pub(crate) fn import_automation_backup_inner(
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
