//! Application state and every data operation (create, edit, trash, backup,
//! import, schedule ...). Rendering lives in `ui`, key handling in `keys`.

use super::input::TextInput;
use super::runner::RunState;
use super::theme::Theme;
use crate::suggestions::SUGGESTIONS;
use crate::*;
use ratatui::widgets::TableState;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Aliases,
    Automations,
    Settings,
}

pub(crate) const ALIAS_FILTERS: [(&str, &str); 6] = [
    ("all", "All aliases"),
    ("favorites", "Favorites"),
    ("git", "Git"),
    ("docker", "Docker"),
    ("navigation", "Navigation"),
    ("build", "Build"),
];

pub(crate) const AUTOMATION_STATIC_FILTERS: [(&str, &str); 7] = [
    ("all", "All automations"),
    ("favorites", "Favorites"),
    ("background", "Background"),
    ("git", "Git"),
    ("docker", "Docker"),
    ("build", "Build"),
    ("groups", "Group view"),
];

pub(crate) struct Message {
    pub(crate) text: String,
    pub(crate) error: bool,
    at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupKind {
    Aliases,
    Automations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupMode {
    Export,
    Import,
}

pub(crate) struct BackupItem {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) detail: String,
}

pub(crate) struct BackupModal {
    pub(crate) kind: BackupKind,
    pub(crate) mode: BackupMode,
    pub(crate) path: TextInput,
    pub(crate) items: Vec<BackupItem>,
    pub(crate) checked: HashSet<String>,
    pub(crate) selected: usize,
    pub(crate) focus_list: bool,
    pub(crate) loaded_path: Option<String>,
    pub(crate) overwrite_armed: bool,
    pub(crate) error: String,
    pub(crate) completions: Vec<String>,
}

pub(crate) struct AliasForm {
    pub(crate) editing_id: Option<String>,
    pub(crate) name: TextInput,
    pub(crate) action: usize,
    pub(crate) path: TextInput,
    pub(crate) custom: TextInput,
    pub(crate) focus: usize,
    pub(crate) error: String,
    pub(crate) completions: Vec<String>,
}

impl AliasForm {
    pub(crate) fn action(&self) -> &'static str {
        ACTIONS[self.action]
    }
    pub(crate) fn preview(&self) -> String {
        build_command_preview(self.action(), &self.path.value, &self.custom.value)
    }
}

pub(crate) struct EditorState {
    pub(crate) automation: Automation,
    pub(crate) name: TextInput,
    pub(crate) path: TextInput,
    pub(crate) group: TextInput,
    // One input per step: the command, or the seconds of a wait step.
    pub(crate) step_inputs: Vec<TextInput>,
    pub(crate) focus: usize,
    pub(crate) is_new: bool,
    pub(crate) error: String,
    pub(crate) completions: Vec<String>,
}

pub(crate) const EDITOR_FIXED_FIELDS: usize = 3;

pub(crate) struct ScheduleState {
    pub(crate) entry: TimedAutomation,
    pub(crate) automation_name: String,
    pub(crate) exists: bool,
    pub(crate) time: TextInput,
    pub(crate) focus: usize,
    pub(crate) day_cursor: usize,
    pub(crate) error: String,
}

pub(crate) const TRIGGERS: [(&str, &str); 3] = [
    ("clock", "Time"),
    ("sunrise", "Sunrise"),
    ("sunset", "Sunset"),
];

pub(crate) enum ConfirmAction {
    DeleteAutomation(String),
    DeleteAliasForever(String),
    EmptyAliasTrash,
    DeleteAutomationForever(String),
    EmptyAutomationTrash,
    RemoveSchedule(String),
    Quit,
}

pub(crate) enum Modal {
    AliasForm(AliasForm),
    Suggestions {
        selected: usize,
    },
    ShellImport {
        selected: usize,
        checked: HashSet<String>,
    },
    AliasTrash {
        selected: usize,
    },
    AutomationTrash {
        selected: usize,
    },
    Backup(BackupModal),
    Editor(EditorState),
    Group {
        automation_id: String,
        input: TextInput,
        selected: Option<usize>,
    },
    Schedule(ScheduleState),
    Run,
    Confirm {
        text: String,
        action: ConfirmAction,
        back: Option<Box<Modal>>,
    },
    Help {
        topic: usize,
        step: usize,
    },
}

pub(crate) enum AutomationRow {
    Header(String, usize),
    Item(usize),
}

pub(crate) struct App {
    pub(crate) tab: Tab,
    pub(crate) quit: bool,
    pub(crate) alias_state: AppState,
    pub(crate) alias_trash: Vec<TrashEntry>,
    pub(crate) automations: Vec<Automation>,
    pub(crate) automation_trash: Vec<AutomationTrashEntry>,
    pub(crate) timed: Vec<TimedAutomation>,
    pub(crate) sun_location: SunLocationSetting,
    pub(crate) settings: TuiSettings,
    pub(crate) theme: Theme,

    pub(crate) alias_table: TableState,
    pub(crate) alias_search: TextInput,
    pub(crate) alias_search_active: bool,
    pub(crate) alias_filter: usize,

    pub(crate) automation_table: TableState,
    pub(crate) automation_search: TextInput,
    pub(crate) automation_search_active: bool,
    pub(crate) automation_filter: String,

    pub(crate) settings_selected: usize,
    pub(crate) modal: Option<Modal>,
    pub(crate) message: Option<Message>,
    pub(crate) run: Option<RunState>,
    pub(crate) tick: u64,
}

pub(crate) fn compare_aliases(left: &AliasEntry, right: &AliasEntry) -> std::cmp::Ordering {
    right
        .favorite
        .cmp(&left.favorite)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
}

pub(crate) fn compare_automations(left: &Automation, right: &Automation) -> std::cmp::Ordering {
    right
        .favorite
        .cmp(&left.favorite)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
}

// Word match used by the Git/Docker/Build filters, same rules as the desktop app:
// the tool has to appear as its own word at the start or after a separator.
fn has_word(command: &str, words: &[&str]) -> bool {
    let tokens: Vec<&str> = command
        .split(|c: char| c.is_whitespace() || c == ';' || c == '&' || c == '|')
        .filter(|token| !token.is_empty())
        .collect();
    tokens.iter().any(|token| words.contains(token))
}

fn has_build_command(command: &str) -> bool {
    let tokens: Vec<&str> = command
        .split(|c: char| c.is_whitespace() || c == ';' || c == '&' || c == '|')
        .filter(|token| !token.is_empty())
        .collect();
    let tool = |token: &str| {
        let token = token.strip_prefix("./").unwrap_or(token);
        matches!(token, "gradle" | "gradlew" | "mvn" | "mvnw" | "make")
    };
    if tokens.iter().any(|token| tool(token)) {
        return true;
    }
    tokens
        .windows(2)
        .any(|pair| pair[0] == "cargo" && pair[1] == "build")
        || tokens
            .windows(2)
            .any(|pair| matches!(pair[0], "npm" | "pnpm" | "yarn" | "bun") && pair[1] == "build")
        || tokens.windows(3).any(|triple| {
            matches!(triple[0], "npm" | "pnpm" | "yarn" | "bun")
                && triple[1] == "run"
                && triple[2] == "build"
        })
}

pub(crate) fn alias_matches_filter(alias: &AliasEntry, filter: &str) -> bool {
    let command = alias.command_preview.trim().to_lowercase();
    match filter {
        "favorites" => alias.favorite,
        "git" => has_word(&command, &["git"]),
        "docker" => has_word(&command, &["docker", "docker-compose"]),
        "navigation" => alias.action == "navigate",
        "build" => {
            alias.action == "compile_gradle"
                || alias.action == "compile_maven"
                || has_build_command(&command)
        }
        _ => true,
    }
}

pub(crate) fn automation_matches_filter(automation: &Automation, filter: &str) -> bool {
    if let Some(group) = filter.strip_prefix("group:") {
        return automation.group.trim() == group;
    }
    let command_text = automation
        .steps
        .iter()
        .filter(|step| step.kind == "command")
        .map(|step| step.command.as_str())
        .collect::<Vec<_>>()
        .join(" \n ")
        .to_lowercase();
    match filter {
        "favorites" => automation.favorite,
        "background" => automation
            .steps
            .iter()
            .any(|step| step.kind == "command" && step.behavior == "background"),
        "git" => has_word(&command_text, &["git"]),
        "docker" => has_word(&command_text, &["docker", "docker-compose"]),
        "build" => has_build_command(&command_text),
        _ => true,
    }
}

pub(crate) fn format_days(days: &[String]) -> String {
    if days.is_empty() {
        return "Every day".to_string();
    }
    WEEKDAYS
        .iter()
        .filter(|day| days.iter().any(|selected| selected == *day))
        .map(|day| weekday_label(day))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn weekday_label(day: &str) -> &'static str {
    match day {
        "mon" => "Mon",
        "tue" => "Tue",
        "wed" => "Wed",
        "thu" => "Thu",
        "fri" => "Fri",
        "sat" => "Sat",
        "sun" => "Sun",
        _ => "?",
    }
}

pub(crate) fn format_trigger(entry: &TimedAutomation) -> String {
    match entry.trigger_kind.as_str() {
        "sunrise" => "Sunrise".to_string(),
        "sunset" => "Sunset".to_string(),
        _ => entry.time.clone(),
    }
}

pub(crate) fn relative_time(unix_seconds: u64) -> String {
    let now = unix_timestamp().unwrap_or(unix_seconds);
    let delta = now.saturating_sub(unix_seconds);
    match delta {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", delta / 60),
        3600..=86_399 => format!("{} h ago", delta / 3600),
        _ => format!("{} d ago", delta / 86_400),
    }
}

pub(crate) fn days_left(deleted_at: u64) -> u64 {
    let now = unix_timestamp().unwrap_or(deleted_at);
    let expires = deleted_at + TRASH_RETENTION_SECONDS;
    if expires <= now {
        0
    } else {
        (expires - now).div_ceil(86_400)
    }
}

fn default_backup_path(kind: BackupKind) -> String {
    let date = local_now().date;
    match kind {
        BackupKind::Aliases => format!("~/easyalias-backup-{}.json", date),
        BackupKind::Automations => format!("~/easyalias-automations-{}.json", date),
    }
}

impl App {
    pub(crate) fn load() -> Self {
        let mut errors: Vec<String> = Vec::new();
        let alias_state = load_aliases().unwrap_or_else(|error| {
            errors.push(error);
            AppState {
                aliases: Vec::new(),
                config_file: "~/.easyalias/config.json".to_string(),
                aliases_file: "~/.easyalias/aliases.zsh".to_string(),
                source_line: SOURCE_LINE.to_string(),
                shell_name: "zsh + Bash".to_string(),
                shell_config_file: "~/.zshrc, ~/.bash_profile and ~/.bashrc".to_string(),
                shell_source_present: false,
                import_candidates: Vec::new(),
            }
        });
        let alias_trash = list_trash().unwrap_or_else(|error| {
            errors.push(format!("Trash could not be loaded: {}", error));
            Vec::new()
        });
        let automations = load_automations().unwrap_or_else(|error| {
            errors.push(format!("Automations could not be loaded: {}", error));
            Vec::new()
        });
        let automation_trash = list_automation_trash().unwrap_or_else(|error| {
            errors.push(format!("Automation Trash could not be loaded: {}", error));
            Vec::new()
        });
        let timed = list_timed_automations().unwrap_or_else(|error| {
            errors.push(format!("Timed automations could not be loaded: {}", error));
            Vec::new()
        });
        let sun_location = load_sun_location().unwrap_or_else(|error| {
            errors.push(format!(
                "Sunrise/sunset region could not be loaded: {}",
                error
            ));
            default_sun_location()
        });
        let settings = load_tui_settings().unwrap_or_else(|error| {
            errors.push(format!("Settings could not be loaded: {}", error));
            default_tui_settings()
        });

        let mut app = Self {
            tab: Tab::Aliases,
            quit: false,
            theme: Theme::from_setting(&settings.theme),
            alias_state,
            alias_trash,
            automations,
            automation_trash,
            timed,
            sun_location,
            settings,
            alias_table: TableState::default(),
            alias_search: TextInput::default(),
            alias_search_active: false,
            alias_filter: 0,
            automation_table: TableState::default(),
            automation_search: TextInput::default(),
            automation_search_active: false,
            automation_filter: "all".to_string(),
            settings_selected: 0,
            modal: None,
            message: None,
            run: None,
            tick: 0,
        };
        app.alias_table.select(Some(0));
        app.automation_table.select(Some(0));
        if let Some(error) = errors.pop() {
            app.error(error);
        }
        // Like the desktop app, offer the one-time shell import on first start.
        if !app.alias_state.import_candidates.is_empty() {
            let checked = app
                .alias_state
                .import_candidates
                .iter()
                .map(|c| c.id.clone())
                .collect();
            app.modal = Some(Modal::ShellImport {
                selected: 0,
                checked,
            });
        }
        app
    }

    // ----- messages -----------------------------------------------------

    pub(crate) fn notice(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
            at: Instant::now(),
        });
    }

    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
            at: Instant::now(),
        });
    }

    pub(crate) fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if let Some(message) = &self.message {
            let lifetime = if message.error {
                Duration::from_secs(8)
            } else {
                Duration::from_secs(4)
            };
            if message.at.elapsed() > lifetime {
                self.message = None;
            }
        }
        let mut finished = None;
        if let Some(run) = &mut self.run {
            let was_running = run.running;
            run.poll();
            if was_running && !run.running {
                finished = Some((run.name.clone(), run.succeeded(), run.error.clone()));
            }
        }
        if let Some((name, ok, error)) = finished {
            if !matches!(self.modal, Some(Modal::Run)) {
                if ok {
                    self.notice(format!("\"{}\" finished.", name));
                } else {
                    self.error(format!("\"{}\": {}", name, error));
                }
            }
        }
    }

    // ----- alias list ---------------------------------------------------

    pub(crate) fn alias_filter_key(&self) -> &'static str {
        ALIAS_FILTERS[self.alias_filter].0
    }

    // Indices into alias_state.aliases, sorted and filtered like the GUI list.
    pub(crate) fn visible_aliases(&self) -> Vec<usize> {
        let query = self.alias_search.value.trim().to_lowercase();
        let filter = self.alias_filter_key();
        let mut indices: Vec<usize> = (0..self.alias_state.aliases.len())
            .filter(|&index| {
                let alias = &self.alias_state.aliases[index];
                alias_matches_filter(alias, filter)
                    && (query.is_empty()
                        || alias.name.to_lowercase().contains(&query)
                        || alias.command_preview.to_lowercase().contains(&query))
            })
            .collect();
        indices.sort_by(|&a, &b| {
            compare_aliases(&self.alias_state.aliases[a], &self.alias_state.aliases[b])
        });
        indices
    }

    pub(crate) fn selected_alias(&self) -> Option<AliasEntry> {
        let visible = self.visible_aliases();
        let index = self.alias_table.selected()?;
        visible
            .get(index)
            .map(|&i| self.alias_state.aliases[i].clone())
    }

    pub(crate) fn clamp_alias_selection(&mut self) {
        let len = self.visible_aliases().len();
        let selected = self.alias_table.selected().unwrap_or(0);
        self.alias_table
            .select(Some(if len == 0 { 0 } else { selected.min(len - 1) }));
    }

    fn select_alias_by_id(&mut self, id: &str) {
        if let Some(position) = self
            .visible_aliases()
            .iter()
            .position(|&index| self.alias_state.aliases[index].id == id)
        {
            self.alias_table.select(Some(position));
        } else {
            self.clamp_alias_selection();
        }
    }

    fn persist_aliases(&mut self, aliases: Vec<AliasEntry>) -> Result<(), String> {
        let state = save_aliases(aliases)?;
        self.alias_state = AppState {
            import_candidates: Vec::new(),
            ..state
        };
        Ok(())
    }

    pub(crate) fn open_alias_form(&mut self, alias: Option<&AliasEntry>) {
        let form = match alias {
            Some(alias) => AliasForm {
                editing_id: Some(alias.id.clone()),
                name: TextInput::new(&alias.name),
                action: ACTIONS.iter().position(|a| *a == alias.action).unwrap_or(0),
                path: TextInput::new(&alias.path),
                custom: TextInput::new(alias.custom_command.as_deref().unwrap_or("")),
                focus: 0,
                error: String::new(),
                completions: Vec::new(),
            },
            None => AliasForm {
                editing_id: None,
                name: TextInput::default(),
                action: 0,
                path: TextInput::default(),
                custom: TextInput::default(),
                focus: 0,
                error: String::new(),
                completions: Vec::new(),
            },
        };
        self.modal = Some(Modal::AliasForm(form));
    }

    // Returns Err(message) to keep the form open with that message.
    pub(crate) fn submit_alias_form(&mut self, form: &AliasForm) -> Result<(), String> {
        let (name, action) = (form.name.value.trim().to_string(), form.action());
        if let Some(error) =
            validate_alias_form(&name, action, &form.path.value, &form.custom.value)
        {
            return Err(error);
        }
        let duplicate = self
            .alias_state
            .aliases
            .iter()
            .any(|alias| alias.name == name && Some(&alias.id) != form.editing_id.as_ref());
        if duplicate {
            return Err(format!("Alias \"{}\" already exists.", name));
        }

        let timestamp = now_iso();
        let custom_command = if action == "custom" {
            Some(form.custom.value.trim().to_string())
        } else {
            None
        };
        let preview = form.preview();
        let mut aliases = self.alias_state.aliases.clone();
        let id = match &form.editing_id {
            Some(id) => {
                let existing = aliases
                    .iter_mut()
                    .find(|alias| &alias.id == id)
                    .ok_or_else(|| "This alias no longer exists.".to_string())?;
                existing.name = name.clone();
                existing.path = form.path.value.trim().to_string();
                existing.action = action.to_string();
                existing.custom_command = custom_command;
                existing.command_preview = preview;
                existing.updated_at = timestamp;
                id.clone()
            }
            None => {
                let id = create_id();
                aliases.push(AliasEntry {
                    id: id.clone(),
                    name: name.clone(),
                    path: form.path.value.trim().to_string(),
                    action: action.to_string(),
                    custom_command,
                    command_preview: preview,
                    favorite: false,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                });
                id
            }
        };
        self.persist_aliases(aliases)?;
        self.select_alias_by_id(&id);
        self.notice(format!("Saved: {}", self.alias_state.aliases_file));
        Ok(())
    }

    pub(crate) fn toggle_alias_favorite(&mut self) {
        let Some(alias) = self.selected_alias() else {
            return;
        };
        let mut aliases = self.alias_state.aliases.clone();
        if let Some(entry) = aliases.iter_mut().find(|entry| entry.id == alias.id) {
            entry.favorite = !entry.favorite;
            entry.updated_at = now_iso();
        }
        match self.persist_aliases(aliases) {
            Ok(()) => self.select_alias_by_id(&alias.id),
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn delete_selected_alias(&mut self) {
        let Some(alias) = self.selected_alias() else {
            return;
        };
        match move_alias_to_trash(alias.id.clone()) {
            Ok(result) => {
                self.alias_state = AppState {
                    import_candidates: Vec::new(),
                    ..result.state
                };
                self.alias_trash = result.trash;
                self.clamp_alias_selection();
                self.notice(format!("Alias \"{}\" moved to Trash.", alias.name));
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn use_suggestion(&mut self, index: usize) {
        let Some(suggestion) = SUGGESTIONS.get(index) else {
            return;
        };
        if self
            .alias_state
            .aliases
            .iter()
            .any(|alias| alias.name == suggestion.name)
        {
            self.error(format!("Alias \"{}\" already exists.", suggestion.name));
            return;
        }
        let timestamp = now_iso();
        let id = create_id();
        let mut aliases = self.alias_state.aliases.clone();
        aliases.push(AliasEntry {
            id: id.clone(),
            name: suggestion.name.to_string(),
            path: suggestion.path.to_string(),
            action: suggestion.action.to_string(),
            custom_command: if suggestion.action == "custom" {
                Some(suggestion.custom_command.to_string())
            } else {
                None
            },
            command_preview: build_command_preview(
                suggestion.action,
                suggestion.path,
                suggestion.custom_command,
            ),
            favorite: false,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        });
        match self.persist_aliases(aliases) {
            Ok(()) => {
                self.select_alias_by_id(&id);
                self.notice(format!("Alias \"{}\" added.", suggestion.name));
            }
            Err(error) => self.error(error),
        }
    }

    // ----- shell import -------------------------------------------------

    pub(crate) fn open_shell_import(&mut self) {
        match scan_shell_import() {
            Ok(state) => {
                let candidates = state.import_candidates.clone();
                self.alias_state = state;
                if candidates.is_empty() {
                    self.notice(format!(
                        "No new aliases found in {}.",
                        self.alias_state.shell_config_file
                    ));
                } else {
                    let checked = candidates.iter().map(|c| c.id.clone()).collect();
                    self.modal = Some(Modal::ShellImport {
                        selected: 0,
                        checked,
                    });
                }
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn import_shell(&mut self, checked: &HashSet<String>) -> Result<(), String> {
        if checked.is_empty() {
            return Err("Select at least one alias to import.".to_string());
        }
        let result = import_shell_aliases(checked.iter().cloned().collect(), now_iso())?;
        self.alias_state = AppState {
            import_candidates: Vec::new(),
            ..result.state
        };
        self.clamp_alias_selection();
        self.notice(format!(
            "{} aliases imported. Backup: {}",
            result.imported_count, result.backup_file
        ));
        Ok(())
    }

    pub(crate) fn dismiss_import(&mut self) {
        match dismiss_shell_import() {
            Ok(state) => {
                self.alias_state = AppState {
                    import_candidates: Vec::new(),
                    ..state
                };
                self.notice(format!(
                    "Existing aliases were left unchanged in {}.",
                    self.alias_state.shell_config_file
                ));
            }
            Err(error) => self.error(error),
        }
    }

    // ----- alias trash --------------------------------------------------

    pub(crate) fn open_alias_trash(&mut self) {
        match list_trash() {
            Ok(trash) => {
                self.alias_trash = trash;
                self.modal = Some(Modal::AliasTrash { selected: 0 });
            }
            Err(error) => self.error(format!("Trash could not be opened: {}", error)),
        }
    }

    pub(crate) fn restore_alias(&mut self, index: usize) {
        let Some(entry) = self.alias_trash.get(index).cloned() else {
            return;
        };
        match restore_trash_alias(entry.alias.id.clone()) {
            Ok(result) => {
                self.alias_state = AppState {
                    import_candidates: Vec::new(),
                    ..result.state
                };
                self.alias_trash = result.trash;
                self.clamp_alias_selection();
                self.notice(format!("Alias \"{}\" restored.", entry.alias.name));
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn delete_alias_forever(&mut self, id: &str) {
        let name = self
            .alias_trash
            .iter()
            .find(|e| e.alias.id == id)
            .map(|e| e.alias.name.clone());
        match permanently_delete_trash_alias(id.to_string()) {
            Ok(trash) => {
                self.alias_trash = trash;
                self.notice(format!(
                    "Alias \"{}\" permanently deleted.",
                    name.unwrap_or_default()
                ));
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn empty_alias_trash(&mut self) {
        match empty_trash() {
            Ok(trash) => {
                self.alias_trash = trash;
                self.notice("Trash emptied.");
            }
            Err(error) => self.error(error),
        }
    }

    // ----- backups ------------------------------------------------------

    pub(crate) fn open_backup(&mut self, kind: BackupKind, mode: BackupMode) {
        let mut modal = BackupModal {
            kind,
            mode,
            path: TextInput::new(if mode == BackupMode::Export { "" } else { "~/" }),
            items: Vec::new(),
            checked: HashSet::new(),
            selected: 0,
            focus_list: mode == BackupMode::Export,
            loaded_path: None,
            overwrite_armed: false,
            error: String::new(),
            completions: Vec::new(),
        };
        if mode == BackupMode::Export {
            modal.path.set(&default_backup_path(kind));
            modal.items = self.backup_items_from_current(kind);
            if modal.items.is_empty() {
                self.error(match kind {
                    BackupKind::Aliases => "There are no aliases to export yet.",
                    BackupKind::Automations => "There are no automations to export yet.",
                });
                return;
            }
            modal.checked = modal.items.iter().map(|item| item.id.clone()).collect();
        }
        self.modal = Some(Modal::Backup(modal));
    }

    fn backup_items_from_current(&self, kind: BackupKind) -> Vec<BackupItem> {
        match kind {
            BackupKind::Aliases => {
                let mut aliases = self.alias_state.aliases.clone();
                aliases.sort_by(compare_aliases);
                aliases.into_iter().map(alias_backup_item).collect()
            }
            BackupKind::Automations => {
                let mut automations = self.automations.clone();
                automations.sort_by(compare_automations);
                automations
                    .into_iter()
                    .map(automation_backup_item)
                    .collect()
            }
        }
    }

    pub(crate) fn inspect_backup(&mut self, modal: &mut BackupModal) {
        let path = expand_home(&modal.path.value).display().to_string();
        let items = match modal.kind {
            BackupKind::Aliases => inspect_alias_backup(path.clone()).map(|mut aliases| {
                aliases.sort_by(compare_aliases);
                aliases
                    .into_iter()
                    .map(alias_backup_item)
                    .collect::<Vec<_>>()
            }),
            BackupKind::Automations => {
                inspect_automation_backup(path.clone()).map(|mut automations| {
                    automations.sort_by(compare_automations);
                    automations
                        .into_iter()
                        .map(automation_backup_item)
                        .collect::<Vec<_>>()
                })
            }
        };
        match items {
            Ok(items) => {
                modal.checked = items.iter().map(|item| item.id.clone()).collect();
                modal.items = items;
                modal.selected = 0;
                modal.loaded_path = Some(path);
                modal.focus_list = true;
                modal.error.clear();
            }
            Err(error) => {
                modal.items.clear();
                modal.loaded_path = None;
                modal.error = format!("Backup could not be opened: {}", error);
            }
        }
    }

    // Ok(true) = done, close the modal.
    pub(crate) fn submit_backup(&mut self, modal: &mut BackupModal) -> bool {
        let noun = |count: usize| match (modal.kind, count) {
            (BackupKind::Aliases, 1) => "alias",
            (BackupKind::Aliases, _) => "aliases",
            (BackupKind::Automations, 1) => "automation",
            (BackupKind::Automations, _) => "automations",
        };
        if modal.mode == BackupMode::Import && modal.loaded_path.is_none() {
            self.inspect_backup(modal);
            return false;
        }
        if modal.checked.is_empty() {
            modal.error = format!(
                "Select at least one {} to {}.",
                if modal.kind == BackupKind::Aliases {
                    "alias"
                } else {
                    "automation"
                },
                if modal.mode == BackupMode::Export {
                    "export"
                } else {
                    "import"
                }
            );
            return false;
        }
        let ids: Vec<String> = modal
            .items
            .iter()
            .filter(|item| modal.checked.contains(&item.id))
            .map(|item| item.id.clone())
            .collect();

        match modal.mode {
            BackupMode::Export => {
                if modal.path.value.trim().is_empty() {
                    modal.error = "Choose where to save the backup.".to_string();
                    return false;
                }
                let destination = expand_home(&modal.path.value);
                if destination.is_dir() {
                    modal.error =
                        "That is a folder - add a file name like backup.json.".to_string();
                    return false;
                }
                if destination.exists() && !modal.overwrite_armed {
                    modal.overwrite_armed = true;
                    modal.error =
                        "That file already exists. Press Enter again to overwrite it.".to_string();
                    return false;
                }
                let destination = destination.display().to_string();
                let result = match modal.kind {
                    BackupKind::Aliases => export_alias_backup(ids, destination, now_iso()),
                    BackupKind::Automations => {
                        export_automation_backup(ids, destination, now_iso())
                    }
                };
                match result {
                    Ok(result) => {
                        self.notice(format!(
                            "{} {} exported to {}.",
                            result.exported_count,
                            noun(result.exported_count),
                            result.file
                        ));
                        true
                    }
                    Err(error) => {
                        modal.error = error;
                        false
                    }
                }
            }
            BackupMode::Import => {
                let path = modal.loaded_path.clone().unwrap_or_default();
                let result = match modal.kind {
                    BackupKind::Aliases => {
                        import_alias_backup(path, ids, now_iso()).map(|result| {
                            self.alias_state = AppState {
                                import_candidates: Vec::new(),
                                ..result.state
                            };
                            self.clamp_alias_selection();
                            (result.imported_count, result.replaced_count)
                        })
                    }
                    BackupKind::Automations => import_automation_backup_inner(path, ids, now_iso())
                        .map(|result| {
                            self.automations = result.automations;
                            self.clamp_automation_selection();
                            (result.imported_count, result.replaced_count)
                        }),
                };
                match result {
                    Ok((imported, replaced)) => {
                        let note = if replaced > 0 {
                            format!(" {} replaced.", replaced)
                        } else {
                            String::new()
                        };
                        self.notice(format!("{} {} imported.{}", imported, noun(imported), note));
                        true
                    }
                    Err(error) => {
                        modal.error = error;
                        false
                    }
                }
            }
        }
    }

    // ----- automations --------------------------------------------------

    pub(crate) fn automation_groups(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .automations
            .iter()
            .map(|automation| automation.group.trim().to_string())
            .filter(|group| !group.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names
    }

    pub(crate) fn automation_filter_label(&self) -> String {
        if let Some(group) = self.automation_filter.strip_prefix("group:") {
            return if group.is_empty() {
                "Ungrouped".to_string()
            } else {
                format!("Group: {}", group)
            };
        }
        AUTOMATION_STATIC_FILTERS
            .iter()
            .find(|(key, _)| *key == self.automation_filter)
            .map(|(_, label)| label.to_string())
            .unwrap_or_else(|| "All automations".to_string())
    }

    pub(crate) fn cycle_automation_filter(&mut self, forward: bool) {
        let mut all: Vec<String> = AUTOMATION_STATIC_FILTERS
            .iter()
            .map(|(key, _)| key.to_string())
            .collect();
        all.extend(
            self.automation_groups()
                .into_iter()
                .map(|group| format!("group:{}", group)),
        );
        let current = all
            .iter()
            .position(|key| *key == self.automation_filter)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % all.len()
        } else {
            (current + all.len() - 1) % all.len()
        };
        self.automation_filter = all[next].clone();
        self.automation_table.select(Some(0));
        self.skip_automation_headers(true);
    }

    pub(crate) fn automation_rows(&self) -> Vec<AutomationRow> {
        let query = self.automation_search.value.trim().to_lowercase();
        let mut indices: Vec<usize> = (0..self.automations.len())
            .filter(|&index| {
                let automation = &self.automations[index];
                automation_matches_filter(automation, &self.automation_filter)
                    && (query.is_empty()
                        || automation.name.to_lowercase().contains(&query)
                        || automation.path.to_lowercase().contains(&query)
                        || automation.group.to_lowercase().contains(&query)
                        || automation.steps.iter().any(|step| {
                            step.kind == "command" && step.command.to_lowercase().contains(&query)
                        }))
            })
            .collect();
        indices.sort_by(|&a, &b| compare_automations(&self.automations[a], &self.automations[b]));

        if self.automation_filter != "groups" {
            return indices.into_iter().map(AutomationRow::Item).collect();
        }
        let mut rows = Vec::new();
        let mut groups = self.automation_groups();
        groups.push(String::new());
        for group in groups {
            let members: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|&index| self.automations[index].group.trim() == group)
                .collect();
            if members.is_empty() {
                continue;
            }
            rows.push(AutomationRow::Header(
                if group.is_empty() {
                    "Ungrouped".to_string()
                } else {
                    group
                },
                members.len(),
            ));
            rows.extend(members.into_iter().map(AutomationRow::Item));
        }
        rows
    }

    pub(crate) fn selected_automation(&self) -> Option<Automation> {
        let rows = self.automation_rows();
        match rows.get(self.automation_table.selected()?) {
            Some(AutomationRow::Item(index)) => self.automations.get(*index).cloned(),
            _ => None,
        }
    }

    fn automation_item_positions(&self) -> Vec<usize> {
        self.automation_rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| matches!(row, AutomationRow::Item(_)))
            .map(|(position, _)| position)
            .collect()
    }

    // Group headers are not selectable: move onto the nearest automation row.
    pub(crate) fn skip_automation_headers(&mut self, forward: bool) {
        let items = self.automation_item_positions();
        let current = self.automation_table.selected().unwrap_or(0);
        let target = if forward {
            items
                .iter()
                .copied()
                .find(|&p| p >= current)
                .or_else(|| items.last().copied())
        } else {
            items
                .iter()
                .rev()
                .copied()
                .find(|&p| p <= current)
                .or_else(|| items.first().copied())
        };
        self.automation_table.select(Some(target.unwrap_or(0)));
    }

    pub(crate) fn move_automation_selection(&mut self, delta: isize) {
        let items = self.automation_item_positions();
        if items.is_empty() {
            return;
        }
        let current = self.automation_table.selected().unwrap_or(0);
        let index = items
            .iter()
            .position(|&p| p >= current)
            .unwrap_or(items.len() - 1) as isize;
        let next = (index + delta).clamp(0, items.len() as isize - 1) as usize;
        self.automation_table.select(Some(items[next]));
    }

    pub(crate) fn clamp_automation_selection(&mut self) {
        let len = self.automation_rows().len();
        let selected = self.automation_table.selected().unwrap_or(0);
        self.automation_table
            .select(Some(if len == 0 { 0 } else { selected.min(len - 1) }));
        self.skip_automation_headers(false);
    }

    fn select_automation_by_id(&mut self, id: &str) {
        let rows = self.automation_rows();
        if let Some(position) = rows.iter().position(|row| match row {
            AutomationRow::Item(index) => self.automations[*index].id == id,
            _ => false,
        }) {
            self.automation_table.select(Some(position));
        } else {
            self.clamp_automation_selection();
        }
    }

    pub(crate) fn schedule_for(&self, automation_id: &str) -> Option<&TimedAutomation> {
        self.timed
            .iter()
            .find(|entry| entry.automation_id == automation_id)
    }

    fn persist_automations(&mut self, next: Vec<Automation>) -> Result<(), String> {
        self.automations = save_automations(next)?;
        Ok(())
    }

    pub(crate) fn open_editor(&mut self, automation: Option<&Automation>) {
        let timestamp = now_iso();
        let (automation, is_new) = match automation {
            Some(existing) => (existing.clone(), false),
            None => (
                Automation {
                    id: create_id(),
                    name: String::new(),
                    path: "~/Projects".to_string(),
                    steps: vec![new_step("command")],
                    favorite: false,
                    group: String::new(),
                    hotkey: None,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                },
                true,
            ),
        };
        let step_inputs = automation.steps.iter().map(step_input).collect();
        self.modal = Some(Modal::Editor(EditorState {
            name: TextInput::new(&automation.name),
            path: TextInput::new(&automation.path),
            group: TextInput::new(&automation.group),
            step_inputs,
            automation,
            focus: 0,
            is_new,
            error: String::new(),
            completions: Vec::new(),
        }));
    }

    pub(crate) fn save_editor(&mut self, editor: &mut EditorState) -> Result<(), String> {
        let mut automation = editor.automation.clone();
        automation.name = editor.name.value.trim().to_string();
        automation.path = editor.path.value.trim().to_string();
        automation.group = editor.group.value.trim().to_string();
        for (step, input) in automation.steps.iter_mut().zip(editor.step_inputs.iter()) {
            if step.kind == "wait" {
                step.seconds = input.value.trim().parse::<u64>().unwrap_or(0);
                step.command = String::new();
            } else {
                step.command = input.value.trim().to_string();
                step.seconds = 0;
            }
        }
        // Same checks and messages as the desktop editor.
        if automation.name.is_empty() {
            return Err("Enter a name for the automation.".to_string());
        }
        if automation.path.is_empty() {
            return Err("Choose a working directory.".to_string());
        }
        if automation.group.chars().count() > 60 {
            return Err("The group label must be at most 60 characters.".to_string());
        }
        if automation.steps.is_empty() {
            return Err("Add at least one step.".to_string());
        }
        for (index, step) in automation.steps.iter().enumerate() {
            if step.kind == "command" && step.command.is_empty() {
                return Err(format!("Step {} needs a command.", index + 1));
            }
            if step.kind == "wait" && (step.seconds < 1 || step.seconds > MAX_WAIT_SECONDS) {
                return Err(format!(
                    "Step {} must wait between 1 second and 24 hours.",
                    index + 1
                ));
            }
        }
        let duplicate = self.automations.iter().any(|other| {
            other.id != automation.id
                && other.name.trim().to_lowercase() == automation.name.to_lowercase()
        });
        if duplicate {
            return Err(format!(
                "Automation \"{}\" already exists.",
                automation.name
            ));
        }
        automation.updated_at = now_iso();

        let mut next = self.automations.clone();
        match next.iter_mut().find(|item| item.id == automation.id) {
            Some(existing) => *existing = automation.clone(),
            None => next.push(automation.clone()),
        }
        self.persist_automations(next)?;
        self.select_automation_by_id(&automation.id);
        self.notice(format!("Automation \"{}\" saved.", automation.name));
        Ok(())
    }

    pub(crate) fn toggle_automation_favorite(&mut self) {
        let Some(automation) = self.selected_automation() else {
            return;
        };
        let next = self
            .automations
            .iter()
            .cloned()
            .map(|mut item| {
                if item.id == automation.id {
                    item.favorite = !item.favorite;
                    item.updated_at = now_iso();
                }
                item
            })
            .collect();
        match self.persist_automations(next) {
            Ok(()) => self.select_automation_by_id(&automation.id),
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn assign_group(&mut self, id: &str, group: &str) -> Result<(), String> {
        let group = group.trim().to_string();
        if group.chars().count() > 60 {
            return Err("The group label must be at most 60 characters.".to_string());
        }
        let name = self
            .automations
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let next = self
            .automations
            .iter()
            .cloned()
            .map(|mut item| {
                if item.id == id {
                    item.group = group.clone();
                    item.updated_at = now_iso();
                }
                item
            })
            .collect();
        self.persist_automations(next)?;
        self.select_automation_by_id(id);
        if group.is_empty() {
            self.notice(format!("Removed \"{}\" from its group.", name));
        } else {
            self.notice(format!("Moved \"{}\" to \"{}\".", name, group));
        }
        Ok(())
    }

    pub(crate) fn delete_automation(&mut self, id: &str) {
        match move_automation_to_trash_inner(id) {
            Ok(result) => {
                let name = result
                    .trash
                    .iter()
                    .find(|entry| entry.automation.id == id)
                    .map(|entry| entry.automation.name.clone())
                    .unwrap_or_default();
                self.automations = result.automations;
                self.automation_trash = result.trash;
                self.clamp_automation_selection();
                self.notice(format!("Automation \"{}\" moved to Trash.", name));
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn open_automation_trash(&mut self) {
        match list_automation_trash() {
            Ok(trash) => {
                self.automation_trash = trash;
                self.modal = Some(Modal::AutomationTrash { selected: 0 });
            }
            Err(error) => self.error(format!("Automation Trash could not be opened: {}", error)),
        }
    }

    pub(crate) fn restore_automation(&mut self, index: usize) {
        let Some(entry) = self.automation_trash.get(index).cloned() else {
            return;
        };
        match restore_trash_automation_inner(&entry.automation.id) {
            Ok(result) => {
                self.automations = result.automations;
                self.automation_trash = result.trash;
                self.clamp_automation_selection();
                self.notice(format!(
                    "Automation \"{}\" restored.",
                    entry.automation.name
                ));
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn delete_automation_forever(&mut self, id: &str) {
        let name = self
            .automation_trash
            .iter()
            .find(|entry| entry.automation.id == id)
            .map(|entry| entry.automation.name.clone())
            .unwrap_or_default();
        match permanently_delete_trash_automation(id.to_string()) {
            Ok(trash) => {
                self.automation_trash = trash;
                self.notice(format!("Automation \"{}\" permanently deleted.", name));
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn empty_automation_trash_now(&mut self) {
        match empty_automation_trash() {
            Ok(trash) => {
                self.automation_trash = trash;
                self.notice("Trash emptied.");
            }
            Err(error) => self.error(error),
        }
    }

    // ----- runs ---------------------------------------------------------

    pub(crate) fn run_selected_automation(&mut self) {
        let Some(automation) = self.selected_automation() else {
            return;
        };
        if self.run.as_ref().map(|run| run.running).unwrap_or(false) {
            self.modal = Some(Modal::Run);
            self.error("Another automation is still running. Stop it first.");
            return;
        }
        self.run = Some(RunState::start(&automation));
        self.modal = Some(Modal::Run);
    }

    // ----- schedules ----------------------------------------------------

    pub(crate) fn open_schedule(&mut self) {
        let Some(automation) = self.selected_automation() else {
            return;
        };
        let existing = self.schedule_for(&automation.id).cloned();
        let timestamp = now_iso();
        let exists = existing.is_some();
        let entry = existing.unwrap_or(TimedAutomation {
            id: create_id(),
            automation_id: automation.id.clone(),
            trigger_kind: "clock".to_string(),
            time: "09:00".to_string(),
            days: Vec::new(),
            enabled: true,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            last_run_at: None,
            last_run_status: None,
            last_run_output: None,
            last_triggered_date: None,
        });
        self.modal = Some(Modal::Schedule(ScheduleState {
            time: TextInput::new(&entry.time),
            entry,
            automation_name: automation.name.clone(),
            exists,
            focus: 0,
            day_cursor: 0,
            error: String::new(),
        }));
    }

    pub(crate) fn save_schedule(&mut self, state: &mut ScheduleState) -> Result<(), String> {
        let mut entry = state.entry.clone();
        if entry.trigger_kind == "clock" {
            let time = state.time.value.trim().to_string();
            if time.is_empty() {
                return Err("Choose a time.".to_string());
            }
            let (hour, minute) = parse_time_of_day(&time)?;
            entry.time = format!("{:02}:{:02}", hour, minute);
        } else if self.sun_location.region.is_empty() {
            return Err("Choose a region for sunrise/sunset scheduling.".to_string());
        }
        entry.updated_at = now_iso();
        self.timed = save_timed_automation(entry)?;
        self.notice("Schedule saved.");
        Ok(())
    }

    pub(crate) fn remove_schedule(&mut self, automation_id: &str) {
        let Some(entry) = self.schedule_for(automation_id).cloned() else {
            return;
        };
        match delete_timed_automation(entry.id) {
            Ok(timed) => {
                self.timed = timed;
                self.notice("Schedule removed.");
            }
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn set_sun_region(&mut self, forward: bool) {
        let current = SUN_REGIONS
            .iter()
            .position(|(key, ..)| *key == self.sun_location.region)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % SUN_REGIONS.len()
        } else {
            (current + SUN_REGIONS.len() - 1) % SUN_REGIONS.len()
        };
        let setting = SunLocationSetting {
            region: SUN_REGIONS[next].0.to_string(),
        };
        match save_sun_location(setting) {
            Ok(setting) => self.sun_location = setting,
            Err(error) => self.error(error),
        }
    }

    // ----- settings -----------------------------------------------------

    pub(crate) fn save_settings_now(&mut self) {
        match write_tui_settings(&self.settings) {
            Ok(()) => self.theme = Theme::from_setting(&self.settings.theme),
            Err(error) => self.error(format!("Settings could not be saved: {}", error)),
        }
    }

    pub(crate) fn cycle_theme(&mut self, forward: bool) {
        let current = THEME_VALUES
            .iter()
            .position(|t| *t == self.settings.theme)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % 3
        } else {
            (current + 2) % 3
        };
        self.settings.theme = THEME_VALUES[next].to_string();
        self.save_settings_now();
    }

    pub(crate) fn toggle_suggestions_setting(&mut self) {
        self.settings.show_suggestions = !self.settings.show_suggestions;
        self.save_settings_now();
    }

    pub(crate) fn open_url(&mut self, url: &str) {
        match Command::new("open").arg(url).output() {
            Ok(output) if output.status.success() => self.notice(format!("Opened {}", url)),
            _ => self.error(format!("Link could not be opened: {}", url)),
        }
    }
}

pub(crate) fn new_step(kind: &str) -> AutomationStep {
    AutomationStep {
        id: create_id(),
        kind: kind.to_string(),
        command: String::new(),
        seconds: if kind == "wait" { 10 } else { 0 },
        behavior: "wait".to_string(),
    }
}

pub(crate) fn step_input(step: &AutomationStep) -> TextInput {
    if step.kind == "wait" {
        TextInput::new(&step.seconds.to_string())
    } else {
        TextInput::new(&step.command)
    }
}

fn alias_backup_item(alias: AliasEntry) -> BackupItem {
    BackupItem {
        label: format!("{}{}", if alias.favorite { "★ " } else { "" }, alias.name),
        detail: alias.command_preview.clone(),
        id: alias.id,
    }
}

fn automation_backup_item(automation: Automation) -> BackupItem {
    let steps = automation.steps.len();
    BackupItem {
        label: format!(
            "{}{}",
            if automation.favorite { "★ " } else { "" },
            automation.name
        ),
        detail: format!(
            "{} step{}{}",
            steps,
            if steps == 1 { "" } else { "s" },
            if automation.group.trim().is_empty() {
                String::new()
            } else {
                format!(" · {}", automation.group.trim())
            }
        ),
        id: automation.id,
    }
}
