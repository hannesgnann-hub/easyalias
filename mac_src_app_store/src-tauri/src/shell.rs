//! Shell startup files: connecting EasyAlias, and importing existing aliases.

use crate::*;

pub(crate) fn ensure_app_files(app: &AppHandle) -> Result<(), String> {
    let directory = app_dir(app)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("{} could not be created: {error}", directory.display()))?;

    let backups = backups_dir(app)?;
    fs::create_dir_all(&backups)
        .map_err(|error| format!("{} could not be created: {error}", backups.display()))
}

pub(crate) fn next_backup_file(app: &AppHandle, source: &Path) -> Result<PathBuf, String> {
    let directory = backups_dir(app)?;
    let timestamp = unix_timestamp()?;
    let source_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shellrc")
        .trim_start_matches('.');

    for suffix in 0..1000 {
        let file_name = if suffix == 0 {
            format!("{source_name}-{timestamp}.backup")
        } else {
            format!("{source_name}-{timestamp}-{suffix}.backup")
        };
        let candidate = directory.join(file_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("A unique shell startup file backup name could not be created.".to_string())
}

pub(crate) fn write_backup(app: &AppHandle, source: &Path, content: &str) -> Result<PathBuf, String> {
    let path = next_backup_file(app, source)?;
    fs::write(&path, content)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok(path)
}

pub(crate) fn shell_config_file_display() -> String {
    "~/.zshrc, ~/.bash_profile and ~/.bashrc".to_string()
}

pub(crate) fn render_managed_block(aliases: &[AliasEntry]) -> Result<String, String> {
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
// markers. Refusing ambiguous input prevents accidental shell-file damage.
pub(crate) fn without_managed_block(content: &str) -> Result<String, String> {
    let had_trailing_newline = content.ends_with('\n');
    let mut output = Vec::new();
    let mut inside_block = false;
    let mut block_seen = false;

    for line in content.lines() {
        match line.trim() {
            MANAGED_BLOCK_START => {
                if inside_block || block_seen {
                    return Err(
                        "A shell startup file contains multiple EasyAlias blocks.".to_string()
                    );
                }
                inside_block = true;
                block_seen = true;
            }
            MANAGED_BLOCK_END => {
                if !inside_block {
                    return Err(
                        "A shell startup file contains an unmatched EasyAlias end marker."
                            .to_string(),
                    );
                }
                inside_block = false;
            }
            _ if !inside_block => output.push(line),
            _ => {}
        }
    }

    if inside_block {
        return Err(
            "A shell startup file contains an EasyAlias block without an end marker.".to_string(),
        );
    }

    let mut result = output.join("\n");
    if had_trailing_newline && !result.is_empty() {
        result.push('\n');
    }
    Ok(result)
}

pub(crate) fn update_managed_block(content: &str, aliases: &[AliasEntry]) -> Result<String, String> {
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

pub(crate) fn managed_block_present(content: &str) -> bool {
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
pub(crate) fn decode_alias_value(value: &str) -> Option<String> {
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

pub(crate) fn parse_shell_alias_line(line: &str, line_number: usize) -> Option<ShellAliasCandidate> {
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
    Some(ShellAliasCandidate {
        id: format!("shell-line-{line_number}"),
        name: name.to_string(),
        command,
        line_number,
        source_file: String::new(),
    })
}

pub(crate) fn find_shell_aliases(content: &str) -> Result<Vec<ShellAliasCandidate>, String> {
    // Validate marker structure first, then scan the original lines so ids keep
    // referring to the real startup-file line numbers if the block is moved.
    without_managed_block(content)?;
    let mut inside_block = false;
    let parsed: Vec<ShellAliasCandidate> = content
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
            parse_shell_alias_line(line, index + 1)
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

pub(crate) fn replace_imported_alias_lines(content: &str, selected_lines: &HashMap<usize, &str>) -> String {
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();

    for (index, line) in lines.iter_mut().enumerate() {
        if let Some(name) = selected_lines.get(&(index + 1)) {
            *line = format!(": # EasyAlias imported alias {name}");
        }
    }

    lines.join("\n")
}

pub(crate) fn mark_import_handled(app: &AppHandle) -> Result<(), String> {
    let path = import_marker_file(app)?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub(crate) fn scan_shell_candidates_in_home(home: &Path) -> Result<Vec<ShellAliasCandidate>, String> {
    let mut candidates = Vec::new();

    for (source_index, path) in shell_config_paths(home).into_iter().enumerate() {
        let content = read_text_or_empty(&path)?;
        let source_file = display_shell_path(&path);
        for mut candidate in find_shell_aliases(&content)? {
            candidate.id = format!("shell-{source_index}-line-{}", candidate.line_number);
            candidate.source_file = source_file.clone();
            candidates.push(candidate);
        }
    }

    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for candidate in &candidates {
        *name_counts.entry(candidate.name.clone()).or_default() += 1;
    }

    Ok(candidates
        .into_iter()
        .filter(|candidate| name_counts.get(&candidate.name) == Some(&1))
        .collect())
}

pub(crate) fn scan_import_candidates(
    app: &AppHandle,
    aliases: &[AliasEntry],
) -> Result<Vec<ShellAliasCandidate>, String> {
    let existing_names: HashSet<&str> = aliases.iter().map(|alias| alias.name.as_str()).collect();
    with_home_access(app, |home| {
        Ok(scan_shell_candidates_in_home(home)?
            .into_iter()
            .filter(|candidate| !existing_names.contains(candidate.name.as_str()))
            .collect())
    })
}

pub(crate) fn prepare_managed_shell_updates(
    home: &Path,
    aliases: &[AliasEntry],
) -> Result<Vec<(PathBuf, String, String)>, String> {
    shell_config_paths(home)
        .into_iter()
        .map(|path| {
            let content = read_text_or_empty(&path)?;
            let next_content = update_managed_block(&content, aliases)?;
            Ok((path, content, next_content))
        })
        .collect()
}

pub(crate) fn write_managed_shell_blocks(
    app: &AppHandle,
    home: &Path,
    aliases: &[AliasEntry],
    create_backups: bool,
) -> Result<Vec<PathBuf>, String> {
    let updates = prepare_managed_shell_updates(home, aliases)?;
    let mut backups = Vec::new();

    if create_backups {
        for (path, content, _) in &updates {
            if path.exists() || !content.is_empty() {
                backups.push(write_backup(app, path, content)?);
            }
        }
    }

    for (path, _, next_content) in updates {
        fs::write(&path, next_content)
            .map_err(|error| format!("{} could not be updated: {error}", path.display()))?;
    }

    Ok(backups)
}

#[tauri::command]
pub(crate) fn scan_shell_import(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let aliases = load_config_aliases(&app)?;
    let import_candidates = scan_import_candidates(&app, &aliases)?;
    app_state(&app, aliases, import_candidates)
}

#[tauri::command]
pub(crate) fn dismiss_shell_import(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    mark_import_handled(&app)?;
    app_state(&app, load_config_aliases(&app)?, Vec::new())
}

#[tauri::command]
pub(crate) fn import_shell_aliases(
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
    let mut backup_paths = Vec::new();

    with_home_access(&app, |home| {
        let candidates = scan_shell_candidates_in_home(home)?;
        let selected: Vec<ShellAliasCandidate> = candidates
            .into_iter()
            .filter(|candidate| selected_id_set.contains(candidate.id.as_str()))
            .collect();

        if selected.len() != selected_id_set.len() {
            return Err(
                "Some aliases changed in the shell files. Reopen Import and try again.".to_string(),
            );
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
                id: format!("imported-{import_id}-{}", candidate.id),
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
        for shell_path in shell_config_paths(home) {
            let content = read_text_or_empty(&shell_path)?;
            let source_file = display_shell_path(&shell_path);
            let selected_lines: HashMap<usize, &str> = selected
                .iter()
                .filter(|candidate| candidate.source_file == source_file)
                .map(|candidate| (candidate.line_number, candidate.name.as_str()))
                .collect();
            let without_imported_lines = replace_imported_alias_lines(&content, &selected_lines);
            let next_content = update_managed_block(&without_imported_lines, &aliases)?;
            updates.push((shell_path, content, next_content));
        }

        for (shell_path, content, _) in &updates {
            backup_paths.push(write_backup(&app, shell_path, content)?);
        }
        for (shell_path, _, next_content) in updates {
            fs::write(&shell_path, next_content).map_err(|error| {
                format!("{} could not be updated: {error}", shell_path.display())
            })?;
        }
        imported_count = selected.len();
        Ok(())
    })?;

    write_config_aliases(&app, &aliases)?;
    mark_import_handled(&app)?;
    Ok(ImportResult {
        state: app_state(&app, aliases, Vec::new())?,
        imported_count,
        backup_file: backup_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    })
}
