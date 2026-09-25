//! Importing existing .cmd shortcut files.

use crate::*;

pub(crate) fn next_import_backup_dir() -> Result<PathBuf, String> {
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

pub(crate) fn mark_import_handled() -> Result<(), String> {
    let path = import_marker_file()?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

// Legacy alias files must contain exactly one executable command. Standard
// echo/comment lines are ignored; location-dependent batch syntax is skipped.
pub(crate) fn parse_legacy_command_script(content: &str) -> Option<String> {
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

pub(crate) fn legacy_path_value() -> String {
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

pub(crate) fn scan_legacy_command_files() -> Result<Vec<CommandFileCandidate>, String> {
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

// Manually scan user-owned PATH folders when Import is opened from the header.
// This intentionally ignores the first-start marker so command files created
// later remain importable. Windows command names are case-insensitive, so names
// already managed by EasyAlias are filtered with lowercase comparisons.
pub(crate) fn scan_command_file_import() -> Result<AppState, String> {
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

pub(crate) fn dismiss_command_file_import() -> Result<AppState, String> {
    ensure_app_files()?;
    ensure_path_contains_command_dir()?;
    mark_import_handled()?;
    app_state(load_config_aliases()?, Vec::new())
}

pub(crate) fn import_command_files(
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
