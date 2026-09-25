//! TUI settings and the sunrise/sunset location.

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

pub(crate) fn load_tui_settings() -> Result<TuiSettings, String> {
    ensure_app_files()?;
    let path = tui_settings_file()?;
    if !path.exists() {
        return Ok(default_tui_settings());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let settings: TuiSettings = serde_json::from_str(&content)
        .map_err(|error| format!("tui-settings.json is not valid EasyAlias JSON: {}", error))?;
    Ok(settings)
}

pub(crate) fn write_tui_settings(settings: &TuiSettings) -> Result<(), String> {
    validate_tui_settings(settings)?;
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("Settings could not be serialized: {}", error))?;
    let path = tui_settings_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

pub(crate) fn validate_tui_settings(settings: &TuiSettings) -> Result<(), String> {
    if !THEME_VALUES.contains(&settings.theme.as_str()) {
        return Err(format!("\"{}\" is not a valid theme.", settings.theme));
    }
    Ok(())
}

pub(crate) fn save_sun_location(setting: SunLocationSetting) -> Result<SunLocationSetting, String> {
    if sun_region_coordinates(&setting.region).is_none() {
        return Err(format!("\"{}\" is not a known region.", setting.region));
    }
    write_sun_location(&setting)?;
    Ok(setting)
}
