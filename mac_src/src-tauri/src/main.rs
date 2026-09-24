use chrono::{DateTime, Datelike, Local, Offset, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    str::FromStr,
    sync::{mpsc, Mutex},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

mod models;
mod paths;
mod clock;
mod shell;
mod aliases;
mod automations;
mod session;
mod timed;
mod sun;
mod launchd;
mod settings;
mod hotkeys;

use models::*;
use paths::*;
use clock::*;
use shell::*;
use aliases::*;
use automations::*;
use session::*;
use timed::*;
use sun::*;
use launchd::*;
use settings::*;
use hotkeys::*;

fn main() {
    // launchd invokes this same executable to fire a timed automation, with
    // no window and no Tauri runtime - handle that before anything else
    // touches the GUI, then exit without ever starting the app.
    let args: Vec<String> = env::args().collect();
    if args.len() >= 3 && args[1] == "--run-timed-automation" {
        let exit_code = match run_timed_automation_headless(&args[2]) {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Timed automation {} failed: {}", args[2], error);
                1
            }
        };
        std::process::exit(exit_code);
    }
    if args.len() >= 2 && args[1] == "--check-sun-timed-automations" {
        let exit_code = match check_sun_timed_automations() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("Sunrise/sunset timed automations check failed: {}", error);
                1
            }
        };
        std::process::exit(exit_code);
    }
    // Register native plugins before exposing commands to the frontend.
    // dialog = file/folder picker, opener = open GitHub in the system browser.
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    handle_global_shortcut(app, shortcut, event.state());
                })
                .build(),
        )
        .manage(AutomationSessions::default())
        .on_window_event(|window, event| {
            // Closing the window only hides it - EasyAlias keeps running in the
            // menu-bar tray so global hotkeys stay active. "Quit EasyAlias"
            // (tray menu or the app menu) is the real exit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            let handle = app.handle();

            // Bring every saved automation hotkey live, even when the window
            // starts hidden - the shortcuts must work without focusing the app.
            register_all_automation_hotkeys(handle);

            // Menu-bar tray so the app is reachable while its window is hidden.
            let show_item =
                MenuItem::with_id(app, "show", "Show EasyAlias", true, None::<&str>)?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit EasyAlias", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("easyalias")
                .tooltip("EasyAlias")
                .menu(&tray_menu)
                // Left click reveals the window; right click opens the menu.
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                });
            // A monochrome template image on macOS: the system tints it to
            // match the menu bar (black on light, white on dark).
            #[cfg(target_os = "macos")]
            {
                tray = tray
                    .icon(tauri::image::Image::from_bytes(include_bytes!(
                        "../icons/tray-icon.png"
                    ))?)
                    .icon_as_template(true);
            }
            #[cfg(not(target_os = "macos"))]
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;

            // Autostart launches with a flag so the window stays hidden in the
            // tray until the user opens it.
            if !env::args().any(|arg| arg == AUTOSTART_FLAG) {
                show_main_window(handle);
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_aliases,
            save_aliases,
            load_automations,
            save_automations,
            set_automation_hotkey,
            list_automation_trash,
            move_automation_to_trash,
            restore_trash_automation,
            permanently_delete_trash_automation,
            empty_automation_trash,
            export_automation_backup,
            inspect_automation_backup,
            import_automation_backup,
            start_automation_session,
            run_session_command,
            stop_automation_session,
            list_timed_automations,
            save_timed_automation,
            delete_timed_automation,
            list_sun_regions,
            load_sun_location_state,
            save_sun_location,
            load_settings,
            save_settings,
            list_trash,
            move_alias_to_trash,
            restore_trash_alias,
            permanently_delete_trash_alias,
            empty_trash,
            export_alias_backup,
            inspect_alias_backup,
            import_alias_backup,
            scan_shell_import,
            dismiss_shell_import,
            import_shell_aliases
        ])
        .run(tauri::generate_context!())
        .expect("error while running EasyAlias");
}

#[cfg(test)]
mod tests;
