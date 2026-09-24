//! Global automation hotkeys and main-window handling.

use crate::*;

// Parse an accelerator string ("CmdOrCtrl+Shift+L") into a Shortcut, returning
// None for anything malformed so callers can skip a bad entry instead of
// failing the whole batch.
pub(crate) fn parse_shortcut(accelerator: &str) -> Option<Shortcut> {
    let trimmed = accelerator.trim();
    if trimmed.is_empty() {
        return None;
    }
    Shortcut::from_str(trimmed).ok()
}

// (Re)registers every automation's global hotkey with the OS. Called once at
// startup and again after any change that can add, remove, or free a binding
// (assign/clear, delete, trash restore, backup import). Bad or already-taken
// combos are logged and skipped - one must never block the rest.
pub(crate) fn register_all_automation_hotkeys(app: &tauri::AppHandle) {
    let global_shortcut = app.global_shortcut();
    let _ = global_shortcut.unregister_all();

    let automations = match load_automation_entries() {
        Ok(automations) => automations,
        Err(error) => {
            eprintln!(
                "Automations could not be loaded for hotkey registration: {}",
                error
            );
            return;
        }
    };

    for automation in &automations {
        let Some(accelerator) = automation.hotkey.as_deref() else {
            continue;
        };
        match parse_shortcut(accelerator) {
            Some(shortcut) => {
                if let Err(error) = global_shortcut.register(shortcut) {
                    eprintln!(
                        "Hotkey \"{}\" for automation \"{}\" could not be registered: {}",
                        accelerator, automation.name, error
                    );
                }
            }
            None => eprintln!(
                "Hotkey \"{}\" for automation \"{}\" is not a valid shortcut.",
                accelerator, automation.name
            ),
        }
    }
}

// Reveal and focus the main window (used by the tray, dock reopen, and the
// "show run window" hotkey mode). Window/AppKit calls are marshalled onto the
// main thread so this is safe to call from the global-shortcut callback or a
// spawned worker thread.
pub(crate) fn show_main_window(app: &tauri::AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

// Fires when any registered global hotkey is pressed. Matches it back to the
// owning automation and either surfaces the run window or runs the automation
// headlessly, per the user's settings. All real work happens on a spawned
// thread so the shortcut callback returns immediately.
pub(crate) fn handle_global_shortcut(app: &tauri::AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    if state != ShortcutState::Pressed {
        return;
    }

    let automations = match load_automation_entries() {
        Ok(automations) => automations,
        Err(_) => return,
    };
    let Some(automation) = automations.into_iter().find(|automation| {
        automation
            .hotkey
            .as_deref()
            .and_then(parse_shortcut)
            .map(|existing| &existing == shortcut)
            .unwrap_or(false)
    }) else {
        return;
    };

    let behavior = load_app_settings()
        .map(|settings| settings.hotkey_behavior)
        .unwrap_or_else(|_| default_hotkey_behavior());
    let app = app.clone();

    thread::spawn(move || {
        let surface_window = || show_main_window(&app);

        if behavior == "background" {
            let result = run_automation_steps_headless(&automation);
            let (ok, message) = match &result {
                Ok(()) => (true, String::new()),
                Err(error) => (false, error.clone()),
            };
            // A silent background failure is confusing - bring the window up
            // so the error is visible; successful runs stay out of the way.
            if !ok {
                surface_window();
            }
            let _ = app.emit(
                "automation-hotkey-result",
                serde_json::json!({
                    "name": automation.name,
                    "ok": ok,
                    "message": message,
                }),
            );
        } else {
            surface_window();
            let _ = app.emit("automation-hotkey-fired", automation.id.clone());
        }
    });
}
