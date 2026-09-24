use super::*;
use std::ffi::OsString;
use std::sync::Mutex;

pub(crate) static HOME_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct TemporaryHome {
    pub(crate) path: PathBuf,
    previous_home: Option<OsString>,
    previous_shell: Option<OsString>,
}

impl TemporaryHome {
    pub(crate) fn create() -> Self {
        Self::create_with_shell("/bin/zsh")
    }

    pub(crate) fn create_with_shell(shell: &str) -> Self {
        let path = env::temp_dir().join(format!(
            "easyalias-import-test-{}-{}",
            std::process::id(),
            unix_timestamp().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        let previous_home = env::var_os("HOME");
        let previous_shell = env::var_os("SHELL");
        env::set_var("HOME", &path);
        env::set_var("SHELL", shell);

        Self {
            path,
            previous_home,
            previous_shell,
        }
    }
}

impl Drop for TemporaryHome {
    fn drop(&mut self) {
        if let Some(previous_home) = &self.previous_home {
            env::set_var("HOME", previous_home);
        } else {
            env::remove_var("HOME");
        }
        if let Some(previous_shell) = &self.previous_shell {
            env::set_var("SHELL", previous_shell);
        } else {
            env::remove_var("SHELL");
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

pub(crate) fn test_automation(id: &str, name: &str, command: &str) -> Automation {
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
fn parses_common_alias_forms_without_running_a_shell() {
    let single = parse_shell_alias_line("alias ll='ls -lah'", 4).unwrap();
    assert_eq!(single.name, "ll");
    assert_eq!(single.command, "ls -lah");
    assert_eq!(single.id, "shell-line-4");

    let double =
        parse_shell_alias_line(r#"alias project="cd \"$HOME/My Project\"""#, 8).unwrap();
    assert_eq!(double.command, "cd \"$HOME/My Project\"");

    let escaped = parse_shell_alias_line(r"alias notes=open\ ~/notes.txt", 12).unwrap();
    assert_eq!(escaped.command, "open ~/notes.txt");
}

#[test]
fn skips_aliases_that_are_unsafe_to_move_automatically() {
    assert!(parse_shell_alias_line("  alias nested='echo nested'", 1).is_none());
    assert!(parse_shell_alias_line("alias -g pipe='| grep'", 2).is_none());
    assert!(parse_shell_alias_line("alias one='echo one' two='echo two'", 3).is_none());
    assert!(parse_shell_alias_line("alias easya='open something-else'", 4).is_none());
    assert!(parse_shell_alias_line("alias broken='missing quote", 5).is_none());
}

#[test]
fn skips_repeated_alias_names() {
    let content = "alias gs='git status'\nalias ll='ls -lah'\nalias gs='git status --short'\n";
    let candidates = find_shell_aliases(content);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].name, "ll");
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
fn replaces_only_confirmed_lines_and_preserves_file_shape() {
    let content = "export PATH=/opt/bin:$PATH\nalias ll='ls -lah'\nalias gs='git status'\n";
    let selected = HashMap::from([(2, "ll")]);

    assert_eq!(
        replace_imported_alias_lines(content, &selected),
        "export PATH=/opt/bin:$PATH\n: # EasyAlias imported alias ll\nalias gs='git status'\n"
    );
}

#[test]
fn first_start_import_creates_backup_and_managed_files() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create();
    let zshrc_path = temporary_home.path.join(".zshrc");
    fs::write(
        &zshrc_path,
        "alias legacy='echo legacy'\nexport PATH=/opt/bin:$PATH\n",
    )
    .unwrap();

    let initial_state = load_aliases().unwrap();
    assert_eq!(initial_state.import_candidates.len(), 1);
    assert_eq!(initial_state.import_candidates[0].name, "legacy");

    let result = import_shell_aliases(
        vec![initial_state.import_candidates[0].id.clone()],
        "2026-07-17T12:00:00.000Z".to_string(),
    )
    .unwrap();

    assert_eq!(result.imported_count, 1);
    assert!(temporary_home
        .path
        .join(result.backup_file.trim_start_matches("~/"))
        .exists());
    assert!(fs::read_to_string(&zshrc_path)
        .unwrap()
        .contains(": # EasyAlias imported alias legacy"));
    assert!(fs::read_to_string(aliases_file().unwrap())
        .unwrap()
        .contains("alias legacy='echo legacy'"));
    assert_eq!(load_config_aliases().unwrap().len(), 1);
    assert!(import_marker_file(&shell_setup().unwrap())
        .unwrap()
        .exists());
}

#[test]
fn connects_zsh_and_bash_when_login_shell_is_zsh() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create_with_shell("/bin/zsh");

    let state = load_aliases().unwrap();
    assert_eq!(state.shell_name, "zsh + Bash");
    assert!(state.shell_config_file.contains("~/.zshrc"));
    assert!(state.shell_config_file.contains("~/.bash_profile"));
    assert!(state.shell_config_file.contains("~/.bashrc"));
    assert!(state.shell_source_present);

    for file_name in [".zshrc", ".bash_profile", ".bashrc"] {
        let content = fs::read_to_string(temporary_home.path.join(file_name)).unwrap();
        assert!(content.contains(SOURCE_LINE));
        // The TUI never adds the desktop app's `easya` launcher alias.
        assert!(!content.contains("alias easya="));
    }
}

#[test]
fn bash_imports_from_both_startup_files_with_separate_backups() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create_with_shell("/bin/bash");
    fs::write(
        temporary_home.path.join(".bash_profile"),
        "alias loginonly='echo login'\n",
    )
    .unwrap();
    fs::write(
        temporary_home.path.join(".bashrc"),
        "alias interactive='echo interactive'\n",
    )
    .unwrap();

    let state = load_aliases().unwrap();
    assert_eq!(state.import_candidates.len(), 2);
    let ids = state
        .import_candidates
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect();
    let result = import_shell_aliases(ids, "2026-08-14T00:00:00.000Z".to_string()).unwrap();

    assert_eq!(result.imported_count, 2);
    assert_eq!(result.backup_file.split(", ").count(), 2);
    assert!(
        fs::read_to_string(temporary_home.path.join(".bash_profile"))
            .unwrap()
            .contains(": # EasyAlias imported alias loginonly")
    );
    assert!(fs::read_to_string(temporary_home.path.join(".bashrc"))
        .unwrap()
        .contains(": # EasyAlias imported alias interactive"));
    assert!(result.backup_file.split(", ").all(|path| temporary_home
        .path
        .join(path.trim_start_matches("~/"))
        .exists()));
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

#[test]
fn automation_backup_export_contains_only_selected_workflows() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create();
    write_automation_entries(&[
        test_automation("build", "Build", "npm run build"),
        test_automation("deploy", "Deploy", "npm run deploy"),
    ])
    .unwrap();
    let destination = temporary_home.path.join("automations-selected.json");

    let result = export_automation_backup(
        vec!["deploy".to_string()],
        destination.display().to_string(),
        "2026-08-25T12:30:00.000Z".to_string(),
    )
    .unwrap();
    let backup = read_automation_backup(&destination).unwrap();

    assert_eq!(result.exported_count, 1);
    assert_eq!(backup.automations.len(), 1);
    assert_eq!(backup.automations[0].name, "Deploy");
}

#[test]
fn automation_backup_import_replaces_names_and_keeps_other_workflows() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let temporary_home = TemporaryHome::create();
    write_automation_entries(&[
        test_automation("current-build", "Build", "npm run build:old"),
        test_automation("keep", "Keep", "echo keep"),
    ])
    .unwrap();
    let backup_path = temporary_home.path.join("automations-restore.json");
    let backup = AutomationBackup {
        format: AUTOMATION_BACKUP_FORMAT.to_string(),
        version: AUTOMATION_BACKUP_VERSION,
        exported_at: "2026-08-25T12:30:00.000Z".to_string(),
        automations: vec![
            test_automation("backup-build", "Build", "npm run build:new"),
            test_automation("backup-test", "Test", "npm test"),
        ],
    };
    fs::write(&backup_path, serde_json::to_string(&backup).unwrap()).unwrap();

    let result = import_automation_backup_inner(
        backup_path.display().to_string(),
        vec!["backup-build".to_string(), "backup-test".to_string()],
        "2026-08-25T13:00:00.000Z".to_string(),
    )
    .unwrap();

    assert_eq!(result.imported_count, 2);
    assert_eq!(result.replaced_count, 1);
    assert_eq!(result.automations.len(), 3);
    assert!(result.automations.iter().any(|automation| {
        automation.name == "Build"
            && automation.steps[0].command == "npm run build:new"
            && automation.id != "backup-build"
    }));
    assert!(result
        .automations
        .iter()
        .any(|automation| automation.name == "Keep"));
    assert!(result
        .automations
        .iter()
        .any(|automation| automation.name == "Test"));
}

#[test]
fn deleted_automation_can_be_restored_from_trash() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    write_automation_entries(&[test_automation("build", "Build", "npm run build")]).unwrap();

    let deleted = move_automation_to_trash_inner("build").unwrap();
    assert!(deleted.automations.is_empty());
    assert_eq!(deleted.trash.len(), 1);
    assert_eq!(deleted.trash[0].automation.name, "Build");

    let restored = restore_trash_automation_inner("build").unwrap();
    assert_eq!(restored.automations.len(), 1);
    assert_eq!(restored.automations[0].name, "Build");
    assert!(restored.trash.is_empty());
}

#[test]
fn expired_automation_trash_entries_are_removed_automatically() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let _temporary_home = TemporaryHome::create();
    ensure_app_files().unwrap();
    let now = unix_timestamp().unwrap();
    write_automation_trash_entries(&[
        AutomationTrashEntry {
            automation: test_automation("expired", "Expired", "echo expired"),
            deleted_at: now.saturating_sub(TRASH_RETENTION_SECONDS + 1),
        },
        AutomationTrashEntry {
            automation: test_automation("current", "Current", "echo current"),
            deleted_at: now,
        },
    ])
    .unwrap();

    let trash = load_automation_trash_entries().unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].automation.id, "current");
    assert!(!fs::read_to_string(automation_trash_file().unwrap())
        .unwrap()
        .contains("expired"));
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

// Reproduces the reported bug: a "cd mac_src" step followed by a command
// that only succeeds from inside that subdirectory. Each step used to run
// in its own fresh shell, so the `cd` had no effect on the next step.
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
fn rejects_a_group_label_over_the_length_limit() {
    let mut automation = test_automation("grouped", "Grouped", "echo hi");
    automation.group = "g".repeat(61);

    let validation_error = validate_automations(&[automation]).unwrap_err();
    assert!(validation_error.contains("group label"));
}

#[test]
fn parses_valid_and_rejects_invalid_times() {
    assert_eq!(parse_time_of_day("09:30").unwrap(), (9, 30));
    assert_eq!(parse_time_of_day("00:00").unwrap(), (0, 0));
    assert_eq!(parse_time_of_day("23:59").unwrap(), (23, 59));
    assert!(parse_time_of_day("24:00").is_err());
    assert!(parse_time_of_day("9:30").is_ok()); // single-digit hour is fine, just parsed as u32
    assert!(parse_time_of_day("09:60").is_err());
    assert!(parse_time_of_day("not-a-time").is_err());
}

pub(crate) fn test_timed_automation(id: &str, automation_id: &str, time: &str) -> TimedAutomation {
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

// Exercises the real macOS scheduler end to end: writes a LaunchAgent
// plist, loads it with the actual `launchctl`, confirms launchd reports
// it as registered, then removes it and confirms launchd forgets it.
// Uses a temporary HOME so the plist never touches the developer's real
// ~/Library/LaunchAgents.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "needs launchd")]
fn schedules_and_unschedules_a_real_launchd_job() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let home = TemporaryHome::create();

    let mut entry = test_timed_automation(
        &format!("test-schedule-{}", unix_timestamp().unwrap()),
        "devstart",
        "03:17",
    );
    entry.days = vec!["mon".to_string()];

    schedule_timed_automation_macos(&entry).unwrap();

    let plist_path = timed_automation_plist_path(&entry.id).unwrap();
    assert!(plist_path.exists(), "plist was not written");
    let plist_content = fs::read_to_string(&plist_path).unwrap();
    assert!(plist_content.contains(&timed_automation_launchd_label(&entry.id)));
    assert!(plist_content.contains("<integer>3</integer>"));
    assert!(plist_content.contains("<integer>17</integer>"));
    assert!(plist_content.contains("<integer>1</integer>")); // Monday

    let label = timed_automation_launchd_label(&entry.id);
    let list_output = Command::new("launchctl").arg("list").arg(&label).output().unwrap();
    assert!(
        list_output.status.success(),
        "launchctl does not report the job as loaded: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );

    unschedule_timed_automation_macos(&entry.id).unwrap();
    assert!(!plist_path.exists(), "plist was not removed");

    let list_after = Command::new("launchctl").arg("list").arg(&label).output().unwrap();
    assert!(
        !list_after.status.success(),
        "launchctl still reports the job as loaded after unscheduling"
    );

    drop(home);
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

// Exercises the real macOS scheduler end to end for the shared sunrise/
// sunset checker job, the same way schedules_and_unschedules_a_real_
// launchd_job does for a single clock-time timed automation.
#[test]
#[cfg_attr(not(target_os = "macos"), ignore = "needs launchd")]
fn schedules_and_unschedules_the_real_sun_checker_launchd_job() {
    let _home_lock = HOME_LOCK.lock().unwrap();
    let home = TemporaryHome::create();

    schedule_sun_timed_automations_checker_macos().unwrap();

    let plist_path = sun_timed_automations_checker_plist_path().unwrap();
    assert!(plist_path.exists(), "plist was not written");
    let plist_content = fs::read_to_string(&plist_path).unwrap();
    assert!(plist_content.contains(&sun_timed_automations_checker_launchd_label()));
    assert!(plist_content.contains("--check-sun-timed-automations"));
    assert!(plist_content.contains(&format!("<integer>{}</integer>", SUN_CHECK_INTERVAL_SECONDS)));
    assert!(plist_content.contains("<key>RunAtLoad</key>"));

    let label = sun_timed_automations_checker_launchd_label();
    let list_output = Command::new("launchctl").arg("list").arg(&label).output().unwrap();
    assert!(
        list_output.status.success(),
        "launchctl does not report the job as loaded: {}",
        String::from_utf8_lossy(&list_output.stderr)
    );

    unschedule_sun_timed_automations_checker_macos().unwrap();
    assert!(!plist_path.exists(), "plist was not removed");

    let list_after = Command::new("launchctl").arg("list").arg(&label).output().unwrap();
    assert!(
        !list_after.status.success(),
        "launchctl still reports the job as loaded after unscheduling"
    );

    drop(home);
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
fn automation_hotkey_survives_a_json_round_trip() {
    let mut automation = test_automation("hk", "Hotkeyed", "echo hi");
    automation.hotkey = Some("CmdOrCtrl+Shift+L".to_string());

    let json = serde_json::to_string(&automation).unwrap();
    let restored: Automation = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.hotkey.as_deref(), Some("CmdOrCtrl+Shift+L"));

    // Older files without the field still load.
    let legacy = json.replace(",\"hotkey\":\"CmdOrCtrl+Shift+L\"", "");
    let restored_legacy: Automation = serde_json::from_str(&legacy).unwrap();
    assert_eq!(restored_legacy.hotkey, None);
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
