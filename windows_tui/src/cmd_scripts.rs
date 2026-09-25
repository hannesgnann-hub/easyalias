//! Generated .cmd scripts that make aliases available in every terminal.

use crate::*;

pub(crate) fn command_dir() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("bin"))
}

pub(crate) fn command_file(name: &str) -> Result<PathBuf, String> {
    Ok(command_dir()?.join(format!("{}.cmd", name)))
}

// Escaping mirrors the frontend so both preview and generated files agree.
// Percent signs need special care because `%NAME%` expands env vars in .cmd.
pub(crate) fn escape_cmd_double_quoted(value: &str) -> String {
    value.replace('%', "%%").replace('"', "\"\"")
}

// Convert app paths to cmd.exe arguments. This intentionally uses
// %USERPROFILE% instead of PowerShell's $HOME because the generated files run
// under cmd.exe.
pub(crate) fn cmd_path(path: &str) -> String {
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
pub(crate) fn build_command_preview(alias: &AliasEntry) -> String {
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

pub(crate) fn normalize_aliases(aliases: Vec<AliasEntry>) -> Vec<AliasEntry> {
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
pub(crate) fn render_cmd_script(alias: &AliasEntry) -> Result<String, String> {
    validate_alias_entry(alias)?;

    Ok(format!("@echo off\r\n{}\r\n", alias.command_preview))
}

// Regenerate the command directory from the structured config:
// - remove stale .cmd files for aliases that were deleted or renamed
// - keep an existing easya.cmd (the desktop app's launcher); the TUI never
//   creates one itself
// - write one fresh .cmd file per alias
pub(crate) fn write_command_scripts(aliases: &[AliasEntry]) -> Result<(), String> {
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

    Ok(())
}

pub(crate) fn write_alias_data(aliases: &[AliasEntry]) -> Result<(), String> {
    let config = serde_json::to_string_pretty(aliases)
        .map_err(|error| format!("Aliases could not be serialized: {}", error))?;
    write_command_scripts(aliases)?;
    let config_path = config_file()?;
    fs::write(&config_path, format!("{}\n", config))
        .map_err(|error| format!("{} could not be written: {}", config_path.display(), error))
}
