//! Everything the terminal UI needs to know about the operating system.
//! `src/tui/` is identical in the macOS, Linux and Windows TUIs - only this
//! file, the suggestions and the help texts differ.

use crate::*;

pub(crate) const SCHEDULER_NOTE: &str =
    "Windows Task Scheduler runs it even while EasyAlias is closed.";
pub(crate) const FOLDER_PLACEHOLDER: &str = "~\\Projects\\app  (Tab completes)";
pub(crate) const FILE_PLACEHOLDER: &str = "~\\path\\to\\file  (Tab completes)";

// ----- alias commands -------------------------------------------------------

// The cmd.exe command an alias runs - the same function the backend uses to
// write the .cmd files, so the preview can never disagree with the result.
pub(crate) fn preview_command(action: &str, path: &str, custom_command: &str) -> String {
    build_command_preview(&AliasEntry {
        id: String::new(),
        name: String::new(),
        path: path.to_string(),
        action: action.to_string(),
        custom_command: Some(custom_command.to_string()),
        command_preview: String::new(),
        favorite: false,
        created_at: String::new(),
        updated_at: String::new(),
    })
}

// How the generated alias reads, shown under the form.
pub(crate) fn preview_line(name: &str, command: &str) -> String {
    format!("{}.cmd  →  {}", name, command)
}

// Windows resolves commands case-insensitively: "GS" and "gs" are the same.
pub(crate) fn same_alias_name(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

// ----- PATH connection ------------------------------------------------------

pub(crate) fn empty_app_state() -> AppState {
    AppState {
        aliases: Vec::new(),
        config_file: "~/.easyalias/config.json".to_string(),
        command_dir: "~/.easyalias/bin".to_string(),
        path_entry: String::new(),
        path_configured: false,
        import_candidates: Vec::new(),
    }
}

// (connected, text) for the line above the alias list.
pub(crate) fn status_line(state: &AppState) -> (bool, String) {
    if state.path_configured {
        (true, format!("{} is on your PATH", state.command_dir))
    } else {
        (
            false,
            format!(
                "{} was added to your user PATH - open a new terminal to use your aliases",
                state.command_dir
            ),
        )
    }
}

pub(crate) fn saved_notice(state: &AppState) -> String {
    format!("Saved: {}", state.command_dir)
}

pub(crate) fn about_rows(state: &AppState) -> Vec<(&'static str, String)> {
    vec![
        ("Aliases data", state.config_file.clone()),
        ("Command files", state.command_dir.clone()),
        ("PATH entry", state.path_entry.clone()),
    ]
}

// ----- importing existing command files -------------------------------------

pub(crate) struct ImportCandidate {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) detail: String,
}

pub(crate) const IMPORT_TITLE: &str = "Import existing command files";

pub(crate) fn import_candidates(state: &AppState) -> Vec<ImportCandidate> {
    state
        .import_candidates
        .iter()
        .map(|candidate| ImportCandidate {
            id: candidate.id.clone(),
            name: candidate.name.clone(),
            detail: format!("{}   ({})", candidate.command, candidate.source_file),
        })
        .collect()
}

pub(crate) fn import_intro(_state: &AppState, count: usize) -> String {
    format!(
        "Found {} command file{} in your PATH folders. Imported files move into EasyAlias - a copy of each is kept in a backup folder first.",
        count,
        if count == 1 { "" } else { "s" }
    )
}

pub(crate) fn scan_import() -> Result<AppState, String> {
    scan_command_file_import()
}

pub(crate) fn nothing_to_import(_state: &AppState) -> String {
    "No new command files found in your PATH folders.".to_string()
}

// Returns the new state and the message to show.
pub(crate) fn import_selected(ids: Vec<String>) -> Result<(AppState, String), String> {
    let result = import_command_files(ids, now_iso())?;
    let mut notice = format!(
        "{} command files imported. Backup: {}",
        result.imported_count, result.backup_dir
    );
    if let Some(warning) = result.warning {
        notice.push_str(&format!(" - {}", warning));
    }
    Ok((result.state, notice))
}

pub(crate) fn dismiss_import() -> Result<(AppState, String), String> {
    let state = dismiss_command_file_import()?;
    Ok((
        state,
        "Existing command files were left unchanged.".to_string(),
    ))
}

// ----- processes and links --------------------------------------------------

pub(crate) fn open_url(url: &str) -> bool {
    // `start` treats the first quoted argument as the window title.
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn child_pids(pid: u32) -> Vec<u32> {
    let query = format!(
        "Get-CimInstance Win32_Process -Filter 'ParentProcessId={}' | ForEach-Object {{ $_.ProcessId }}",
        pid
    );
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &query])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn kill(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

// taskkill /T ends the process together with everything it started.
pub(crate) fn kill_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    #[test]
    fn builds_the_same_commands_as_the_desktop_app() {
        assert_eq!(
            preview_command("navigate", "~/Projects/app", ""),
            "cd /d \"%USERPROFILE%\\Projects\\app\""
        );
        assert_eq!(
            preview_command("open", "C:\\a b.txt", ""),
            "start \"\" \"C:\\a b.txt\""
        );
        assert_eq!(
            preview_command("execute", "C:\\Tools\\run.bat", ""),
            "call \"C:\\Tools\\run.bat\" %*"
        );
        assert_eq!(
            preview_command("compile_gradle", "~", ""),
            "cd /d \"%USERPROFILE%\" && call gradlew.bat build"
        );
        assert_eq!(
            preview_command("custom", "", "  git status  "),
            "git status"
        );
        assert_eq!(
            preview_command("open", "C:\\100%\\\"x\"", ""),
            "start \"\" \"C:\\100%%\\\"\"x\"\"\""
        );
        assert_eq!(preview_command("navigate", "  ", ""), "");
        assert!(same_alias_name("GS", "gs"));
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! cmd.exe snippets for the shared TUI tests in `tui/tests.rs`. Automations
    //! need a real cmd.exe, so those tests only run on Windows itself.
    use crate::*;

    pub(crate) const CAN_RUN_AUTOMATIONS: bool = cfg!(windows);
    pub(crate) const SET_VAR_AND_GO_UP: &str = "set GREETING=hi&& cd ..";
    pub(crate) const ECHO_VAR_AND_FOLDER: &str = "for %I in (.) do @echo %GREETING% from %~nxI";
    pub(crate) const FAIL: &str = "cmd /c exit 3";

    pub(crate) fn sleep_command(seconds: u32, _tag: &str) -> String {
        format!("powershell -NoProfile -Command Start-Sleep {}", seconds)
    }

    // The pattern is split so the query's own command line never matches it.
    fn sleep_filter(seconds: u32) -> String {
        format!(
            "Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like ('*Start-Sl' + 'eep {}*') }}",
            seconds
        )
    }

    pub(crate) fn sleep_is_running(seconds: u32) -> bool {
        let query = format!("@({}).Count", sleep_filter(seconds));
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &query])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim() != "0")
            .unwrap_or(false)
    }

    pub(crate) fn stop_sleep(seconds: u32) {
        let query = format!(
            "{} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force }}",
            sleep_filter(seconds)
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &query])
            .output();
    }

    pub(crate) fn generated_aliases() -> String {
        let mut text = String::new();
        for entry in fs::read_dir(command_dir().unwrap()).unwrap().flatten() {
            text.push_str(&fs::read_to_string(entry.path()).unwrap_or_default());
        }
        text
    }

    // A one-line .cmd file in a PATH folder inside the (temporary) profile.
    pub(crate) fn seed_importable_alias(home: &Path) {
        let tools = home.join("tools");
        fs::create_dir_all(&tools).unwrap();
        fs::write(tools.join("ll.cmd"), "@echo off\r\ndir /b\r\n").unwrap();
        let previous = env::var("PATH").unwrap_or_default();
        env::set_var("PATH", format!("{};{}", tools.display(), previous));
    }
}
