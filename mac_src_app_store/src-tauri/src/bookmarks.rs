//! Sandbox access to the home folder through security-scoped bookmarks.

use crate::*;

pub(crate) fn validate_selected_home(path: &Path) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!("{} is not a folder.", path.display()));
    }
    fs::read_dir(path).map_err(|error| format!("{} is not readable: {error}", path.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn foundation_error(error: &NSError) -> String {
    error.localizedDescription().to_string()
}

#[cfg(target_os = "macos")]
pub(crate) fn create_bookmark_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let path = path
        .to_str()
        .ok_or_else(|| "The selected Home folder path is not valid UTF-8.".to_string())?;
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
                "macOS could not create persistent access for the selected Home folder: {}",
                foundation_error(&error)
            )
        })?;
    Ok(data.to_vec())
}

#[cfg(target_os = "macos")]
pub(crate) fn resolve_bookmark(
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
            "The saved Home folder permission could not be restored: {}",
            foundation_error(&error)
        )
    })?;
    let resolved_path = url
        .path()
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| "The saved Home folder bookmark has no file path.".to_string())?;
    Ok((url, resolved_path, is_stale.as_bool()))
}

#[cfg(target_os = "macos")]
pub(crate) fn refresh_bookmark(app: &AppHandle, url: &NSURL) -> Result<(), String> {
    let data = url
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::WithSecurityScope,
            None,
            None,
        )
        .map_err(|error| {
            format!(
                "The Home folder permission could not be refreshed: {}",
                foundation_error(&error)
            )
        })?;
    let path = bookmark_file(app)?;
    fs::write(&path, data.to_vec())
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

#[cfg(target_os = "macos")]
pub(crate) fn with_home_access<T>(
    app: &AppHandle,
    operation: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    let (url, path, is_stale) = resolve_bookmark(app)?;
    let access_started = unsafe { url.startAccessingSecurityScopedResource() };
    if !access_started {
        return Err(
            "macOS did not grant access to the saved Home folder. Choose the folder again."
                .to_string(),
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
pub(crate) fn create_bookmark_bytes(_path: &Path) -> Result<Vec<u8>, String> {
    Err("The App Store edition only supports macOS.".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn with_home_access<T>(
    _app: &AppHandle,
    _operation: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    Err("The App Store edition only supports macOS.".to_string())
}

pub(crate) fn connection_status(app: &AppHandle) -> (bool, Option<String>, bool, Option<String>) {
    if bookmark_file(app).map_or(true, |path| !path.exists()) {
        return (false, None, false, None);
    }

    match with_home_access(app, |home| {
        validate_selected_home(home)?;
        let mut all_blocks_present = true;
        for path in shell_config_paths(home) {
            let content = read_text_or_empty(&path)?;
            all_blocks_present &= managed_block_present(&content);
        }
        Ok((home.display().to_string(), all_blocks_present))
    }) {
        Ok((path, block_present)) => (true, Some(path), block_present, None),
        Err(error) => (false, None, false, Some(error)),
    }
}

// Called immediately after NSOpenPanel returns a user-selected Home folder. The
// security-scoped bookmark is stored before the temporary panel permission ends.
#[tauri::command]
pub(crate) fn connect_home(app: AppHandle, path: String) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let selected_path = PathBuf::from(path);
    validate_selected_home(&selected_path)?;

    let bookmark = create_bookmark_bytes(&selected_path)?;
    let bookmark_path = bookmark_file(&app)?;
    fs::write(&bookmark_path, bookmark)
        .map_err(|error| format!("{} could not be written: {error}", bookmark_path.display()))?;

    let aliases = load_config_aliases(&app)?;
    let update_result = with_home_access(&app, |home| {
        validate_selected_home(home)?;
        write_managed_shell_blocks(&app, home, &aliases, true)?;
        Ok(())
    });

    if let Err(error) = update_result {
        let _ = fs::remove_file(&bookmark_path);
        return Err(error);
    }

    let legacy_path = legacy_bookmark_file(&app)?;
    if legacy_path.exists() {
        fs::remove_file(&legacy_path)
            .map_err(|error| format!("{} could not be removed: {error}", legacy_path.display()))?;
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

// A deliberate disconnect removes the blocks from the three allowlisted shell
// files before dropping the folder bookmark. Structured aliases remain stored.
#[tauri::command]
pub(crate) fn disconnect_home(app: AppHandle) -> Result<AppState, String> {
    ensure_app_files(&app)?;
    let aliases = load_config_aliases(&app)?;
    let path = bookmark_file(&app)?;

    if path.exists() {
        with_home_access(&app, |home| {
            let mut updates = Vec::new();
            for shell_path in shell_config_paths(home) {
                let content = read_text_or_empty(&shell_path)?;
                if managed_block_present(&content) {
                    let next_content = without_managed_block(&content)?;
                    updates.push((shell_path, content, next_content));
                }
            }

            for (shell_path, content, _) in &updates {
                write_backup(&app, shell_path, content)?;
            }
            for (shell_path, _, next_content) in updates {
                fs::write(&shell_path, next_content).map_err(|error| {
                    format!("{} could not be updated: {error}", shell_path.display())
                })?;
            }
            Ok(())
        })?;
        fs::remove_file(&path)
            .map_err(|error| format!("{} could not be removed: {error}", path.display()))?;
    }

    let legacy_path = legacy_bookmark_file(&app)?;
    if legacy_path.exists() {
        fs::remove_file(&legacy_path)
            .map_err(|error| format!("{} could not be removed: {error}", legacy_path.display()))?;
    }

    app_state(&app, aliases, Vec::new())
}
