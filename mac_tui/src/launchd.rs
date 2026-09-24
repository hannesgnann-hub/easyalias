//! macOS launchd scheduling for timed automations.

use crate::*;

// launchd stores an absolute program path. A Homebrew install runs from a
// versioned Cellar directory that disappears on `brew upgrade`, so prefer the
// stable `<prefix>/bin/<name>` symlink whenever it points at this binary.
pub(crate) fn scheduler_executable() -> Result<PathBuf, String> {
    let exe = env::current_exe()
        .map_err(|error| format!("Application path could not be determined: {}", error))?;
    Ok(stable_executable_path(&exe))
}

pub(crate) fn stable_executable_path(exe: &Path) -> PathBuf {
    let text = exe.to_string_lossy();
    if let (Some(index), Some(name)) = (text.find("/Cellar/"), exe.file_name()) {
        let candidate = PathBuf::from(&text[..index]).join("bin").join(name);
        let same = fs::canonicalize(&candidate)
            .ok()
            .zip(fs::canonicalize(exe).ok())
            .map(|(left, right)| left == right)
            .unwrap_or(false);
        if same {
            return candidate;
        }
    }
    exe.to_path_buf()
}

// launchd Weekday integers: 0 (or 7) = Sunday, 1 = Monday, ... 6 = Saturday.
pub(crate) fn weekday_to_launchd(day: &str) -> u32 {
    match day {
        "sun" => 0,
        "mon" => 1,
        "tue" => 2,
        "wed" => 3,
        "thu" => 4,
        "fri" => 5,
        "sat" => 6,
        _ => 0,
    }
}

pub(crate) fn timed_automation_launchd_label(id: &str) -> String {
    format!("dev.hannesgnann.easyalias.timed.{}", id)
}

pub(crate) fn timed_automation_plist_path(id: &str) -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", timed_automation_launchd_label(id))))
}

// Removes any existing launchd job for this timed automation, regardless of
// whether one is currently loaded - safe to call even if nothing was ever
// scheduled. Called both on delete and before every re-schedule, since
// launchd does not pick up an edited plist without an unload/load cycle.
pub(crate) fn unschedule_timed_automation_macos(id: &str) -> Result<(), String> {
    let plist_path = timed_automation_plist_path(id)?;
    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .arg("unload")
            .arg("-w")
            .arg(&plist_path)
            .output();
        fs::remove_file(&plist_path).map_err(|error| {
            format!("{} could not be removed: {}", plist_path.display(), error)
        })?;
    }
    Ok(())
}

// Writes a launchd LaunchAgent that invokes this same executable with
// `--run-timed-automation <id>` at the configured time (and weekdays, if
// any), then loads it. This is what lets a timed automation fire even when
// EasyAlias itself is not open - launchd, not the app, owns the clock.
pub(crate) fn schedule_timed_automation_macos(entry: &TimedAutomation) -> Result<(), String> {
    unschedule_timed_automation_macos(&entry.id)?;
    if !entry.enabled {
        return Ok(());
    }

    let exe = scheduler_executable()?;
    let (hour, minute) = parse_time_of_day(&entry.time)?;

    let log_dir = timed_automation_log_dir()?;
    fs::create_dir_all(&log_dir)
        .map_err(|error| format!("{} could not be created: {}", log_dir.display(), error))?;
    let log_path = log_dir.join(format!("{}.log", entry.id));

    let intervals = if entry.days.is_empty() {
        format!(
            "    <dict>\n      <key>Hour</key><integer>{}</integer>\n      <key>Minute</key><integer>{}</integer>\n    </dict>\n",
            hour, minute
        )
    } else {
        entry
            .days
            .iter()
            .map(|day| {
                format!(
                    "    <dict>\n      <key>Hour</key><integer>{}</integer>\n      <key>Minute</key><integer>{}</integer>\n      <key>Weekday</key><integer>{}</integer>\n    </dict>\n",
                    hour, minute, weekday_to_launchd(day)
                )
            })
            .collect::<Vec<_>>()
            .join("")
    };

    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>{label}</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>{exe}</string>\n\
    <string>--run-timed-automation</string>\n\
    <string>{id}</string>\n\
  </array>\n\
  <key>StartCalendarInterval</key>\n\
  <array>\n\
{intervals}\
  </array>\n\
  <key>StandardOutPath</key>\n\
  <string>{log}</string>\n\
  <key>StandardErrorPath</key>\n\
  <string>{log}</string>\n\
  <key>RunAtLoad</key>\n\
  <false/>\n\
</dict>\n\
</plist>\n",
        label = timed_automation_launchd_label(&entry.id),
        exe = exe.display(),
        id = entry.id,
        intervals = intervals,
        log = log_path.display()
    );

    let plist_path = timed_automation_plist_path(&entry.id)?;
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("{} could not be created: {}", parent.display(), error))?;
    }
    fs::write(&plist_path, plist)
        .map_err(|error| format!("{} could not be written: {}", plist_path.display(), error))?;

    let output = Command::new("launchctl")
        .arg("load")
        .arg("-w")
        .arg(&plist_path)
        .output()
        .map_err(|error| format!("launchctl could not be started: {}", error))?;
    if !output.status.success() {
        return Err(format!(
            "launchctl load failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

pub(crate) fn sun_timed_automations_checker_launchd_label() -> String {
    "dev.hannesgnann.easyalias.sun-timed-automations".to_string()
}

pub(crate) fn sun_timed_automations_checker_plist_path() -> Result<PathBuf, String> {
    Ok(home_dir()?
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", sun_timed_automations_checker_launchd_label())))
}

// Removes the shared launchd job that periodically checks sunrise/sunset
// timed automations, if one is currently loaded - safe to call even if
// nothing was ever scheduled.
pub(crate) fn unschedule_sun_timed_automations_checker_macos() -> Result<(), String> {
    let plist_path = sun_timed_automations_checker_plist_path()?;
    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .arg("unload")
            .arg("-w")
            .arg(&plist_path)
            .output();
        fs::remove_file(&plist_path).map_err(|error| {
            format!("{} could not be removed: {}", plist_path.display(), error)
        })?;
    }
    Ok(())
}

// Writes a launchd LaunchAgent that invokes this same executable with
// `--check-sun-timed-automations` every SUN_CHECK_INTERVAL_SECONDS, and once
// immediately at login (RunAtLoad). One shared job serves every
// sunrise/sunset timed automation, since none of them can be baked into a
// fixed OS calendar trigger the way a clock-time one can.
pub(crate) fn schedule_sun_timed_automations_checker_macos() -> Result<(), String> {
    unschedule_sun_timed_automations_checker_macos()?;

    let exe = scheduler_executable()?;

    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>{label}</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>{exe}</string>\n\
    <string>--check-sun-timed-automations</string>\n\
  </array>\n\
  <key>StartInterval</key>\n\
  <integer>{interval}</integer>\n\
  <key>RunAtLoad</key>\n\
  <true/>\n\
</dict>\n\
</plist>\n",
        label = sun_timed_automations_checker_launchd_label(),
        exe = exe.display(),
        interval = SUN_CHECK_INTERVAL_SECONDS
    );

    let plist_path = sun_timed_automations_checker_plist_path()?;
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("{} could not be created: {}", parent.display(), error))?;
    }
    fs::write(&plist_path, plist)
        .map_err(|error| format!("{} could not be written: {}", plist_path.display(), error))?;

    let output = Command::new("launchctl")
        .arg("load")
        .arg("-w")
        .arg(&plist_path)
        .output()
        .map_err(|error| format!("launchctl could not be started: {}", error))?;
    if !output.status.success() {
        return Err(format!(
            "launchctl load failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(())
}

// Keeps the shared checker job's scheduled state in sync with whether any
// enabled sunrise/sunset timed automation currently exists - called after
// every save/delete since any of those can change the answer.
pub(crate) fn sync_sun_timed_automations_checker_macos(entries: &[TimedAutomation]) -> Result<(), String> {
    let needed = entries
        .iter()
        .any(|entry| entry.enabled && (entry.trigger_kind == "sunrise" || entry.trigger_kind == "sunset"));
    if needed {
        schedule_sun_timed_automations_checker_macos()
    } else {
        unschedule_sun_timed_automations_checker_macos()
    }
}
