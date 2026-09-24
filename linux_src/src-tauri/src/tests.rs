use super::*;
use std::ffi::OsString;
use std::sync::Mutex;

static HOME_LOCK: Mutex<()> = Mutex::new(());

struct TemporaryHome {
    path: PathBuf,
    previous_home: Option<OsString>,
    previous_shell: Option<OsString>,
}

impl TemporaryHome {
    fn create() -> Self {
        let path = env::temp_dir().join(format!(
            "easyalias-linux-import-test-{}-{}",
            std::process::id(),
            unix_timestamp().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        let previous_home = env::var_os("HOME");
        let previous_shell = env::var_os("SHELL");
        env::set_var("HOME", &path);
        env::set_var("SHELL", "/bin/bash");
        Self {
            path,
            previous_home,
            previous_shell,
        }
    }
}

impl Drop for TemporaryHome {
    fn drop(&mut self) {
        if let Some(value) = &self.previous_home {
            env::set_var("HOME", value);
        } else {
            env::remove_var("HOME");
        }
        if let Some(value) = &self.previous_shell {
            env::set_var("SHELL", value);
        } else {
            env::remove_var("SHELL");
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn test_alias(id: &str, name: &str, command: &str) -> AliasEntry {
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
fn parses_only_safe_single_line_aliases() {
    let alias = parse_shell_alias_line("alias ll='ls -lah'", 3).unwrap();
    assert_eq!(alias.name, "ll");
    assert_eq!(alias.command, "ls -lah");
    assert!(parse_shell_alias_line("  alias nested='echo no'", 4).is_none());
    assert!(parse_shell_alias_line("alias -g pipe='| grep'", 5).is_none());
    assert!(parse_shell_alias_line("alias a='one' b='two'", 6).is_none());
}

#[test]
fn skips_repeated_names() {
    let aliases = find_shell_aliases(
        "alias gs='git status'\nalias ll='ls -lah'\nalias gs='git status --short'\n",
    );
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0].name, "ll");
}

#[test]
fn old_alias_json_defaults_to_not_favorite() {
    let legacy = r#"{
        "id":"legacy-1",
        "name":"ll",
        "path":"",
        "action":"custom",
        "customCommand":"ls -lah",
        "commandPreview":"ls -lah",
        "createdAt":"2026-07-01T12:00:00.000Z",
        "updatedAt":"2026-07-01T12:00:00.000Z"
    }"#;

    let alias: AliasEntry = serde_json::from_str(legacy).unwrap();
    assert!(!alias.favorite);
}

#[test]
fn deleted_alias_can_be_restored_from_trash() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    write_alias_files(&[test_alias("one", "ll", "ls -lah")]).unwrap();

    let deleted = move_alias_to_trash("one".to_string()).unwrap();
    assert!(deleted.state.aliases.is_empty());
    assert_eq!(deleted.trash.len(), 1);
    assert_eq!(deleted.trash[0].alias.name, "ll");
    assert!(!fs::read_to_string(aliases_file().unwrap())
        .unwrap()
        .contains("alias ll="));

    let restored = restore_trash_alias("one".to_string()).unwrap();
    assert_eq!(restored.state.aliases.len(), 1);
    assert!(restored.trash.is_empty());
    assert!(fs::read_to_string(aliases_file().unwrap())
        .unwrap()
        .contains("alias ll='ls -lah'"));
}

#[test]
fn expired_trash_entries_are_removed_automatically() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
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
fn first_start_import_uses_detected_shell_and_creates_backup() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create();
    let bashrc = temporary_home.path.join(".bashrc");
    fs::write(&bashrc, "alias legacy='echo legacy'\nexport TEST=1\n").unwrap();

    let initial = load_aliases().unwrap();
    assert_eq!(initial.shell_name, "bash");
    assert_eq!(initial.import_candidates.len(), 1);

    let result = import_shell_aliases(
        vec![initial.import_candidates[0].id.clone()],
        "2026-07-18T10:00:00.000Z".to_string(),
    )
    .unwrap();

    assert_eq!(result.imported_count, 1);
    assert!(temporary_home
        .path
        .join(result.backup_file.trim_start_matches("~/"))
        .exists());
    assert!(fs::read_to_string(&bashrc)
        .unwrap()
        .contains(": # EasyAlias imported alias legacy"));
    assert!(fs::read_to_string(aliases_file().unwrap())
        .unwrap()
        .contains("alias legacy='echo legacy'"));
}

#[test]
fn backup_export_contains_only_selected_aliases() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    write_alias_files(&[
        test_alias("one", "ll", "ls -lah"),
        test_alias("two", "gs", "git status"),
    ])
    .unwrap();
    let destination = temporary_home.path.join("selected.json");

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
    let temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    write_alias_files(&[
        test_alias("current-ll", "ll", "ls"),
        test_alias("keep", "gs", "git status"),
    ])
    .unwrap();
    let backup_path = temporary_home.path.join("restore.json");
    let backup = AliasBackup {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at: "2026-08-13T18:30:00.000Z".to_string(),
        aliases: vec![
            test_alias("backup-ll", "ll", "ls -lah"),
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
        .any(|alias| alias.name == "ll" && alias.command_preview == "ls -lah"));
    assert!(result.state.aliases.iter().any(|alias| alias.name == "gs"));
    assert!(result.state.aliases.iter().any(|alias| alias.name == "dcu"));
}

fn test_automation(id: &str, name: &str, command: &str) -> Automation {
    Automation {
        id: id.to_string(),
        name: name.to_string(),
        path: "~/Projects".to_string(),
        steps: vec![AutomationStep {
            id: format!("{}-step", id),
            kind: "command".to_string(),
            command: command.to_string(),
            seconds: 0,
            behavior: "wait".to_string(),
        }],
        favorite: false,
        group: String::new(),
        hotkey: None,
        created_at: "2026-08-25T12:00:00.000Z".to_string(),
        updated_at: "2026-08-25T12:00:00.000Z".to_string(),
    }
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

#[test]
fn rejects_a_group_label_over_the_length_limit() {
    let mut automation = test_automation("grouped", "Grouped", "echo hi");
    automation.group = "g".repeat(61);

    let validation_error = validate_automations(&[automation]).unwrap_err();
    assert!(validation_error.contains("group label"));
}

// Reproduces the reported macOS bug this session model was built to fix: a
// "cd mac_src" step followed by a command that only succeeds from inside
// that subdirectory. Each step used to run in its own fresh shell, so the
// `cd` had no effect on the next step.
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

    let pwd_result = execute_in_session(&mut session, "pwd", false).unwrap();
    assert_eq!(pwd_result.exit_code, Some(0));
    assert!(
        pwd_result.stdout.trim().ends_with("/mac_src"),
        "expected pwd to report the subdirectory, got: {:?}",
        pwd_result.stdout
    );

    let _ = session.child.kill();
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn automation_session_background_command_does_not_block_next_step() {
    let base = env::temp_dir().join(format!(
        "easyalias-automation-session-bg-test-{}",
        unix_timestamp().unwrap()
    ));
    fs::create_dir_all(&base).unwrap();

    let mut session = spawn_automation_session(&base).unwrap();

    let bg_result = execute_in_session(&mut session, "sleep 5", true).unwrap();
    assert!(bg_result.process_id.is_some());

    let echo_result = execute_in_session(&mut session, "echo still-here", false).unwrap();
    assert_eq!(echo_result.exit_code, Some(0));
    assert_eq!(echo_result.stdout.trim(), "still-here");

    let _ = session.child.kill();
    let _ = fs::remove_dir_all(&base);
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

fn test_timed_automation(id: &str, automation_id: &str, time: &str) -> TimedAutomation {
    TimedAutomation {
        id: id.to_string(),
        automation_id: automation_id.to_string(),
        trigger_kind: "clock".to_string(),
        time: time.to_string(),
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
fn validates_timed_automation_against_its_target() {
    let automation = test_automation("devstart", "DevStart", "echo hi");
    let mut entry = test_timed_automation("timed-1", "devstart", "09:00");
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

#[test]
fn builds_the_expected_oncalendar_expression() {
    let mut entry = test_timed_automation("timed-1", "devstart", "09:05");
    assert_eq!(
        timed_automation_on_calendar(&entry).unwrap(),
        "*-*-* 09:05:00"
    );

    entry.days = vec!["mon".to_string(), "fri".to_string()];
    assert_eq!(
        timed_automation_on_calendar(&entry).unwrap(),
        "Mon,Fri *-*-* 09:05:00"
    );
}

#[test]
fn rejects_an_unknown_trigger_kind() {
    let automation = test_automation("devstart", "DevStart", "echo hi");
    let mut entry = test_timed_automation("timed-1", "devstart", "09:00");
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
    let entry = test_timed_automation("timed-1", "devstart", "14:30");
    assert_eq!(resolve_trigger_time_today(&entry, 80, 0).unwrap(), Some((14, 30)));
}

#[test]
fn resolves_sunrise_trigger_using_the_configured_region() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let home = TemporaryHome::create();

    write_sun_location(&SunLocationSetting {
        region: "eu-central".to_string(),
    })
    .unwrap();
    let mut entry = test_timed_automation("timed-1", "devstart", "");
    entry.trigger_kind = "sunrise".to_string();
    let resolved = resolve_trigger_time_today(&entry, 172, 120).unwrap();
    assert!(resolved.is_some());

    drop(home);
}

#[test]
fn check_sun_timed_automations_skips_an_entry_already_triggered_today() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let home = TemporaryHome::create();

    write_sun_location(&SunLocationSetting {
        region: "eu-central".to_string(),
    })
    .unwrap();
    let automation = test_automation("devstart", "DevStart", "echo hi");
    write_automation_entries(&[automation]).unwrap();

    let mut entry = test_timed_automation("timed-1", "devstart", "");
    entry.trigger_kind = "sunrise".to_string();
    entry.last_triggered_date = Some(local_now().date);
    write_timed_automation_entries(&[entry]).unwrap();

    check_sun_timed_automations().unwrap();

    let entries = load_timed_automation_entries().unwrap();
    assert_eq!(
        entries[0].last_run_at, None,
        "an entry already triggered today should not run again"
    );

    drop(home);
}

#[test]
fn check_sun_timed_automations_skips_a_disabled_entry() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let home = TemporaryHome::create();

    write_sun_location(&SunLocationSetting {
        region: "eu-central".to_string(),
    })
    .unwrap();
    let automation = test_automation("devstart", "DevStart", "echo hi");
    write_automation_entries(&[automation]).unwrap();

    let mut entry = test_timed_automation("timed-1", "devstart", "");
    entry.trigger_kind = "sunrise".to_string();
    entry.enabled = false;
    write_timed_automation_entries(&[entry]).unwrap();

    check_sun_timed_automations().unwrap();

    let entries = load_timed_automation_entries().unwrap();
    assert_eq!(entries[0].last_run_at, None, "a disabled entry should never run");
    assert_eq!(entries[0].last_triggered_date, None);

    drop(home);
}

#[test]
fn app_settings_round_trip_and_default_when_missing() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();

    let defaults = load_app_settings().unwrap();
    assert_eq!(defaults.theme, "system");
    assert_eq!(defaults.hotkey_behavior, "window");

    write_app_settings(&AppSettings {
        theme: "dark".to_string(),
        hotkey_behavior: "background".to_string(),
        show_suggestions: true,
        autostart: false,
    })
    .unwrap();

    let reloaded = load_app_settings().unwrap();
    assert_eq!(reloaded.theme, "dark");
    assert_eq!(reloaded.hotkey_behavior, "background");
}

#[test]
fn save_settings_rejects_unknown_values() {
    assert!(validate_app_settings(&AppSettings {
        theme: "sepia".to_string(),
        hotkey_behavior: "window".to_string(),
        show_suggestions: true,
        autostart: false,
    })
    .unwrap_err()
    .contains("not a valid theme"));

    assert!(validate_app_settings(&AppSettings {
        theme: "light".to_string(),
        hotkey_behavior: "silent".to_string(),
        show_suggestions: true,
        autostart: false,
    })
    .unwrap_err()
    .contains("not a valid hotkey behavior"));
}

#[test]
fn partial_settings_file_fills_in_defaults() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    fs::write(settings_file().unwrap(), "{\"theme\":\"light\"}\n").unwrap();

    let settings = load_app_settings().unwrap();
    assert_eq!(settings.theme, "light");
    assert_eq!(settings.hotkey_behavior, "window");
    assert!(settings.show_suggestions);
}

#[test]
fn automation_hotkey_survives_a_json_round_trip() {
    let mut automation = test_automation("hk", "Hotkeyed", "echo hi");
    automation.hotkey = Some("CmdOrCtrl+Shift+L".to_string());

    let json = serde_json::to_string(&automation).unwrap();
    let restored: Automation = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.hotkey.as_deref(), Some("CmdOrCtrl+Shift+L"));

    let legacy = json.replace(",\"hotkey\":\"CmdOrCtrl+Shift+L\"", "");
    let restored_legacy: Automation = serde_json::from_str(&legacy).unwrap();
    assert_eq!(restored_legacy.hotkey, None);
}

#[test]
fn parse_shortcut_accepts_accelerators_and_rejects_junk() {
    assert!(parse_shortcut("CmdOrCtrl+Shift+L").is_some());
    assert!(parse_shortcut("  Alt+F4  ").is_some());
    assert!(parse_shortcut("").is_none());
    assert!(parse_shortcut("not a shortcut").is_none());
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
