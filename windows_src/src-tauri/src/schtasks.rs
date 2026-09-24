//! Windows Task Scheduler (schtasks) jobs for timed automations.

use crate::*;

// Windows Task Scheduler task name for this timed automation. Task names may
// not contain backslashes; the id is a UUID so no further sanitizing is
// needed.
pub(crate) fn timed_automation_task_name(id: &str) -> String {
    format!("EasyAliasTimedAutomation_{}", id)
}

pub(crate) fn weekday_to_schtasks(day: &str) -> &'static str {
    match day {
        "mon" => "MON",
        "tue" => "TUE",
        "wed" => "WED",
        "thu" => "THU",
        "fri" => "FRI",
        "sat" => "SAT",
        "sun" => "SUN",
        _ => "MON",
    }
}

// Builds the `/SC` recurrence type and, for specific weekdays, the `/D`
// day list `schtasks /Create` needs. Extracted as a pure function so it can
// be unit tested without invoking schtasks.exe, which only exists on
// Windows and is unavailable on this development machine.
pub(crate) fn timed_automation_schedule_args(entry: &TimedAutomation) -> (String, Option<String>) {
    if entry.days.is_empty() {
        ("DAILY".to_string(), None)
    } else {
        let days = entry
            .days
            .iter()
            .map(|day| weekday_to_schtasks(day))
            .collect::<Vec<_>>()
            .join(",");
        ("WEEKLY".to_string(), Some(days))
    }
}

pub(crate) fn run_schtasks(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("schtasks")
        .args(args)
        .output()
        .map_err(|error| format!("schtasks could not be started: {}", error))
}

// Removes any existing Scheduled Task for this timed automation, regardless
// of whether one currently exists - safe to call even if nothing was ever
// scheduled. Called both on delete and before every re-schedule, since a
// changed time/day/enabled state needs the task recreated, not edited.
pub(crate) fn unschedule_timed_automation_windows(id: &str) -> Result<(), String> {
    let task_name = timed_automation_task_name(id);
    // Ignore the result: /Delete fails harmlessly when the task does not exist.
    let _ = run_schtasks(&["/Delete", "/TN", &task_name, "/F"]);
    Ok(())
}

// Registers a Scheduled Task that invokes this same executable with
// `--run-timed-automation <id>` at the configured time (and weekdays, if
// any). This is what lets a timed automation fire even when EasyAlias
// itself is not open - Task Scheduler, not the app, owns the clock.
pub(crate) fn schedule_timed_automation_windows(entry: &TimedAutomation) -> Result<(), String> {
    unschedule_timed_automation_windows(&entry.id)?;
    if !entry.enabled {
        return Ok(());
    }

    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;
    let (hour, minute) = parse_time_of_day(&entry.time)?;
    let start_time = format!("{:02}:{:02}", hour, minute);
    let task_name = timed_automation_task_name(&entry.id);
    let (schedule_type, days) = timed_automation_schedule_args(entry);
    let task_run = format!(
        "\"{}\" --run-timed-automation {}",
        exe.display(),
        entry.id
    );

    let mut args: Vec<String> = vec![
        "/Create".to_string(),
        "/TN".to_string(),
        task_name,
        "/TR".to_string(),
        task_run,
        "/SC".to_string(),
        schedule_type,
        "/ST".to_string(),
        start_time,
        "/F".to_string(),
    ];
    if let Some(days_value) = days {
        args.push("/D".to_string());
        args.push(days_value);
    }

    let args_ref: Vec<&str> = args.iter().map(|value| value.as_str()).collect();
    let output = run_schtasks(&args_ref)?;
    if !output.status.success() {
        return Err(format!(
            "schtasks /Create failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

pub(crate) fn sun_timed_automations_checker_task_name() -> &'static str {
    "EasyAliasSunTimedAutomations"
}

// A second, one-off ONSTART task alongside the periodic MINUTE task, so the
// check also runs right after boot/login rather than only at the next
// periodic tick.
pub(crate) fn sun_timed_automations_checker_startup_task_name() -> String {
    format!("{}_Startup", sun_timed_automations_checker_task_name())
}

// Removes both Scheduled Tasks for the shared sunrise/sunset checker, if
// they exist - safe to call even if nothing was ever scheduled.
pub(crate) fn unschedule_sun_timed_automations_checker_windows() -> Result<(), String> {
    let _ = run_schtasks(&["/Delete", "/TN", sun_timed_automations_checker_task_name(), "/F"]);
    let startup_task_name = sun_timed_automations_checker_startup_task_name();
    let _ = run_schtasks(&["/Delete", "/TN", &startup_task_name, "/F"]);
    Ok(())
}

// Registers two Scheduled Tasks that invoke this same executable with
// `--check-sun-timed-automations`: one repeating every
// SUN_CHECK_INTERVAL_MINUTES minutes, and one that fires once at system
// startup. One shared pair of tasks serves every sunrise/sunset timed
// automation, since none of them can be baked into a fixed daily trigger
// the way a clock-time one can.
pub(crate) fn schedule_sun_timed_automations_checker_windows() -> Result<(), String> {
    unschedule_sun_timed_automations_checker_windows()?;

    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;
    let task_run = format!("\"{}\" --check-sun-timed-automations", exe.display());
    let interval = SUN_CHECK_INTERVAL_MINUTES.to_string();

    let periodic_output = run_schtasks(&[
        "/Create",
        "/TN",
        sun_timed_automations_checker_task_name(),
        "/TR",
        &task_run,
        "/SC",
        "MINUTE",
        "/MO",
        &interval,
        "/F",
    ])?;
    if !periodic_output.status.success() {
        return Err(format!(
            "schtasks /Create failed: {}",
            String::from_utf8_lossy(&periodic_output.stderr).trim()
        ));
    }

    let startup_task_name = sun_timed_automations_checker_startup_task_name();
    let startup_output = run_schtasks(&[
        "/Create",
        "/TN",
        &startup_task_name,
        "/TR",
        &task_run,
        "/SC",
        "ONSTART",
        "/F",
    ])?;
    if !startup_output.status.success() {
        return Err(format!(
            "schtasks /Create failed: {}",
            String::from_utf8_lossy(&startup_output.stderr).trim()
        ));
    }

    Ok(())
}

// Keeps the shared checker tasks' scheduled state in sync with whether any
// enabled sunrise/sunset timed automation currently exists - called after
// every save/delete since any of those can change the answer.
pub(crate) fn sync_sun_timed_automations_checker_windows(entries: &[TimedAutomation]) -> Result<(), String> {
    let needed = entries
        .iter()
        .any(|entry| entry.enabled && (entry.trigger_kind == "sunrise" || entry.trigger_kind == "sunset"));
    if needed {
        schedule_sun_timed_automations_checker_windows()
    } else {
        unschedule_sun_timed_automations_checker_windows()
    }
}
