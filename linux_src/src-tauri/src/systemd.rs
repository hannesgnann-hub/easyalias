//! systemd user timers for timed automations on Linux.

use crate::*;

pub(crate) fn timed_automation_unit_name(id: &str) -> String {
    format!("easyalias-timed-{}", id)
}

pub(crate) fn timed_automation_service_path(id: &str) -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.service", timed_automation_unit_name(id))))
}

pub(crate) fn timed_automation_timer_path(id: &str) -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.timer", timed_automation_unit_name(id))))
}

pub(crate) fn weekday_to_systemd(day: &str) -> &'static str {
    match day {
        "mon" => "Mon",
        "tue" => "Tue",
        "wed" => "Wed",
        "thu" => "Thu",
        "fri" => "Fri",
        "sat" => "Sat",
        "sun" => "Sun",
        _ => "Mon",
    }
}

pub(crate) fn run_systemctl_user(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("systemctl").arg("--user").args(args).output()
}

// Every day: "*-*-* HH:MM:00". Specific weekdays: "Mon,Fri *-*-* HH:MM:00".
// Pulled out as its own function so the expression can be checked without
// needing a real systemd instance to run against.
pub(crate) fn timed_automation_on_calendar(entry: &TimedAutomation) -> Result<String, String> {
    let (hour, minute) = parse_time_of_day(&entry.time)?;
    if entry.days.is_empty() {
        Ok(format!("*-*-* {:02}:{:02}:00", hour, minute))
    } else {
        let days = entry
            .days
            .iter()
            .map(|day| weekday_to_systemd(day))
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!("{} *-*-* {:02}:{:02}:00", days, hour, minute))
    }
}

// Removes any existing systemd --user timer/service pair for this timed
// automation, regardless of whether one is currently enabled - safe to call
// even if nothing was ever scheduled. Called both on delete and before every
// re-schedule, since an edited unit file needs a daemon-reload to take
// effect.
pub(crate) fn unschedule_timed_automation_linux(id: &str) -> Result<(), String> {
    let unit = format!("{}.timer", timed_automation_unit_name(id));
    let _ = run_systemctl_user(&["disable", "--now", &unit]);

    let service_path = timed_automation_service_path(id)?;
    let timer_path = timed_automation_timer_path(id)?;
    let mut removed_any = false;
    if service_path.exists() {
        fs::remove_file(&service_path).map_err(|error| {
            format!("{} could not be removed: {}", service_path.display(), error)
        })?;
        removed_any = true;
    }
    if timer_path.exists() {
        fs::remove_file(&timer_path).map_err(|error| {
            format!("{} could not be removed: {}", timer_path.display(), error)
        })?;
        removed_any = true;
    }
    if removed_any {
        let _ = run_systemctl_user(&["daemon-reload"]);
    }
    Ok(())
}

// Writes a oneshot systemd --user service (invokes this same executable with
// `--run-timed-automation <id>`) plus a timer unit with the configured
// OnCalendar expression, then enables it. This is what lets a timed
// automation fire even when EasyAlias itself is not open - the systemd user
// instance, not the app, owns the clock. Requires a running systemd --user
// session (the default on virtually all modern desktop distros).
pub(crate) fn schedule_timed_automation_linux(entry: &TimedAutomation) -> Result<(), String> {
    unschedule_timed_automation_linux(&entry.id)?;
    if !entry.enabled {
        return Ok(());
    }

    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;

    let unit_dir = systemd_user_dir()?;
    fs::create_dir_all(&unit_dir)
        .map_err(|error| format!("{} could not be created: {}", unit_dir.display(), error))?;

    let service = format!(
        "[Unit]\nDescription=EasyAlias timed automation {id}\n\n[Service]\nType=oneshot\nExecStart={exe} --run-timed-automation {id}\n",
        id = entry.id,
        exe = exe.display()
    );
    let service_path = timed_automation_service_path(&entry.id)?;
    fs::write(&service_path, service)
        .map_err(|error| format!("{} could not be written: {}", service_path.display(), error))?;

    let on_calendar = timed_automation_on_calendar(entry)?;

    let timer = format!(
        "[Unit]\nDescription=EasyAlias timed automation {id} schedule\n\n[Timer]\nOnCalendar={calendar}\nPersistent=false\n\n[Install]\nWantedBy=timers.target\n",
        id = entry.id,
        calendar = on_calendar
    );
    let timer_path = timed_automation_timer_path(&entry.id)?;
    fs::write(&timer_path, timer)
        .map_err(|error| format!("{} could not be written: {}", timer_path.display(), error))?;

    let reload = run_systemctl_user(&["daemon-reload"])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !reload.status.success() {
        return Err(format!(
            "systemctl daemon-reload failed: {}",
            String::from_utf8_lossy(&reload.stderr).trim()
        ));
    }

    let unit = format!("{}.timer", timed_automation_unit_name(&entry.id));
    let enable = run_systemctl_user(&["enable", "--now", &unit])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !enable.status.success() {
        return Err(format!(
            "systemctl enable failed: {}",
            String::from_utf8_lossy(&enable.stderr).trim()
        ));
    }

    Ok(())
}

pub(crate) fn sun_timed_automations_checker_unit_name() -> &'static str {
    "easyalias-sun-timed-automations"
}

pub(crate) fn sun_timed_automations_checker_service_path() -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.service", sun_timed_automations_checker_unit_name())))
}

pub(crate) fn sun_timed_automations_checker_timer_path() -> Result<PathBuf, String> {
    Ok(systemd_user_dir()?.join(format!("{}.timer", sun_timed_automations_checker_unit_name())))
}

// Removes the shared systemd --user timer/service pair that periodically
// checks sunrise/sunset timed automations, if one is currently enabled -
// safe to call even if nothing was ever scheduled.
pub(crate) fn unschedule_sun_timed_automations_checker_linux() -> Result<(), String> {
    let unit = format!("{}.timer", sun_timed_automations_checker_unit_name());
    let _ = run_systemctl_user(&["disable", "--now", &unit]);

    let service_path = sun_timed_automations_checker_service_path()?;
    let timer_path = sun_timed_automations_checker_timer_path()?;
    let mut removed_any = false;
    if service_path.exists() {
        fs::remove_file(&service_path).map_err(|error| {
            format!("{} could not be removed: {}", service_path.display(), error)
        })?;
        removed_any = true;
    }
    if timer_path.exists() {
        fs::remove_file(&timer_path).map_err(|error| {
            format!("{} could not be removed: {}", timer_path.display(), error)
        })?;
        removed_any = true;
    }
    if removed_any {
        let _ = run_systemctl_user(&["daemon-reload"]);
    }
    Ok(())
}

// Writes a oneshot systemd --user service (invokes this same executable
// with `--check-sun-timed-automations`) plus a timer unit that fires every
// SUN_CHECK_INTERVAL_SECONDS and once shortly after boot (OnBootSec), with
// Persistent=true so a check missed while suspended runs as soon as the
// session is back. One shared job serves every sunrise/sunset timed
// automation, since none of them can be baked into a fixed OnCalendar
// expression the way a clock-time one can.
pub(crate) fn schedule_sun_timed_automations_checker_linux() -> Result<(), String> {
    unschedule_sun_timed_automations_checker_linux()?;

    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;

    let unit_dir = systemd_user_dir()?;
    fs::create_dir_all(&unit_dir)
        .map_err(|error| format!("{} could not be created: {}", unit_dir.display(), error))?;

    let service = format!(
        "[Unit]\nDescription=EasyAlias sunrise/sunset timed automations check\n\n[Service]\nType=oneshot\nExecStart={exe} --check-sun-timed-automations\n",
        exe = exe.display()
    );
    let service_path = sun_timed_automations_checker_service_path()?;
    fs::write(&service_path, service)
        .map_err(|error| format!("{} could not be written: {}", service_path.display(), error))?;

    let timer = format!(
        "[Unit]\nDescription=EasyAlias sunrise/sunset timed automations timer\n\n[Timer]\nOnBootSec=60\nOnUnitActiveSec={interval}\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        interval = SUN_CHECK_INTERVAL_SECONDS
    );
    let timer_path = sun_timed_automations_checker_timer_path()?;
    fs::write(&timer_path, timer)
        .map_err(|error| format!("{} could not be written: {}", timer_path.display(), error))?;

    let reload = run_systemctl_user(&["daemon-reload"])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !reload.status.success() {
        return Err(format!(
            "systemctl daemon-reload failed: {}",
            String::from_utf8_lossy(&reload.stderr).trim()
        ));
    }

    let unit = format!("{}.timer", sun_timed_automations_checker_unit_name());
    let enable = run_systemctl_user(&["enable", "--now", &unit])
        .map_err(|error| format!("systemctl could not be started: {}", error))?;
    if !enable.status.success() {
        return Err(format!(
            "systemctl enable failed: {}",
            String::from_utf8_lossy(&enable.stderr).trim()
        ));
    }

    Ok(())
}

// Keeps the shared checker job's scheduled state in sync with whether any
// enabled sunrise/sunset timed automation currently exists - called after
// every save/delete since any of those can change the answer.
pub(crate) fn sync_sun_timed_automations_checker_linux(entries: &[TimedAutomation]) -> Result<(), String> {
    let needed = entries
        .iter()
        .any(|entry| entry.enabled && (entry.trigger_kind == "sunrise" || entry.trigger_kind == "sunset"));
    if needed {
        schedule_sun_timed_automations_checker_linux()
    } else {
        unschedule_sun_timed_automations_checker_linux()
    }
}
