//! Timed automations: validation, storage, Tauri commands and headless runners.

use crate::*;

pub(crate) fn parse_time_of_day(value: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("\"{}\" is not a valid time (expected HH:MM).", value));
    }
    let hour = parts[0]
        .parse::<u32>()
        .map_err(|_| format!("\"{}\" is not a valid time (expected HH:MM).", value))?;
    let minute = parts[1]
        .parse::<u32>()
        .map_err(|_| format!("\"{}\" is not a valid time (expected HH:MM).", value))?;
    if hour > 23 || minute > 59 {
        return Err(format!("\"{}\" is not a valid time (expected HH:MM).", value));
    }
    Ok((hour, minute))
}

pub(crate) fn validate_timed_automation(entry: &TimedAutomation, automations: &[Automation]) -> Result<(), String> {
    if entry.id.trim().is_empty() {
        return Err("Every timed automation needs an id.".to_string());
    }
    if !automations
        .iter()
        .any(|automation| automation.id == entry.automation_id)
    {
        return Err("Choose an automation to schedule.".to_string());
    }
    match entry.trigger_kind.as_str() {
        "clock" => {
            parse_time_of_day(&entry.time)?;
        }
        "sunrise" | "sunset" => {
            let location = load_sun_location()?;
            if sun_region_coordinates(&location.region).is_none() {
                return Err("Choose a region for sunrise/sunset scheduling.".to_string());
            }
        }
        other => return Err(format!("\"{}\" is not a valid trigger.", other)),
    }
    for day in &entry.days {
        if !WEEKDAYS.contains(&day.as_str()) {
            return Err(format!("\"{}\" is not a valid weekday.", day));
        }
    }
    Ok(())
}

pub(crate) fn load_timed_automation_entries() -> Result<Vec<TimedAutomation>, String> {
    ensure_app_files()?;
    let path = timed_automations_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read: {}", path.display(), error))?;
    let entries: Vec<TimedAutomation> = serde_json::from_str(&content)
        .map_err(|error| format!("timed-automations.json is not valid EasyAlias JSON: {}", error))?;
    Ok(entries)
}

pub(crate) fn write_timed_automation_entries(entries: &[TimedAutomation]) -> Result<(), String> {
    ensure_app_files()?;
    let json = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Timed automations could not be serialized: {}", error))?;
    let path = timed_automations_file()?;
    fs::write(&path, format!("{}\n", json))
        .map_err(|error| format!("{} could not be written: {}", path.display(), error))
}

// Runs every step of `automation` sequentially through one persistent shell
// session, exactly like an interactive run, but with no frontend to report
// progress to - only the final outcome is recorded by the caller.
pub(crate) fn run_automation_steps_headless(automation: &Automation) -> Result<(), String> {
    let working_directory = automation_working_directory(&automation.path)?;
    let mut session = spawn_automation_session(&working_directory)?;

    for (index, step) in automation.steps.iter().enumerate() {
        if step.kind == "wait" {
            thread::sleep(std::time::Duration::from_secs(step.seconds));
            continue;
        }

        match execute_in_session(&mut session, &step.command, step.behavior == "background") {
            Ok(result) if step.behavior == "background" || result.exit_code == Some(0) => {}
            Ok(result) => {
                let _ = session.child.kill();
                return Err(format!(
                    "Step {} failed (exit code {}): {}",
                    index + 1,
                    result
                        .exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    limited_output(result.stdout.as_bytes())
                ));
            }
            Err(error) => {
                let _ = session.child.kill();
                return Err(error);
            }
        }
    }

    let _ = session.child.kill();
    Ok(())
}

// Entry point for `--run-timed-automation <id>`: no Tauri runtime, no
// window, just load the schedule and the automation it points to, run it,
// and record the outcome so the app can show "last run" next time it opens.
pub(crate) fn run_timed_automation_headless(id: &str) -> Result<(), String> {
    let mut entries = load_timed_automation_entries()?;
    let index = entries
        .iter()
        .position(|entry| entry.id == id)
        .ok_or_else(|| "Timed automation no longer exists.".to_string())?;
    let automation_id = entries[index].automation_id.clone();

    let automations = load_automation_entries()?;
    let automation = automations
        .into_iter()
        .find(|automation| automation.id == automation_id)
        .ok_or_else(|| "Automation no longer exists.".to_string())?;

    let run_result = run_automation_steps_headless(&automation);

    entries[index].last_run_at = unix_timestamp().ok();
    match &run_result {
        Ok(()) => {
            entries[index].last_run_status = Some("success".to_string());
            entries[index].last_run_output = None;
        }
        Err(message) => {
            entries[index].last_run_status = Some("error".to_string());
            entries[index].last_run_output = Some(message.chars().take(500).collect());
        }
    }
    write_timed_automation_entries(&entries)?;

    run_result
}

// Entry point for `--check-sun-timed-automations`: no Tauri runtime, no
// window. Runs on the shared periodic checker (unlike clock-time entries,
// which each get their own exact-fire systemd timer). For every enabled
// sunrise/sunset entry that hasn't already fired today and whose target
// time for today has passed, runs it and records the outcome, same as the
// exact-time path.
pub(crate) fn check_sun_timed_automations() -> Result<(), String> {
    let mut entries = load_timed_automation_entries()?;
    let automations = load_automation_entries()?;
    let LocalNow {
        hour,
        minute,
        date: today,
        day_of_year: today_day_of_year,
        weekday: today_weekday,
        utc_offset_minutes,
    } = local_now();
    let now_minutes = hour * 60 + minute;
    let mut changed = false;

    for index in 0..entries.len() {
        let entry = entries[index].clone();
        if !entry.enabled {
            continue;
        }
        if entry.trigger_kind != "sunrise" && entry.trigger_kind != "sunset" {
            continue;
        }
        if entry.last_triggered_date.as_deref() == Some(today.as_str()) {
            continue;
        }
        if !entry.days.is_empty() && !entry.days.iter().any(|day| day == &today_weekday) {
            continue;
        }

        let target = match resolve_trigger_time_today(&entry, today_day_of_year, utc_offset_minutes) {
            Ok(Some(target)) => target,
            Ok(None) => continue, // polar day/night - no event today
            Err(_) => continue,   // don't let one bad entry block the rest of the batch
        };
        if now_minutes < (target.0 * 60 + target.1) {
            continue;
        }

        let automation = match automations.iter().find(|item| item.id == entry.automation_id) {
            Some(automation) => automation,
            None => continue,
        };

        let run_result = run_automation_steps_headless(automation);
        entries[index].last_triggered_date = Some(today.clone());
        entries[index].last_run_at = unix_timestamp().ok();
        match &run_result {
            Ok(()) => {
                entries[index].last_run_status = Some("success".to_string());
                entries[index].last_run_output = None;
            }
            Err(message) => {
                entries[index].last_run_status = Some("error".to_string());
                entries[index].last_run_output = Some(message.chars().take(500).collect());
            }
        }
        changed = true;
    }

    if changed {
        write_timed_automation_entries(&entries)?;
    }

    Ok(())
}

#[tauri::command]
pub(crate) fn list_timed_automations() -> Result<Vec<TimedAutomation>, String> {
    load_timed_automation_entries()
}

// Validates against the current automation list, persists, and re-syncs the
// OS schedule so it always matches what was just saved - editing the
// trigger/time/days/enabled state takes effect immediately, not just after
// the app restarts. Clock-time entries each get their own exact-fire
// systemd timer; sunrise/sunset entries instead ride the shared checker
// timer, (re)synced against the full list since it serves every such entry
// at once.
#[tauri::command]
pub(crate) fn save_timed_automation(entry: TimedAutomation) -> Result<Vec<TimedAutomation>, String> {
    let automations = load_automation_entries()?;
    validate_timed_automation(&entry, &automations)?;

    let mut entries = load_timed_automation_entries()?;
    match entries.iter().position(|existing| existing.id == entry.id) {
        Some(index) => entries[index] = entry.clone(),
        None => entries.push(entry.clone()),
    }
    write_timed_automation_entries(&entries)?;

    if entry.trigger_kind == "clock" {
        schedule_timed_automation_linux(&entry)?;
    } else {
        // Not a clock entry (any more) - remove a stale individual job left
        // over from switching this entry away from a clock trigger.
        unschedule_timed_automation_linux(&entry.id)?;
    }
    sync_sun_timed_automations_checker_linux(&entries)?;

    Ok(entries)
}

#[tauri::command]
pub(crate) fn delete_timed_automation(id: String) -> Result<Vec<TimedAutomation>, String> {
    let mut entries = load_timed_automation_entries()?;
    let original_len = entries.len();
    entries.retain(|entry| entry.id != id);
    if entries.len() == original_len {
        return Err("Timed automation no longer exists.".to_string());
    }
    write_timed_automation_entries(&entries)?;
    unschedule_timed_automation_linux(&id)?;
    sync_sun_timed_automations_checker_linux(&entries)?;
    Ok(entries)
}
