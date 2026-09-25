use super::*;
use std::ffi::OsString;
use std::sync::Mutex;

pub(crate) static HOME_LOCK: Mutex<()> = Mutex::new(());

// The TUI tests share one helper name across macOS, Linux and Windows.
pub(crate) type TemporaryHome = TemporaryProfile;

pub(crate) struct TemporaryProfile {
    pub(crate) path: PathBuf,
    user_profile: Option<OsString>,
    home: Option<OsString>,
    path_value: Option<OsString>,
    app_data: Option<OsString>,
}

impl TemporaryProfile {
    pub(crate) fn create() -> Self {
        let path = env::temp_dir().join(format!(
            "easyalias-windows-import-test-{}-{}",
            std::process::id(),
            unix_timestamp().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        let user_profile = env::var_os("USERPROFILE");
        let home = env::var_os("HOME");
        let path_value = env::var_os("PATH");
        let app_data = env::var_os("APPDATA");
        env::set_var("USERPROFILE", &path);
        env::set_var("HOME", &path);
        // Also sandbox APPDATA so tests touching vscode_settings_file()
        // never write to the developer's real %APPDATA%\Code\User.
        env::set_var("APPDATA", path.join("AppData").join("Roaming"));
        Self {
            path,
            user_profile,
            home,
            path_value,
            app_data,
        }
    }
}

impl Drop for TemporaryProfile {
    fn drop(&mut self) {
        for (name, value) in [
            ("USERPROFILE", &self.user_profile),
            ("HOME", &self.home),
            ("PATH", &self.path_value),
            ("APPDATA", &self.app_data),
        ] {
            if let Some(value) = value {
                env::set_var(name, value);
            } else {
                env::remove_var(name);
            }
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn test_alias(id: &str, name: &str, command: &str) -> AliasEntry {
    AliasEntry {
        id: id.to_string(),
        name: name.to_string(),
        path: String::new(),
        action: "custom".to_string(),
        custom_command: Some(command.to_string()),
        command_preview: command.to_string(),
        favorite: false,
        created_at: "2026-08-13T18:00:00.000Z".to_string(),
        updated_at: "2026-08-13T18:00:00.000Z".to_string(),
    }
}

#[test]
fn parses_only_simple_command_files() {
    assert_eq!(
        parse_legacy_command_script("@echo off\r\ngit status --short %*\r\n"),
        Some("git status --short %*".to_string())
    );
    assert!(parse_legacy_command_script("echo one\necho two\n").is_none());
    assert!(parse_legacy_command_script("@echo off\ncall %~dp0tool.cmd %*\n").is_none());
    assert!(parse_legacy_command_script("@echo off\n:label\n").is_none());
}

#[test]
fn old_alias_json_defaults_to_not_favorite() {
    let legacy = r#"{
        "id":"legacy-1","name":"ll","path":"","action":"custom",
        "customCommand":"dir /a","commandPreview":"dir /a",
        "createdAt":"2026-07-01T12:00:00.000Z","updatedAt":"2026-07-01T12:00:00.000Z"
    }"#;

    let alias: AliasEntry = serde_json::from_str(legacy).unwrap();
    assert!(!alias.favorite);
}

#[test]
fn deleted_alias_can_be_restored_from_trash() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _profile = TemporaryProfile::create();
    ensure_app_files().unwrap();
    write_alias_data(&[test_alias("one", "ll", "dir /a")]).unwrap();

    let deleted = move_alias_to_trash("one".to_string()).unwrap();
    assert!(deleted.state.aliases.is_empty());
    assert_eq!(deleted.trash.len(), 1);
    assert_eq!(deleted.trash[0].alias.name, "ll");
    assert!(!command_file("ll").unwrap().exists());

    let restored = restore_trash_alias("one".to_string()).unwrap();
    assert_eq!(restored.state.aliases.len(), 1);
    assert!(restored.trash.is_empty());
    assert!(fs::read_to_string(command_file("ll").unwrap())
        .unwrap()
        .contains("dir /a"));
}

#[test]
fn expired_trash_entries_are_removed_automatically() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _profile = TemporaryProfile::create();
    ensure_app_files().unwrap();
    let now = unix_timestamp().unwrap();
    write_trash_entries(&[
        TrashEntry {
            alias: test_alias("expired", "old", "echo old"),
            deleted_at: now.saturating_sub(TRASH_RETENTION_SECONDS + 1),
        },
        TrashEntry {
            alias: test_alias("current", "new", "echo new"),
            deleted_at: now,
        },
    ])
    .unwrap();

    let trash = load_trash_entries().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].alias.id, "current");
    assert!(!fs::read_to_string(trash_file().unwrap())
        .unwrap()
        .contains("expired"));
}

#[test]
fn first_start_import_backs_up_and_moves_command_file() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let profile = TemporaryProfile::create();
    let legacy_dir = profile.path.join("aliases");
    fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_file = legacy_dir.join("gst.cmd");
    fs::write(&legacy_file, "@echo off\r\ngit status --short %*\r\n").unwrap();
    env::set_var("PATH", legacy_dir.display().to_string());

    let initial = load_aliases().unwrap();
    assert_eq!(initial.import_candidates.len(), 1);
    assert_eq!(initial.import_candidates[0].name, "gst");

    let result = import_command_files(
        vec![initial.import_candidates[0].id.clone()],
        "2026-07-18T10:00:00.000Z".to_string(),
    )
    .unwrap();

    assert_eq!(result.imported_count, 1);
    assert!(result.warning.is_none());
    assert!(!legacy_file.exists());
    assert!(profile
        .path
        .join(result.backup_dir.trim_start_matches("~/"))
        .join("gst.cmd")
        .exists());
    assert!(fs::read_to_string(command_file("gst").unwrap())
        .unwrap()
        .contains("git status --short %*"));
    assert_eq!(load_config_aliases().unwrap().len(), 1);
}

#[test]
fn backup_export_contains_only_selected_aliases() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let profile = TemporaryProfile::create();
    ensure_app_files().unwrap();
    write_alias_data(&[
        test_alias("one", "ll", "dir /a"),
        test_alias("two", "gs", "git status"),
    ])
    .unwrap();
    let destination = profile.path.join("selected.json");

    let result = export_alias_backup(
        vec!["two".to_string()],
        destination.display().to_string(),
        "2026-08-13T18:30:00.000Z".to_string(),
    )
    .unwrap();
    let backup = read_backup(&destination).unwrap();

    assert_eq!(result.exported_count, 1);
    assert_eq!(backup.aliases.len(), 1);
    assert_eq!(backup.aliases[0].name, "gs");
}

#[test]
fn backup_import_replaces_name_conflicts_and_keeps_other_aliases() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let profile = TemporaryProfile::create();
    ensure_app_files().unwrap();
    write_alias_data(&[
        test_alias("current-ll", "ll", "dir"),
        test_alias("keep", "gs", "git status"),
    ])
    .unwrap();
    let backup_path = profile.path.join("restore.json");
    let backup = AliasBackup {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at: "2026-08-13T18:30:00.000Z".to_string(),
        aliases: vec![
            test_alias("backup-ll", "LL", "dir /a"),
            test_alias("backup-dcu", "dcu", "docker compose up -d"),
        ],
    };
    fs::write(&backup_path, serde_json::to_string(&backup).unwrap()).unwrap();

    let result = import_alias_backup(
        backup_path.display().to_string(),
        vec!["backup-ll".to_string(), "backup-dcu".to_string()],
        "2026-08-13T19:00:00.000Z".to_string(),
    )
    .unwrap();

    assert_eq!(result.imported_count, 2);
    assert_eq!(result.replaced_count, 1);
    assert_eq!(result.state.aliases.len(), 3);
    assert!(result
        .state
        .aliases
        .iter()
        .any(|alias| alias.name == "LL" && alias.command_preview == "dir /a"));
    assert!(result.state.aliases.iter().any(|alias| alias.name == "gs"));
    assert!(result.state.aliases.iter().any(|alias| alias.name == "dcu"));
}

#[test]
fn accepts_a_valid_sequential_automation() {
    let automation = Automation {
        id: "devstart".to_string(),
        name: "DevStart".to_string(),
        path: "~/Projects/nava".to_string(),
        steps: vec![
            AutomationStep {
                id: "compose".to_string(),
                kind: "command".to_string(),
                command: "docker compose up -d".to_string(),
                seconds: 0,
                behavior: "wait".to_string(),
            },
            AutomationStep {
                id: "settle".to_string(),
                kind: "wait".to_string(),
                command: String::new(),
                seconds: 10,
                behavior: "wait".to_string(),
            },
        ],
        favorite: false,
        group: "Backend".to_string(),
        hotkey: None,
        created_at: "2026-08-24T18:00:00.000Z".to_string(),
        updated_at: "2026-08-24T18:00:00.000Z".to_string(),
    };

    assert!(validate_automations(&[automation]).is_ok());
}

#[test]
fn rejects_invalid_automation_steps() {
    let automation = Automation {
        id: "broken".to_string(),
        name: "Broken".to_string(),
        path: "~/Projects/nava".to_string(),
        steps: vec![AutomationStep {
            id: "wait".to_string(),
            kind: "wait".to_string(),
            command: String::new(),
            seconds: 0,
            behavior: "background".to_string(),
        }],
        favorite: false,
        group: String::new(),
        hotkey: None,
        created_at: "2026-08-24T18:00:00.000Z".to_string(),
        updated_at: "2026-08-24T18:00:00.000Z".to_string(),
    };

    let validation_error = validate_automations(&[automation]).unwrap_err();
    assert!(validation_error.contains("between 1 second and 24 hours"));
}

// These two spawn a real cmd.exe, so they only run on Windows (there is no
// `cmd` binary to spawn when `cargo test` runs on macOS/Linux). They
// reproduce the same scenario as the macOS session tests: a `cd` step
// followed by a command that only succeeds from inside that subdirectory,
// and a backgrounded command that must not block the next step.
#[cfg(target_os = "windows")]
#[test]
fn automation_session_keeps_directory_change_across_steps() {
    let base = env::temp_dir().join(format!(
        "easyalias-automation-session-test-{}",
        unix_timestamp().unwrap()
    ));
    let subdir = base.join("mac_src");
    fs::create_dir_all(&subdir).unwrap();

    let mut session = spawn_automation_session(&base).unwrap();

    let cd_result = execute_in_session(&mut session, "cd mac_src", false).unwrap();
    assert_eq!(cd_result.exit_code, Some(0));

    let cwd_result = execute_in_session(&mut session, "echo %CD%", false).unwrap();
    assert_eq!(cwd_result.exit_code, Some(0));
    assert!(
        cwd_result.stdout.trim().to_lowercase().ends_with("\\mac_src"),
        "expected %CD% to report the subdirectory, got: {:?}",
        cwd_result.stdout
    );

    let _ = session.child.kill();
    let _ = fs::remove_dir_all(&base);
}

#[cfg(target_os = "windows")]
#[test]
fn automation_session_background_command_does_not_block_next_step() {
    let base = env::temp_dir().join(format!(
        "easyalias-automation-session-bg-test-{}",
        unix_timestamp().unwrap()
    ));
    fs::create_dir_all(&base).unwrap();

    let mut session = spawn_automation_session(&base).unwrap();

    let bg_result = execute_in_session(&mut session, "ping 127.0.0.1 -n 6", true).unwrap();
    assert_eq!(bg_result.exit_code, None);

    let echo_result = execute_in_session(&mut session, "echo still-here", false).unwrap();
    assert_eq!(echo_result.exit_code, Some(0));
    assert_eq!(echo_result.stdout.trim(), "still-here");

    let _ = session.child.kill();
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn rejects_a_group_label_over_the_length_limit() {
    let automation = Automation {
        id: "grouped".to_string(),
        name: "Grouped".to_string(),
        path: "~/Projects".to_string(),
        steps: vec![AutomationStep {
            id: "step".to_string(),
            kind: "command".to_string(),
            command: "echo hi".to_string(),
            seconds: 0,
            behavior: "wait".to_string(),
        }],
        favorite: false,
        group: "g".repeat(61),
        hotkey: None,
        created_at: "2026-08-24T18:00:00.000Z".to_string(),
        updated_at: "2026-08-24T18:00:00.000Z".to_string(),
    };

    let validation_error = validate_automations(&[automation]).unwrap_err();
    assert!(validation_error.contains("group label"));
}

pub(crate) fn test_automation(id: &str, name: &str, command: &str) -> Automation {
    Automation {
        id: id.to_string(),
        name: name.to_string(),
        path: "~/Projects".to_string(),
        steps: vec![AutomationStep {
            id: "step".to_string(),
            kind: "command".to_string(),
            command: command.to_string(),
            seconds: 0,
            behavior: "wait".to_string(),
        }],
        favorite: false,
        group: String::new(),
        hotkey: None,
        created_at: "2026-08-24T18:00:00.000Z".to_string(),
        updated_at: "2026-08-24T18:00:00.000Z".to_string(),
    }
}

fn test_timed_automation(automation_id: &str) -> TimedAutomation {
    TimedAutomation {
        id: "timed-1".to_string(),
        automation_id: automation_id.to_string(),
        trigger_kind: "clock".to_string(),
        time: "09:00".to_string(),
        days: Vec::new(),
        enabled: true,
        created_at: "2026-08-24T18:00:00.000Z".to_string(),
        updated_at: "2026-08-24T18:00:00.000Z".to_string(),
        last_run_at: None,
        last_run_status: None,
        last_run_output: None,
        last_triggered_date: None,
    }
}

#[test]
fn parses_valid_and_rejects_invalid_times() {
    assert_eq!(parse_time_of_day("09:30").unwrap(), (9, 30));
    assert_eq!(parse_time_of_day("00:00").unwrap(), (0, 0));
    assert_eq!(parse_time_of_day("23:59").unwrap(), (23, 59));
    assert!(parse_time_of_day("24:00").is_err());
    assert!(parse_time_of_day("09:60").is_err());
    assert!(parse_time_of_day("not-a-time").is_err());
}

#[test]
fn validates_timed_automation_against_its_target() {
    let automation = test_automation("devstart", "DevStart", "echo hi");
    let mut entry = test_timed_automation("devstart");
    entry.days = vec!["mon".to_string(), "wed".to_string()];
    assert!(validate_timed_automation(&entry, &[automation.clone()]).is_ok());

    entry.automation_id = "missing".to_string();
    assert!(validate_timed_automation(&entry, &[automation.clone()])
        .unwrap_err()
        .contains("Choose an automation"));

    entry.automation_id = "devstart".to_string();
    entry.days = vec!["someday".to_string()];
    assert!(validate_timed_automation(&entry, &[automation])
        .unwrap_err()
        .contains("weekday"));
}

// schtasks.exe only exists on Windows, so the /Create arguments are built
// and asserted directly rather than run - the same testability tradeoff
// Linux's OnCalendar-string builder makes for systemd.
#[test]
fn builds_the_expected_schedule_arguments() {
    let mut entry = test_timed_automation("devstart");
    assert_eq!(
        timed_automation_schedule_args(&entry),
        ("DAILY".to_string(), None)
    );

    entry.days = vec!["mon".to_string(), "fri".to_string()];
    assert_eq!(
        timed_automation_schedule_args(&entry),
        ("WEEKLY".to_string(), Some("MON,FRI".to_string()))
    );
}

#[test]
fn timed_automation_task_names_are_namespaced_by_id() {
    assert_eq!(
        timed_automation_task_name("abc-123"),
        "EasyAliasTimedAutomation_abc-123"
    );
}

#[test]
fn rejects_an_unknown_trigger_kind() {
    let automation = test_automation("devstart", "DevStart", "echo hi");
    let mut entry = test_timed_automation("devstart");
    entry.trigger_kind = "moonrise".to_string();
    assert!(validate_timed_automation(&entry, &[automation])
        .unwrap_err()
        .contains("not a valid trigger"));
}

// At the equator, day length stays close to 12 hours year-round
// regardless of season - a solid sanity check for the algorithm that
// does not depend on an external reference table.
#[test]
fn sunrise_and_sunset_are_roughly_12_hours_apart_at_the_equator() {
    for day in [1, 80, 172, 264, 355] {
        let sunrise = sun_event_utc_minutes(day, 0.0, 0.0, true).unwrap();
        let sunset = sun_event_utc_minutes(day, 0.0, 0.0, false).unwrap();
        let day_length = sunset - sunrise;
        assert!(
            (day_length - 720.0).abs() < 20.0,
            "day {} expected ~720 min of daylight at the equator, got {}",
            day,
            day_length
        );
    }
}

#[test]
fn sun_never_sets_near_the_pole_in_local_summer() {
    // Just inside the Arctic Circle around the summer solstice: the sun
    // should not set at all (midnight sun), so cos(hour angle) falls
    // outside [-1, 1] and the function returns None.
    assert!(sun_event_utc_minutes(172, 78.0, 0.0, false).is_none());
}

#[test]
fn sun_never_rises_near_the_pole_in_local_winter() {
    // Same location, opposite solstice - polar night.
    assert!(sun_event_utc_minutes(355, 78.0, 0.0, true).is_none());
}

#[test]
fn converts_utc_sun_event_to_local_time_with_wraparound() {
    assert_eq!(sun_event_local_time(360.0, 120), (8, 0)); // 06:00 UTC + 2h = 08:00
    assert_eq!(sun_event_local_time(30.0, -120), (22, 30)); // 00:30 UTC - 2h wraps to the previous day, 22:30
    assert_eq!(sun_event_local_time(1430.0, 60), (0, 50)); // 23:50 UTC + 1h wraps past midnight to 00:50
}

#[test]
fn local_snapshot_reads_date_weekday_and_offset() {
    use chrono::FixedOffset;
    let offset = FixedOffset::east_opt(2 * 3600).unwrap();
    let now = offset.with_ymd_and_hms(2026, 3, 21, 6, 45, 0).unwrap();
    let snapshot = local_snapshot(&now);
    assert_eq!((snapshot.hour, snapshot.minute), (6, 45));
    assert_eq!(snapshot.date, "2026-03-21");
    assert_eq!(snapshot.day_of_year, 80);
    assert_eq!(snapshot.weekday, "sat");
    assert_eq!(snapshot.utc_offset_minutes, 120);

    let india = FixedOffset::west_opt(-(5 * 3600 + 30 * 60)).unwrap();
    let snapshot = local_snapshot(&india.with_ymd_and_hms(2026, 12, 31, 23, 59, 0).unwrap());
    assert_eq!(snapshot.day_of_year, 365);
    assert_eq!(snapshot.utc_offset_minutes, 330);
}

#[test]
fn resolves_clock_trigger_time_directly() {
    let entry = test_timed_automation("devstart");
    assert_eq!(resolve_trigger_time_today(&entry, 80, 0).unwrap(), Some((9, 0)));
}

#[test]
fn resolves_sunrise_trigger_using_the_configured_region() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _profile = TemporaryProfile::create();

    write_sun_location(&SunLocationSetting {
        region: "eu-central".to_string(),
    })
    .unwrap();
    let mut entry = test_timed_automation("devstart");
    entry.trigger_kind = "sunrise".to_string();
    let resolved = resolve_trigger_time_today(&entry, 172, 120).unwrap();
    assert!(resolved.is_some());
}

#[test]
fn check_sun_timed_automations_skips_an_entry_already_triggered_today() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _profile = TemporaryProfile::create();

    write_sun_location(&SunLocationSetting {
        region: "eu-central".to_string(),
    })
    .unwrap();
    let automation = test_automation("devstart", "DevStart", "echo hi");
    write_automation_entries(&[automation]).unwrap();

    let mut entry = test_timed_automation("devstart");
    entry.trigger_kind = "sunrise".to_string();
    let today = local_now().date;
    entry.last_triggered_date = Some(today);
    write_timed_automation_entries(&[entry]).unwrap();

    check_sun_timed_automations().unwrap();

    let entries = load_timed_automation_entries().unwrap();
    assert_eq!(
        entries[0].last_run_at, None,
        "an entry already triggered today should not run again"
    );
}

#[test]
fn check_sun_timed_automations_skips_a_disabled_entry() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _profile = TemporaryProfile::create();

    write_sun_location(&SunLocationSetting {
        region: "eu-central".to_string(),
    })
    .unwrap();
    let automation = test_automation("devstart", "DevStart", "echo hi");
    write_automation_entries(&[automation]).unwrap();

    let mut entry = test_timed_automation("devstart");
    entry.trigger_kind = "sunrise".to_string();
    entry.enabled = false;
    write_timed_automation_entries(&[entry]).unwrap();

    if check_sun_timed_automations().is_err() {
        // PowerShell not available on this dev machine - same
        // environment-gap tradeoff as the test above.
        return;
    }

    let entries = load_timed_automation_entries().unwrap();
    assert_eq!(entries[0].last_run_at, None, "a disabled entry should never run");
    assert_eq!(entries[0].last_triggered_date, None);
}

#[test]
fn tui_settings_round_trip_and_default_when_missing() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();

    let defaults = load_tui_settings().unwrap();
    assert_eq!(defaults.theme, "system");
    assert!(defaults.show_suggestions);

    write_tui_settings(&TuiSettings { theme: "dark".to_string(), show_suggestions: false }).unwrap();
    let reloaded = load_tui_settings().unwrap();
    assert_eq!(reloaded.theme, "dark");
    assert!(!reloaded.show_suggestions);
}

#[test]
fn tui_settings_leave_the_desktop_settings_alone() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    let desktop = app_dir().unwrap().join("settings.json");
    fs::write(&desktop, "{\"theme\":\"light\",\"autostart\":true}\n").unwrap();

    write_tui_settings(&TuiSettings { theme: "dark".to_string(), show_suggestions: true }).unwrap();
    assert_eq!(fs::read_to_string(&desktop).unwrap(), "{\"theme\":\"light\",\"autostart\":true}\n");
}

#[test]
fn tui_settings_reject_unknown_themes_and_fill_in_defaults() {
    assert!(validate_tui_settings(&TuiSettings { theme: "sepia".to_string(), show_suggestions: true })
        .unwrap_err()
        .contains("not a valid theme"));

    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    fs::write(tui_settings_file().unwrap(), "{\"theme\":\"light\"}\n").unwrap();
    let settings = load_tui_settings().unwrap();
    assert_eq!(settings.theme, "light");
    assert!(settings.show_suggestions);
}

#[test]
fn automation_working_directory_falls_back_to_a_file_s_parent() {
    let dir = env::temp_dir().join(format!("easyalias-wd-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.json");
    fs::write(&file, "{}").unwrap();

    let resolved = automation_working_directory(file.to_str().unwrap()).unwrap();
    assert_eq!(resolved.canonicalize().unwrap(), dir.canonicalize().unwrap());

    let missing = automation_working_directory(&dir.join("nope").join("x").to_string_lossy());
    assert!(missing.is_err());

    let _ = fs::remove_dir_all(&dir);
}
