//! Reading and updating the user's PATH so the command directory is found.

use crate::*;

pub(crate) fn normalize_path(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

// PATH is a semicolon-separated list on Windows. Comparing paths as plain
// strings is enough here after trimming quotes, trailing slashes, and case.
pub(crate) fn path_contains_command_dir(path_value: &str) -> Result<bool, String> {
    let bin = command_dir()?;
    let needle = normalize_path(&bin.display().to_string());

    Ok(path_value
        .split(';')
        .any(|entry| normalize_path(entry) == needle))
}

// `reg query HKCU\Environment /v Path` returns localized console text around
// the actual value. The stable part is the line that starts with Path and then
// includes a registry type such as REG_SZ or REG_EXPAND_SZ.
pub(crate) fn parse_registry_path(stdout: &str) -> String {
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
pub(crate) fn user_path_value() -> String {
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

// Expand values such as %USERPROFILE% in persisted PATH entries. Unknown
// variables remain untouched and therefore naturally fail the directory check.
pub(crate) fn expand_percent_variables(value: &str) -> String {
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

pub(crate) fn path_is_within(path: &Path, parent: &Path) -> bool {
    let path = normalize_path(&path.display().to_string());
    let parent = normalize_path(&parent.display().to_string());
    path == parent
        || path
            .strip_prefix(&parent)
            .is_some_and(|suffix| suffix.starts_with('\\') || suffix.starts_with('/'))
}

// Status for the UI. We accept either persisted User PATH or current process
// PATH because the app may be launched after PATH is already refreshed.
pub(crate) fn path_configured() -> bool {
    path_contains_command_dir(&user_path_value()).unwrap_or(false)
        || env::var("PATH")
            .ok()
            .and_then(|path| path_contains_command_dir(&path).ok())
            .unwrap_or(false)
}

pub(crate) fn persist_user_path(next_path: &str) -> Result<(), String> {
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
pub(crate) fn ensure_path_contains_command_dir() -> Result<(), String> {
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
