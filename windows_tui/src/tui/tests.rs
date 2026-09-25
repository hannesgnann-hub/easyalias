//! End-to-end tests: key presses in, files on disk and rendered screens out.

use super::app::*;
use super::runner::StepStatus;
use crate::platform::test_support as shell;
use crate::tests::{TemporaryHome, HOME_LOCK};
use crate::*;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use std::time::{Duration, Instant};

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| super::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..height {
        for x in 0..width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn wait_for_run(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.run.as_ref().map(|run| run.running).unwrap_or(false) {
        assert!(Instant::now() < deadline, "automation run did not finish");
        std::thread::sleep(Duration::from_millis(20));
        app.on_tick();
    }
}

#[test]
fn creates_edits_favorites_and_trashes_an_alias_with_the_keyboard() {
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    let mut app = App::load();

    press(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "proj");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    type_text(&mut app, "~/Projects/my app");
    press(&mut app, KeyCode::Enter);
    assert!(app.modal.is_none(), "form should close after saving");

    let expected = platform::preview_command("navigate", "~/Projects/my app", "");
    let generated = shell::generated_aliases();
    assert!(generated.contains(&expected), "{}", generated);
    let _ = &home;

    // Edit: switch the action to Open.
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    ctrl(&mut app, 's');
    let alias = load_config_aliases().unwrap().pop().unwrap();
    assert_eq!(alias.action, "open");
    assert_eq!(
        alias.command_preview,
        platform::preview_command("open", "~/Projects/my app", "")
    );

    press(&mut app, KeyCode::Char(' '));
    assert!(load_config_aliases().unwrap()[0].favorite);

    press(&mut app, KeyCode::Char('d'));
    assert!(load_config_aliases().unwrap().is_empty());
    assert_eq!(list_trash().unwrap().len(), 1);

    press(&mut app, KeyCode::Char('t'));
    press(&mut app, KeyCode::Char('r'));
    assert_eq!(load_config_aliases().unwrap().len(), 1);
    assert!(list_trash().unwrap().is_empty());
}

#[test]
fn rejects_invalid_and_duplicate_aliases_in_the_form() {
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _home = TemporaryHome::create();
    let mut app = App::load();

    press(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "1bad");
    ctrl(&mut app, 's');
    match &app.modal {
        Some(Modal::AliasForm(form)) => assert!(form.error.contains("must start with a letter")),
        _ => panic!("form should stay open"),
    }
    press(&mut app, KeyCode::Esc);

    press(&mut app, KeyCode::Char('s'));
    let gs = crate::suggestions::SUGGESTIONS
        .iter()
        .position(|s| s.name == "gs")
        .unwrap();
    for _ in 0..gs {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert_eq!(load_config_aliases().unwrap().len(), 1);
    assert!(app
        .message
        .as_ref()
        .map(|m| m.error && m.text.contains("already exists"))
        .unwrap_or(false));
}

#[test]
fn builds_and_runs_an_automation_in_one_shell_session() {
    if !shell::CAN_RUN_AUTOMATIONS {
        return;
    }
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    fs::create_dir_all(home.path.join("work")).unwrap();
    let mut app = App::load();
    press(&mut app, KeyCode::Char('2'));

    press(&mut app, KeyCode::Char('n'));
    type_text(&mut app, "Chain");
    press(&mut app, KeyCode::Down);
    ctrl(&mut app, 'u');
    type_text(&mut app, "~/work");
    press(&mut app, KeyCode::Down);
    type_text(&mut app, "demo");
    press(&mut app, KeyCode::Down);
    type_text(&mut app, shell::SET_VAR_AND_GO_UP);
    ctrl(&mut app, 'p');
    ctrl(&mut app, 'u');
    type_text(&mut app, "1");
    ctrl(&mut app, 'n');
    type_text(&mut app, shell::ECHO_VAR_AND_FOLDER);
    ctrl(&mut app, 'n');
    type_text(&mut app, shell::FAIL);
    ctrl(&mut app, 'n');
    type_text(&mut app, "echo never");
    ctrl(&mut app, 's');
    assert!(
        app.modal.is_none(),
        "editor should close: {:?}",
        app.message.as_ref().map(|m| &m.text)
    );

    let saved = load_automations().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].group, "demo");
    let kinds: Vec<&str> = saved[0].steps.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(kinds, ["command", "wait", "command", "command", "command"]);
    assert_eq!(saved[0].steps[1].seconds, 1);

    press(&mut app, KeyCode::Enter);
    wait_for_run(&mut app);
    let run = app.run.as_ref().unwrap();
    let statuses: Vec<StepStatus> = run.steps.iter().map(|s| s.status).collect();
    assert_eq!(
        statuses,
        [
            StepStatus::Success,
            StepStatus::Success,
            StepStatus::Success,
            StepStatus::Error,
            StepStatus::Skipped
        ]
    );
    let expected_dir = home.path.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(run.steps[2].output, format!("hi from {}", expected_dir));
    assert!(run.error.contains("Step 4 failed"));
}

#[test]
fn stop_ends_a_running_automation_immediately() {
    if !shell::CAN_RUN_AUTOMATIONS {
        return;
    }
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    let mut automation = crate::tests::test_automation("slow", "Slow", &shell::sleep_command(30, "a"));
    automation.path = home.path.display().to_string();
    write_automation_entries(&[automation]).unwrap();

    let mut app = App::load();
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Enter);
    std::thread::sleep(Duration::from_millis(300));
    app.on_tick();
    let started = Instant::now();
    press(&mut app, KeyCode::Char('s'));
    wait_for_run(&mut app);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(app
        .run
        .as_ref()
        .unwrap()
        .error
        .contains("Automation stopped"));
}

#[test]
fn stop_keeps_background_jobs_running() {
    if !shell::CAN_RUN_AUTOMATIONS {
        return;
    }
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    // Unusual durations make both commands easy to find in the process list.
    let background_seconds = 700 + std::process::id() % 97;
    let foreground_seconds = 800 + std::process::id() % 97;
    let background_command = shell::sleep_command(background_seconds, "bg");
    let foreground_command = shell::sleep_command(foreground_seconds, "fg");
    let mut automation = crate::tests::test_automation("mix", "Mix", &background_command);
    automation.path = home.path.display().to_string();
    automation.steps[0].behavior = "background".to_string();
    let mut foreground = automation.steps[0].clone();
    foreground.id = "fg".to_string();
    foreground.behavior = "wait".to_string();
    foreground.command = foreground_command.clone();
    automation.steps.push(foreground);
    write_automation_entries(&[automation]).unwrap();

    let mut app = App::load();
    press(&mut app, KeyCode::Char('2'));
    press(&mut app, KeyCode::Enter);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.run.as_ref().map(|run| run.current < 1).unwrap_or(true) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
        app.on_tick();
    }
    std::thread::sleep(Duration::from_millis(300));
    press(&mut app, KeyCode::Char('s'));
    wait_for_run(&mut app);

    let background_alive = shell::sleep_is_running(background_seconds);
    let foreground_alive = shell::sleep_is_running(foreground_seconds);
    shell::stop_sleep(background_seconds);
    shell::stop_sleep(foreground_seconds);
    assert!(background_alive, "background job should keep running");
    assert!(!foreground_alive, "foreground command should be stopped");
}

#[test]
fn groups_filter_and_group_view() {
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _home = TemporaryHome::create();
    let mut git = crate::tests::test_automation("a", "Sync", "git pull");
    git.group = "repo".to_string();
    let build = crate::tests::test_automation("b", "Build", "npm run build");
    write_automation_entries(&[git, build]).unwrap();

    let mut app = App::load();
    app.automation_filter = "git".to_string();
    assert_eq!(app.automation_rows().len(), 1);
    app.automation_filter = "build".to_string();
    assert_eq!(app.automation_rows().len(), 1);

    app.automation_filter = "groups".to_string();
    let rows = app.automation_rows();
    assert!(matches!(&rows[0], AutomationRow::Header(name, 1) if name == "repo"));
    assert!(matches!(&rows[2], AutomationRow::Header(name, 1) if name == "Ungrouped"));
    app.automation_table.select(Some(0));
    app.skip_automation_headers(true);
    assert_eq!(app.selected_automation().unwrap().name, "Sync");
    app.move_automation_selection(1);
    assert_eq!(app.selected_automation().unwrap().name, "Build");

    // Cycling through every filter visits the per-group filter too.
    app.automation_filter = "all".to_string();
    let mut seen = Vec::new();
    for _ in 0..8 {
        app.cycle_automation_filter(true);
        seen.push(app.automation_filter.clone());
    }
    assert!(seen.contains(&"group:repo".to_string()));
}

#[test]
fn alias_filters_match_the_desktop_rules() {
    let alias = |action: &str, command: &str| AliasEntry {
        action: action.to_string(),
        command_preview: command.to_string(),
        ..crate::tests::test_alias("x", "x", command)
    };
    assert!(alias_matches_filter(&alias("custom", "git status"), "git"));
    assert!(!alias_matches_filter(&alias("custom", "gitk"), "git"));
    assert!(alias_matches_filter(
        &alias("custom", "docker-compose up"),
        "docker"
    ));
    assert!(alias_matches_filter(
        &alias("custom", "cd x && ./gradlew build"),
        "build"
    ));
    assert!(alias_matches_filter(
        &alias("custom", "pnpm run build"),
        "build"
    ));
    assert!(alias_matches_filter(
        &alias("custom", "cargo build --release"),
        "build"
    ));
    assert!(!alias_matches_filter(
        &alias("custom", "cargo test"),
        "build"
    ));
    assert!(alias_matches_filter(
        &alias("navigate", "cd x"),
        "navigation"
    ));
}

#[test]
fn exports_and_imports_an_alias_backup() {
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    save_aliases(vec![
        crate::tests::test_alias("1", "one", "echo one"),
        crate::tests::test_alias("2", "two", "echo two"),
    ])
    .unwrap();
    let mut app = App::load();

    press(&mut app, KeyCode::Char('b'));
    press(&mut app, KeyCode::Char(' ')); // uncheck "one"
    press(&mut app, KeyCode::Tab);
    ctrl(&mut app, 'u');
    type_text(&mut app, "~/backup.json");
    press(&mut app, KeyCode::Enter);
    assert!(app.modal.is_none());
    let backup = fs::read_to_string(home.path.join("backup.json")).unwrap();
    assert!(backup.contains("\"two\"") && !backup.contains("\"one\""));

    // Exporting again to the same file asks before overwriting.
    press(&mut app, KeyCode::Char('b'));
    press(&mut app, KeyCode::Tab);
    ctrl(&mut app, 'u');
    type_text(&mut app, "~/backup.json");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(&app.modal, Some(Modal::Backup(m)) if m.overwrite_armed));
    press(&mut app, KeyCode::Esc);

    save_aliases(Vec::new()).unwrap();
    press(&mut app, KeyCode::Char('B'));
    ctrl(&mut app, 'u');
    type_text(&mut app, "~/backup.json");
    press(&mut app, KeyCode::Enter); // open
    press(&mut app, KeyCode::Enter); // import
    assert!(app.modal.is_none());
    let names: Vec<String> = load_config_aliases()
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, ["two"]);
}

#[test]
fn theme_is_stored_in_the_tui_settings_only() {
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    let mut app = App::load();
    press(&mut app, KeyCode::Char('3'));
    press(&mut app, KeyCode::Right);
    assert_eq!(load_tui_settings().unwrap().theme, "light");
    assert!(!home.path.join(".easyalias/settings.json").exists());
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(!load_tui_settings().unwrap().show_suggestions);
}

#[test]
fn renders_every_view_and_modal_at_small_and_large_sizes() {
    let _lock = HOME_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TemporaryHome::create();
    shell::seed_importable_alias(&home.path);
    let mut automation = crate::tests::test_automation("a", "Deploy", "echo hi");
    automation.group = "ops".to_string();
    automation.path = "~".to_string();
    ensure_app_files().unwrap();
    write_automation_entries(&[automation]).unwrap();
    save_aliases(vec![crate::tests::test_alias("1", "gs", "git status")]).unwrap();

    let mut app = App::load();
    for (width, height) in [(60, 20), (80, 24), (160, 50)] {
        press(&mut app, KeyCode::Char('1'));
        press(&mut app, KeyCode::Char('i'));
        let text = screen(&mut app, width, height);
        assert!(text.contains("Import existing"), "{}", text);
        app.modal = None;

        for tab in ['1', '2', '3'] {
            press(&mut app, KeyCode::Char(tab));
            screen(&mut app, width, height);
        }
        press(&mut app, KeyCode::Char('1'));
        for key in ['n', 's', 't', 'b', 'B', '?'] {
            press(&mut app, KeyCode::Char(key));
            assert!(app.modal.is_some(), "key {} should open a modal", key);
            screen(&mut app, width, height);
            app.modal = None;
        }
        press(&mut app, KeyCode::Char('2'));
        for key in ['n', 'e', 'g', 'c', 'd', 't', 'b', 'B'] {
            press(&mut app, KeyCode::Char(key));
            assert!(app.modal.is_some(), "key {} should open a modal", key);
            screen(&mut app, width, height);
            app.modal = None;
        }
        if shell::CAN_RUN_AUTOMATIONS {
            press(&mut app, KeyCode::Enter);
            wait_for_run(&mut app);
            let text = screen(&mut app, width, height);
            assert!(text.contains("Finished"), "{}", text);
            app.modal = None;
        }
    }
}
