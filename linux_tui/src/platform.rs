//! Everything the terminal UI needs to know about the operating system.
//! `src/tui/` is identical in the macOS, Linux and Windows TUIs - only this
//! file, the suggestions and the help texts differ.

use crate::*;

pub(crate) const SCHEDULER_NOTE: &str =
    "A systemd user timer runs it even while EasyAlias is closed.";
pub(crate) const FOLDER_PLACEHOLDER: &str = "~/Projects/app  (Tab completes)";
pub(crate) const FILE_PLACEHOLDER: &str = "~/path/to/file  (Tab completes)";

// ----- alias commands -------------------------------------------------------

// Escape characters that can break a double-quoted shell string.
pub(crate) fn escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
}

// Converts a user-entered path into a safe Bash/zsh command argument.
// "~/" is expanded to "$HOME/" so generated aliases keep working reliably.
pub(crate) fn shell_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed == "~" {
        return "\"$HOME\"".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return format!("\"$HOME/{}\"", escape_double_quoted(rest));
    }
    format!("\"{}\"", escape_double_quoted(trimmed))
}

// Converts the selected action + path/custom command into the shell command
// that is written into aliases.sh (same rules as the desktop app).
pub(crate) fn preview_command(action: &str, path: &str, custom_command: &str) -> String {
    let path = shell_path(path);
    let with_path = |format: &dyn Fn(&str) -> String| {
        if path.is_empty() {
            String::new()
        } else {
            format(&path)
        }
    };
    match action {
        "navigate" => with_path(&|p| format!("cd {}", p)),
        "open" => with_path(&|p| format!("xdg-open {}", p)),
        "execute" => path.clone(),
        "compile_gradle" => with_path(&|p| format!("cd {} && ./gradlew build", p)),
        "compile_maven" => with_path(&|p| format!("cd {} && mvn clean package", p)),
        "custom" => custom_command.trim().to_string(),
        _ => String::new(),
    }
}

// How the generated alias reads, shown under the form.
pub(crate) fn preview_line(name: &str, command: &str) -> String {
    format!("alias {}='{}'", name, command)
}

// Alias names are case-sensitive in bash and zsh.
pub(crate) fn same_alias_name(left: &str, right: &str) -> bool {
    left == right
}

// ----- shell connection -----------------------------------------------------

pub(crate) fn empty_app_state() -> AppState {
    AppState {
        aliases: Vec::new(),
        config_file: "~/.easyalias/config.json".to_string(),
        aliases_file: "~/.easyalias/aliases.sh".to_string(),
        source_line: SOURCE_LINE.to_string(),
        shell_name: "bash".to_string(),
        shell_config_file: "~/.bashrc".to_string(),
        shell_source_present: false,
        import_candidates: Vec::new(),
    }
}

// (connected, text) for the line above the alias list.
pub(crate) fn status_line(state: &AppState) -> (bool, String) {
    if state.shell_source_present {
        (
            true,
            format!(
                "{} connected · {}",
                state.shell_name, state.shell_config_file
            ),
        )
    } else {
        (
            false,
            format!("Add `{}` to {}", state.source_line, state.shell_config_file),
        )
    }
}

pub(crate) fn saved_notice(state: &AppState) -> String {
    format!("Saved: {}", state.aliases_file)
}

pub(crate) fn about_rows(state: &AppState) -> Vec<(&'static str, String)> {
    vec![
        ("Aliases data", state.config_file.clone()),
        ("Generated file", state.aliases_file.clone()),
        ("Shell file", state.shell_config_file.clone()),
    ]
}

// ----- importing existing aliases -------------------------------------------

pub(crate) struct ImportCandidate {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) detail: String,
}

pub(crate) const IMPORT_TITLE: &str = "Import existing aliases";

pub(crate) fn import_candidates(state: &AppState) -> Vec<ImportCandidate> {
    state
        .import_candidates
        .iter()
        .map(|candidate| ImportCandidate {
            id: candidate.id.clone(),
            name: candidate.name.clone(),
            detail: format!("{}   (line {})", candidate.command, candidate.line_number),
        })
        .collect()
}

pub(crate) fn import_intro(state: &AppState, count: usize) -> String {
    format!(
        "Found {} alias{} in {}. Imported lines are commented out there - a backup of the file is written first.",
        count,
        if count == 1 { "" } else { "es" },
        state.shell_config_file
    )
}

pub(crate) fn scan_import() -> Result<AppState, String> {
    scan_shell_import()
}

pub(crate) fn nothing_to_import(state: &AppState) -> String {
    format!("No new aliases found in {}.", state.shell_config_file)
}

// Returns the new state and the message to show.
pub(crate) fn import_selected(ids: Vec<String>) -> Result<(AppState, String), String> {
    let result = import_shell_aliases(ids, now_iso())?;
    let notice = format!(
        "{} aliases imported. Backup: {}",
        result.imported_count, result.backup_file
    );
    Ok((result.state, notice))
}

pub(crate) fn dismiss_import() -> Result<(AppState, String), String> {
    let state = dismiss_shell_import()?;
    let notice = format!(
        "Existing aliases were left unchanged in {}.",
        state.shell_config_file
    );
    Ok((state, notice))
}

// ----- processes and links --------------------------------------------------

pub(crate) fn open_url(url: &str) -> bool {
    Command::new("xdg-open")
        .arg(url)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn child_pids(pid: u32) -> Vec<u32> {
    Command::new("pgrep")
        .arg("-P")
        .arg(pid.to_string())
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
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
}

// Collect the whole subtree first, then kill it, so children cannot be
// re-parented away before we have seen them.
pub(crate) fn kill_tree(pid: u32) {
    let mut all = vec![pid];
    let mut index = 0;
    while index < all.len() {
        let children = child_pids(all[index]);
        all.extend(children);
        index += 1;
    }
    for pid in all {
        kill(pid);
    }
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    #[test]
    fn builds_the_same_commands_as_the_desktop_app() {
        assert_eq!(
            preview_command("navigate", "~/Projects/app", ""),
            "cd \"$HOME/Projects/app\""
        );
        assert_eq!(preview_command("navigate", "~", ""), "cd \"$HOME\"");
        assert_eq!(
            preview_command("open", "/tmp/a b.txt", ""),
            "xdg-open \"/tmp/a b.txt\""
        );
        assert_eq!(preview_command("execute", "./run.sh", ""), "\"./run.sh\"");
        assert_eq!(
            preview_command("compile_gradle", "~/app", ""),
            "cd \"$HOME/app\" && ./gradlew build"
        );
        assert_eq!(
            preview_command("compile_maven", "/srv/x", ""),
            "cd \"/srv/x\" && mvn clean package"
        );
        assert_eq!(
            preview_command("custom", "", "  git status  "),
            "git status"
        );
        assert_eq!(preview_command("navigate", "  ", ""), "");
        assert_eq!(
            preview_command("open", "/a/$HOME\"`x`", ""),
            "xdg-open \"/a/\\$HOME\\\"\\`x\\`\""
        );
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Shell-specific snippets for the shared TUI tests in `tui/tests.rs`.
    use crate::*;

    pub(crate) const CAN_RUN_AUTOMATIONS: bool = true;
    pub(crate) const SET_VAR_AND_GO_UP: &str = "export GREETING=hi && cd ..";
    pub(crate) const ECHO_VAR_AND_FOLDER: &str = "echo $GREETING from $(basename $PWD)";
    pub(crate) const FAIL: &str = "false";

    pub(crate) fn sleep_command(seconds: u32, _tag: &str) -> String {
        format!("sleep {}", seconds)
    }

    pub(crate) fn sleep_is_running(seconds: u32) -> bool {
        Command::new("pgrep")
            .arg("-f")
            .arg(format!("^sleep {}$", seconds))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub(crate) fn stop_sleep(seconds: u32) {
        let _ = Command::new("pkill")
            .arg("-f")
            .arg(format!("^sleep {}$", seconds))
            .output();
    }

    pub(crate) fn generated_aliases() -> String {
        fs::read_to_string(aliases_file().unwrap()).unwrap()
    }

    pub(crate) fn seed_importable_alias(home: &Path) {
        let _ = home;
        fs::write(shell_setup().unwrap().config_file, "alias ll='ls -la'\n").unwrap();
    }
}
