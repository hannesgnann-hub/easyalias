//! App settings, sunrise/sunset location and their Tauri commands.

use crate::*;

pub(crate) fn load_sun_location() -> Result<SunLocationSetting, String> {
    ensure_app_files()?;
    let path = sun_location_file()?;
    if !path.exists() {
        return Ok(default_sun_location());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let setting: SunLocationSetting = serde_json::from_str(&content)
        .map_err(|error| format!("sun-location.json is not valid EasyAlias JSON: {}", error))?;
    Ok(setting)
}

pub(crate) fn write_sun_location(setting: &SunLocationSetting) -> Result<(), String> {
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(setting)
        .map_err(|error| format!("Location could not be serialized: {}", error))?;
    let path = sun_location_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

pub(crate) fn load_app_settings() -> Result<AppSettings, String> {
    ensure_app_files()?;
    let path = settings_file()?;
    if !path.exists() {
        return Ok(default_app_settings());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let settings: AppSettings = serde_json::from_str(&content)
        .map_err(|error| format!("settings.json is not valid EasyAlias JSON: {}", error))?;
    Ok(settings)
}

pub(crate) fn write_app_settings(settings: &AppSettings) -> Result<(), String> {
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("Settings could not be serialized: {}", error))?;
    let path = settings_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

#[tauri::command]
pub(crate) fn list_sun_regions() -> Vec<SunRegionOption> {
    SUN_REGIONS
        .iter()
        .map(|(key, label, _, _)| SunRegionOption {
            value: key.to_string(),
            label: label.to_string(),
        })
        .collect()
}

#[tauri::command]
pub(crate) fn load_sun_location_state() -> Result<SunLocationSetting, String> {
    load_sun_location()
}

#[tauri::command]
pub(crate) fn save_sun_location(setting: SunLocationSetting) -> Result<SunLocationSetting, String> {
    if sun_region_coordinates(&setting.region).is_none() {
        return Err(format!("\"{}\" is not a known region.", setting.region));
    }
    write_sun_location(&setting)?;
    Ok(setting)
}

pub(crate) fn validate_app_settings(settings: &AppSettings) -> Result<(), String> {
    if !THEME_VALUES.contains(&settings.theme.as_str()) {
        return Err(format!("\"{}\" is not a valid theme.", settings.theme));
    }
    if !HOTKEY_BEHAVIOR_VALUES.contains(&settings.hotkey_behavior.as_str()) {
        return Err(format!(
            "\"{}\" is not a valid hotkey behavior.",
            settings.hotkey_behavior
        ));
    }
    Ok(())
}

// Reconcile the "autostart" preference with the plugin's actual OS-level
// registration; the plugin is the source of truth.
pub(crate) fn apply_autostart(app: &tauri::AppHandle, enabled: bool) {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(error) = result {
        eprintln!("Autostart could not be updated: {}", error);
    }
}

#[tauri::command]
pub(crate) fn load_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let mut settings = load_app_settings()?;
    settings.autostart = app.autolaunch().is_enabled().unwrap_or(settings.autostart);
    Ok(settings)
}

#[tauri::command]
pub(crate) fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    validate_app_settings(&settings)?;
    write_app_settings(&settings)?;
    apply_autostart(&app, settings.autostart);
    Ok(settings)
}


// Assigns, replaces, or clears (accelerator = None) an automation's global
// keyboard shortcut. The OS registration is attempted before anything is
// persisted, so a clash with the system or another app leaves the stored
// automation untouched and its previous shortcut still active.
#[tauri::command]
pub(crate) fn set_automation_hotkey(
    app: tauri::AppHandle,
    id: String,
    accelerator: Option<String>,
) -> Result<Vec<Automation>, String> {
    let mut automations = load_automation_entries()?;
    let index = automations
        .iter()
        .position(|automation| automation.id == id)
        .ok_or_else(|| "Automation no longer exists.".to_string())?;

    let previous = automations[index].hotkey.clone();
    let previous_shortcut = previous.as_deref().and_then(parse_shortcut);
    let global_shortcut = app.global_shortcut();

    match accelerator {
        Some(raw) => {
            let acc = raw.trim().to_string();
            let shortcut = parse_shortcut(&acc)
                .ok_or_else(|| format!("\"{}\" is not a valid keyboard shortcut.", acc))?;

            if let Some(other) = automations.iter().find(|automation| {
                automation.id != id
                    && automation
                        .hotkey
                        .as_deref()
                        .and_then(parse_shortcut)
                        .map(|existing| existing == shortcut)
                        .unwrap_or(false)
            }) {
                return Err(format!(
                    "That shortcut is already assigned to \"{}\".",
                    other.name.trim()
                ));
            }

            if let Some(old) = previous_shortcut {
                let _ = global_shortcut.unregister(old);
            }
            if let Err(error) = global_shortcut.register(shortcut) {
                if let Some(old) = previous_shortcut {
                    let _ = global_shortcut.register(old);
                }
                return Err(format!(
                    "That shortcut is already in use by the system or another app ({}).",
                    error
                ));
            }
            automations[index].hotkey = Some(acc);
        }
        None => {
            if let Some(old) = previous_shortcut {
                let _ = global_shortcut.unregister(old);
            }
            automations[index].hotkey = None;
        }
    }

    write_automation_entries(&automations)?;
    Ok(automations)
}
