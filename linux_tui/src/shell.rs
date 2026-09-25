//! Shell startup files: connecting EasyAlias, and importing existing aliases.

use crate::*;

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

// Only unindented, one-line aliases with one assignment are movable without
// interpreting arbitrary shell syntax or changing conditional behavior.
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

pub(crate) fn find_shell_aliases(content: &str) -> Vec<ShellAliasCandidate> {
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

pub(crate) fn scan_shell_aliases(setup: &ShellSetup) -> Result<Vec<ShellAliasCandidate>, String> {
    Ok(find_shell_aliases(&read_text_or_empty(&setup.config_file)?))
}

pub(crate) fn mark_import_handled() -> Result<(), String> {
    let path = import_marker_file()?;
    fs::write(&path, IMPORT_MARKER_CONTENT)
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

pub(crate) fn next_shell_backup_file(setup: &ShellSetup) -> Result<PathBuf, String> {
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
pub(crate) fn shell_source_present(setup: &ShellSetup) -> bool {
    fs::read_to_string(&setup.config_file)
        .ok()
        .map(|content| content.lines().any(|line| line.trim() == SOURCE_LINE))
        .unwrap_or(false)
}

// First-run setup creates both the private app directory and an empty alias
// file, so a newly added source line can never point at a missing file.
pub(crate) fn ensure_app_files() -> Result<(), String> {
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
pub(crate) fn ensure_shell_source(setup: &ShellSetup) -> Result<(), String> {
    let content = read_text_or_empty(&setup.config_file)?;

    if content.lines().any(|line| line.trim() == SOURCE_LINE) {
        return Ok(());
    }

    let mut next_content = content;
    if !next_content.is_empty() && !next_content.ends_with('\n') {
        next_content.push('\n');
    }
    next_content.push_str("\n# EasyAlias aliases\n");
    next_content.push_str(SOURCE_LINE);
    next_content.push('\n');

    fs::write(&setup.config_file, next_content)
        .map_err(|error| format!("{} could not be updated: {}", setup.config_file.display(), error))
}

pub(crate) fn replace_imported_alias_lines(content: &str, selected_lines: &HashMap<usize, &str>) -> String {
    let mut lines: Vec<String> = content.split('\n').map(str::to_string).collect();
    for (index, line) in lines.iter_mut().enumerate() {
        if let Some(name) = selected_lines.get(&(index + 1)) {
            *line = format!(": # EasyAlias imported alias {}", name);
        }
    }
    lines.join("\n")
}

// Manually rescan the detected shell startup file when Import is opened from
// the header. This ignores the first-start marker so aliases added later remain
// importable, while already managed names are excluded from the selection.
pub(crate) fn scan_shell_import() -> Result<AppState, String> {
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

pub(crate) fn dismiss_shell_import() -> Result<AppState, String> {
    let setup = shell_setup()?;
    ensure_app_files()?;
    ensure_shell_source(&setup)?;
    mark_import_handled()?;
    app_state(load_config_aliases()?, &setup, Vec::new())
}

pub(crate) fn import_shell_aliases(
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
