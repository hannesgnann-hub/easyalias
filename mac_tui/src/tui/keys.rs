//! Key handling for the tabs and every modal.

use super::app::*;
use super::input::TextInput;
use crate::suggestions::SUGGESTIONS;
use crate::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) const SETTINGS_ROWS: usize = 7;
pub(crate) const LINKS: [(&str, &str); 4] = [
    (
        "GitHub repository",
        "https://github.com/hannesgnann-hub/easyalias",
    ),
    ("Website", "https://easyalias.org"),
    (
        "Become a GitHub Sponsor",
        "https://github.com/sponsors/hannesgnann-hub",
    ),
    (
        "r/easyalias on Reddit",
        "https://www.reddit.com/r/easyalias/",
    ),
];

fn ctrl(key: &KeyEvent, c: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(c)
}

fn plain(key: &KeyEvent) -> bool {
    !key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

fn is_up(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Up) || (plain(key) && key.code == KeyCode::Char('k'))
}

fn is_down(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Down) || (plain(key) && key.code == KeyCode::Char('j'))
}

fn move_index(index: usize, len: usize, key: &KeyEvent) -> usize {
    if len == 0 {
        return 0;
    }
    match key.code {
        KeyCode::PageUp => index.saturating_sub(10),
        KeyCode::PageDown => (index + 10).min(len - 1),
        KeyCode::Home => 0,
        KeyCode::End => len - 1,
        _ if is_up(key) => index.saturating_sub(1),
        _ if is_down(key) => (index + 1).min(len - 1),
        _ => index,
    }
}

fn is_nav(key: &KeyEvent) -> bool {
    is_up(key)
        || is_down(key)
        || matches!(
            key.code,
            KeyCode::PageUp | KeyCode::PageDown | KeyCode::Home | KeyCode::End
        )
}

impl App {
    pub(crate) fn on_paste(&mut self, text: &str) {
        let text = text.replace(['\r', '\n'], " ");
        let input: Option<&mut TextInput> = match &mut self.modal {
            Some(Modal::AliasForm(form)) => match form.focus {
                0 => Some(&mut form.name),
                2 if form.action() == "custom" => Some(&mut form.custom),
                2 => Some(&mut form.path),
                _ => None,
            },
            Some(Modal::Editor(editor)) => match editor.focus {
                0 => Some(&mut editor.name),
                1 => Some(&mut editor.path),
                2 => Some(&mut editor.group),
                n => editor.step_inputs.get_mut(n - EDITOR_FIXED_FIELDS),
            },
            Some(Modal::Backup(modal)) if !modal.focus_list => Some(&mut modal.path),
            Some(Modal::Group { input, .. }) => Some(input),
            Some(Modal::Schedule(state)) if state.focus == 1 => Some(&mut state.time),
            None if self.tab == Tab::Aliases && self.alias_search_active => {
                Some(&mut self.alias_search)
            }
            None if self.tab == Tab::Automations && self.automation_search_active => {
                Some(&mut self.automation_search)
            }
            _ => None,
        };
        if let Some(input) = input {
            input.insert_str(&text);
        }
    }

    pub(crate) fn request_quit(&mut self) {
        if self.run.as_ref().map(|run| run.running).unwrap_or(false) {
            self.modal = Some(Modal::Confirm {
                text: "An automation is still running. Stop it and quit?".to_string(),
                action: ConfirmAction::Quit,
                back: None,
            });
        } else {
            self.quit = true;
        }
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) {
        if ctrl(&key, 'c') {
            if let Some(run) = &mut self.run {
                run.stop();
            }
            self.quit = true;
            return;
        }
        if let Some(modal) = self.modal.take() {
            let next = self.modal_key(modal, key);
            if self.modal.is_none() {
                self.modal = next;
            }
            return;
        }
        match self.tab {
            Tab::Aliases if self.alias_search_active => self.search_key(key, true),
            Tab::Automations if self.automation_search_active => self.search_key(key, false),
            _ => self.tab_key(key),
        }
    }

    fn search_key(&mut self, key: KeyEvent, aliases: bool) {
        match key.code {
            KeyCode::Esc => {
                if aliases {
                    self.alias_search.clear();
                    self.alias_search_active = false;
                } else {
                    self.automation_search.clear();
                    self.automation_search_active = false;
                }
            }
            KeyCode::Enter => {
                if aliases {
                    self.alias_search_active = false;
                } else {
                    self.automation_search_active = false;
                }
            }
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                if aliases {
                    let len = self.visible_aliases().len();
                    let next = move_index(self.alias_table.selected().unwrap_or(0), len, &key);
                    self.alias_table.select(Some(next));
                } else {
                    let delta = match key.code {
                        KeyCode::Up => -1,
                        KeyCode::Down => 1,
                        KeyCode::PageUp => -10,
                        _ => 10,
                    };
                    self.move_automation_selection(delta);
                }
            }
            _ => {
                let input = if aliases {
                    &mut self.alias_search
                } else {
                    &mut self.automation_search
                };
                if input.handle_key(key) {
                    if aliases {
                        self.alias_table.select(Some(0));
                    } else {
                        self.automation_table.select(Some(0));
                        self.skip_automation_headers(true);
                    }
                }
            }
        }
    }

    fn tab_key(&mut self, key: KeyEvent) {
        // Global keys.
        match key.code {
            KeyCode::Char('q') if plain(&key) => return self.request_quit(),
            KeyCode::Char('?') => {
                let topic = match self.tab {
                    Tab::Automations => 1,
                    _ => 0,
                };
                self.modal = Some(Modal::Help { topic, step: 0 });
                return;
            }
            KeyCode::Char('1') => return self.tab = Tab::Aliases,
            KeyCode::Char('2') => return self.tab = Tab::Automations,
            KeyCode::Char('3') => return self.tab = Tab::Settings,
            KeyCode::Tab => {
                self.tab = match self.tab {
                    Tab::Aliases => Tab::Automations,
                    Tab::Automations => Tab::Settings,
                    Tab::Settings => Tab::Aliases,
                };
                return;
            }
            KeyCode::BackTab => {
                self.tab = match self.tab {
                    Tab::Aliases => Tab::Settings,
                    Tab::Automations => Tab::Aliases,
                    Tab::Settings => Tab::Automations,
                };
                return;
            }
            _ => {}
        }
        match self.tab {
            Tab::Aliases => self.aliases_key(key),
            Tab::Automations => self.automations_key(key),
            Tab::Settings => self.settings_key(key),
        }
    }

    fn aliases_key(&mut self, key: KeyEvent) {
        if is_nav(&key) {
            let len = self.visible_aliases().len();
            let next = move_index(self.alias_table.selected().unwrap_or(0), len, &key);
            self.alias_table.select(Some(next));
            return;
        }
        match key.code {
            KeyCode::Char('/') => self.alias_search_active = true,
            KeyCode::Char('f') => {
                self.alias_filter = (self.alias_filter + 1) % ALIAS_FILTERS.len();
                self.alias_table.select(Some(0));
            }
            KeyCode::Char('F') => {
                self.alias_filter =
                    (self.alias_filter + ALIAS_FILTERS.len() - 1) % ALIAS_FILTERS.len();
                self.alias_table.select(Some(0));
            }
            KeyCode::Char('n') | KeyCode::Char('a') => self.open_alias_form(None),
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(alias) = self.selected_alias() {
                    self.open_alias_form(Some(&alias));
                }
            }
            KeyCode::Char(' ') | KeyCode::Char('*') => self.toggle_alias_favorite(),
            KeyCode::Char('d') | KeyCode::Delete => self.delete_selected_alias(),
            KeyCode::Char('s') => {
                if self.settings.show_suggestions {
                    self.modal = Some(Modal::Suggestions { selected: 0 });
                } else {
                    self.error("Alias suggestions are turned off. Turn them on in Settings (3).");
                }
            }
            KeyCode::Char('i') => self.open_shell_import(),
            KeyCode::Char('b') => self.open_backup(BackupKind::Aliases, BackupMode::Export),
            KeyCode::Char('B') => self.open_backup(BackupKind::Aliases, BackupMode::Import),
            KeyCode::Char('t') => self.open_alias_trash(),
            KeyCode::Esc => {
                self.alias_search.clear();
                self.alias_filter = 0;
            }
            _ => {}
        }
    }

    fn automations_key(&mut self, key: KeyEvent) {
        if is_nav(&key) {
            let delta = match key.code {
                KeyCode::PageUp => -10,
                KeyCode::PageDown => 10,
                KeyCode::Home => -10_000,
                KeyCode::End => 10_000,
                _ if is_up(&key) => -1,
                _ => 1,
            };
            self.move_automation_selection(delta);
            return;
        }
        match key.code {
            KeyCode::Char('/') => self.automation_search_active = true,
            KeyCode::Char('f') => self.cycle_automation_filter(true),
            KeyCode::Char('F') => self.cycle_automation_filter(false),
            KeyCode::Char('n') | KeyCode::Char('a') => self.open_editor(None),
            KeyCode::Char('e') => {
                if let Some(automation) = self.selected_automation() {
                    self.open_editor(Some(&automation));
                }
            }
            KeyCode::Enter | KeyCode::Char('r') => self.run_selected_automation(),
            KeyCode::Char('o') => {
                if self.run.is_some() {
                    self.modal = Some(Modal::Run);
                } else {
                    self.error("No automation has run yet in this session.");
                }
            }
            KeyCode::Char(' ') | KeyCode::Char('*') => self.toggle_automation_favorite(),
            KeyCode::Char('g') => {
                if let Some(automation) = self.selected_automation() {
                    self.modal = Some(Modal::Group {
                        automation_id: automation.id.clone(),
                        input: TextInput::new(&automation.group),
                        selected: None,
                    });
                }
            }
            KeyCode::Char('c') => self.open_schedule(),
            KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                if let Some(automation) = self.selected_automation() {
                    self.modal = Some(Modal::Confirm {
                        text: format!("Move automation \"{}\" to Trash?", automation.name),
                        action: ConfirmAction::DeleteAutomation(automation.id),
                        back: None,
                    });
                }
            }
            KeyCode::Char('b') => self.open_backup(BackupKind::Automations, BackupMode::Export),
            KeyCode::Char('B') => self.open_backup(BackupKind::Automations, BackupMode::Import),
            KeyCode::Char('t') => self.open_automation_trash(),
            KeyCode::Esc => {
                self.automation_search.clear();
                self.automation_filter = "all".to_string();
                self.clamp_automation_selection();
            }
            _ => {}
        }
    }

    fn settings_key(&mut self, key: KeyEvent) {
        if is_up(&key) || is_down(&key) {
            self.settings_selected = move_index(self.settings_selected, SETTINGS_ROWS, &key);
            return;
        }
        let change = matches!(
            key.code,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Enter
                | KeyCode::Char(' ')
                | KeyCode::Char('h')
                | KeyCode::Char('l')
        );
        if !change {
            return;
        }
        let forward = !matches!(key.code, KeyCode::Left | KeyCode::Char('h'));
        match self.settings_selected {
            0 => self.cycle_theme(forward),
            1 => self.toggle_suggestions_setting(),
            2 => self.set_sun_region(forward),
            row => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    let (_, url) = LINKS[row - 3];
                    self.open_url(url);
                }
            }
        }
    }

    // ----- modals -------------------------------------------------------

    fn modal_key(&mut self, modal: Modal, key: KeyEvent) -> Option<Modal> {
        match modal {
            Modal::AliasForm(form) => self.alias_form_key(form, key),
            Modal::Suggestions { selected } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return None,
                    KeyCode::Enter | KeyCode::Char(' ') => self.use_suggestion(selected),
                    _ => {}
                }
                Some(Modal::Suggestions {
                    selected: move_index(selected, SUGGESTIONS.len(), &key),
                })
            }
            Modal::ShellImport {
                selected,
                mut checked,
            } => {
                let candidates = self.alias_state.import_candidates.clone();
                match key.code {
                    KeyCode::Esc => return None,
                    KeyCode::Char(' ') => {
                        if let Some(candidate) = candidates.get(selected) {
                            if !checked.remove(&candidate.id) {
                                checked.insert(candidate.id.clone());
                            }
                        }
                    }
                    KeyCode::Char('a') => {
                        if checked.len() == candidates.len() {
                            checked.clear();
                        } else {
                            checked = candidates.iter().map(|c| c.id.clone()).collect();
                        }
                    }
                    KeyCode::Enter => match self.import_shell(&checked) {
                        Ok(()) => return None,
                        Err(error) => self.error(error),
                    },
                    KeyCode::Char('d') => {
                        self.dismiss_import();
                        return None;
                    }
                    _ => {}
                }
                Some(Modal::ShellImport {
                    selected: move_index(selected, candidates.len(), &key),
                    checked,
                })
            }
            Modal::AliasTrash { selected } => {
                let len = self.alias_trash.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return None,
                    KeyCode::Enter | KeyCode::Char('r') => self.restore_alias(selected),
                    KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                        if let Some(entry) = self.alias_trash.get(selected) {
                            return Some(Modal::Confirm {
                                text: format!(
                                    "Permanently delete alias \"{}\"? This cannot be undone.",
                                    entry.alias.name
                                ),
                                action: ConfirmAction::DeleteAliasForever(entry.alias.id.clone()),
                                back: Some(Box::new(Modal::AliasTrash { selected })),
                            });
                        }
                    }
                    KeyCode::Char('E') if len > 0 => {
                        return Some(Modal::Confirm {
                            text: format!("Permanently delete all {} aliases in Trash? This cannot be undone.", len),
                            action: ConfirmAction::EmptyAliasTrash,
                            back: Some(Box::new(Modal::AliasTrash { selected: 0 })),
                        });
                    }
                    _ => {}
                }
                let len = self.alias_trash.len();
                Some(Modal::AliasTrash {
                    selected: move_index(selected.min(len.saturating_sub(1)), len, &key),
                })
            }
            Modal::AutomationTrash { selected } => {
                let len = self.automation_trash.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return None,
                    KeyCode::Enter | KeyCode::Char('r') => self.restore_automation(selected),
                    KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                        if let Some(entry) = self.automation_trash.get(selected) {
                            return Some(Modal::Confirm {
                                text: format!(
                                    "Permanently delete automation \"{}\"? This cannot be undone.",
                                    entry.automation.name
                                ),
                                action: ConfirmAction::DeleteAutomationForever(
                                    entry.automation.id.clone(),
                                ),
                                back: Some(Box::new(Modal::AutomationTrash { selected })),
                            });
                        }
                    }
                    KeyCode::Char('E') if len > 0 => {
                        return Some(Modal::Confirm {
                            text: format!(
                                "Permanently delete all {} automation{} in Trash? This cannot be undone.",
                                len,
                                if len == 1 { "" } else { "s" }
                            ),
                            action: ConfirmAction::EmptyAutomationTrash,
                            back: Some(Box::new(Modal::AutomationTrash { selected: 0 })),
                        });
                    }
                    _ => {}
                }
                let len = self.automation_trash.len();
                Some(Modal::AutomationTrash {
                    selected: move_index(selected.min(len.saturating_sub(1)), len, &key),
                })
            }
            Modal::Backup(modal) => self.backup_key(modal, key),
            Modal::Editor(editor) => self.editor_key(editor, key),
            Modal::Group {
                automation_id,
                mut input,
                selected,
            } => {
                let groups = self.automation_groups();
                match key.code {
                    KeyCode::Esc => return None,
                    KeyCode::Enter => match self.assign_group(&automation_id, &input.value) {
                        Ok(()) => return None,
                        Err(error) => self.error(error),
                    },
                    KeyCode::Up | KeyCode::Down if !groups.is_empty() => {
                        let next = match (selected, key.code) {
                            (None, KeyCode::Down) => 0,
                            (None, _) => groups.len() - 1,
                            (Some(i), KeyCode::Down) => (i + 1) % groups.len(),
                            (Some(i), _) => (i + groups.len() - 1) % groups.len(),
                        };
                        input.set(&groups[next]);
                        return Some(Modal::Group {
                            automation_id,
                            input,
                            selected: Some(next),
                        });
                    }
                    _ => {
                        input.handle_key(key);
                    }
                }
                Some(Modal::Group {
                    automation_id,
                    input,
                    selected,
                })
            }
            Modal::Schedule(state) => self.schedule_key(state, key),
            Modal::Run => self.run_key(key),
            Modal::Confirm { text, action, back } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    let back = back.map(|modal| *modal);
                    match action {
                        ConfirmAction::DeleteAutomation(id) => self.delete_automation(&id),
                        ConfirmAction::DeleteAliasForever(id) => self.delete_alias_forever(&id),
                        ConfirmAction::EmptyAliasTrash => self.empty_alias_trash(),
                        ConfirmAction::DeleteAutomationForever(id) => {
                            self.delete_automation_forever(&id)
                        }
                        ConfirmAction::EmptyAutomationTrash => self.empty_automation_trash_now(),
                        ConfirmAction::RemoveSchedule(id) => {
                            self.remove_schedule(&id);
                            return None;
                        }
                        ConfirmAction::Quit => {
                            if let Some(run) = &mut self.run {
                                run.stop();
                            }
                            self.quit = true;
                            return None;
                        }
                    }
                    back.map(|modal| match modal {
                        Modal::AliasTrash { selected } => Modal::AliasTrash {
                            selected: selected.min(self.alias_trash.len().saturating_sub(1)),
                        },
                        Modal::AutomationTrash { selected } => Modal::AutomationTrash {
                            selected: selected.min(self.automation_trash.len().saturating_sub(1)),
                        },
                        other => other,
                    })
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => back.map(|modal| *modal),
                _ => Some(Modal::Confirm { text, action, back }),
            },
            Modal::Help { topic, step } => {
                let topics = super::help::TOPICS;
                let steps = topics[topic].steps.len();
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => None,
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char(' ') => {
                        if step + 1 < steps {
                            Some(Modal::Help {
                                topic,
                                step: step + 1,
                            })
                        } else if topic + 1 < topics.len() {
                            Some(Modal::Help {
                                topic: topic + 1,
                                step: 0,
                            })
                        } else {
                            None
                        }
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        if step > 0 {
                            Some(Modal::Help {
                                topic,
                                step: step - 1,
                            })
                        } else if topic > 0 {
                            Some(Modal::Help {
                                topic: topic - 1,
                                step: topics[topic - 1].steps.len() - 1,
                            })
                        } else {
                            Some(Modal::Help { topic, step })
                        }
                    }
                    KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => Some(Modal::Help {
                        topic: (topic + 1) % topics.len(),
                        step: 0,
                    }),
                    KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => Some(Modal::Help {
                        topic: (topic + topics.len() - 1) % topics.len(),
                        step: 0,
                    }),
                    KeyCode::Char(c @ '1'..='9') => {
                        let index = (c as usize) - ('1' as usize);
                        Some(Modal::Help {
                            topic: index.min(topics.len() - 1),
                            step: 0,
                        })
                    }
                    KeyCode::Char('s') if topics[topic].id == "support" => {
                        self.open_url(LINKS[2].1);
                        Some(Modal::Help { topic, step })
                    }
                    _ => Some(Modal::Help { topic, step }),
                }
            }
        }
    }

    fn alias_form_key(&mut self, mut form: AliasForm, key: KeyEvent) -> Option<Modal> {
        const FIELDS: usize = 3;
        form.completions.clear();
        if key.code == KeyCode::Esc {
            return None;
        }
        let submit = ctrl(&key, 's') || (key.code == KeyCode::Enter && form.focus == FIELDS - 1);
        if submit {
            return match self.submit_alias_form(&form) {
                Ok(()) => None,
                Err(error) => {
                    form.error = error;
                    Some(Modal::AliasForm(form))
                }
            };
        }
        match key.code {
            KeyCode::Enter | KeyCode::Down => form.focus = (form.focus + 1) % FIELDS,
            KeyCode::Up | KeyCode::BackTab => form.focus = (form.focus + FIELDS - 1) % FIELDS,
            KeyCode::Tab => {
                if form.focus == 2 && form.action() != "custom" {
                    form.completions = form.path.complete_path();
                } else {
                    form.focus = (form.focus + 1) % FIELDS;
                }
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') if form.focus == 1 => {
                form.action = if key.code == KeyCode::Left {
                    (form.action + ACTIONS.len() - 1) % ACTIONS.len()
                } else {
                    (form.action + 1) % ACTIONS.len()
                };
                form.error.clear();
            }
            _ => {
                let input = match form.focus {
                    0 => Some(&mut form.name),
                    2 if form.action() == "custom" => Some(&mut form.custom),
                    2 => Some(&mut form.path),
                    _ => None,
                };
                if let Some(input) = input {
                    if input.handle_key(key) {
                        form.error.clear();
                    }
                }
            }
        }
        Some(Modal::AliasForm(form))
    }

    fn backup_key(&mut self, mut modal: BackupModal, key: KeyEvent) -> Option<Modal> {
        modal.completions.clear();
        if key.code == KeyCode::Esc {
            return None;
        }
        if key.code == KeyCode::Enter || ctrl(&key, 's') {
            let path_changed = modal.mode == BackupMode::Import
                && modal.loaded_path.as_deref()
                    != Some(&expand_home(&modal.path.value).display().to_string());
            if path_changed && !modal.focus_list {
                modal.loaded_path = None;
                self.inspect_backup(&mut modal);
                return Some(Modal::Backup(modal));
            }
            return if self.submit_backup(&mut modal) {
                None
            } else {
                Some(Modal::Backup(modal))
            };
        }
        if modal.focus_list {
            match key.code {
                KeyCode::Tab => modal.focus_list = false,
                KeyCode::Up | KeyCode::Char('k') if modal.selected == 0 => modal.focus_list = false,
                KeyCode::Char(' ') => {
                    if let Some(item) = modal.items.get(modal.selected) {
                        if !modal.checked.remove(&item.id) {
                            modal.checked.insert(item.id.clone());
                        }
                    }
                }
                KeyCode::Char('a') => {
                    if modal.checked.len() == modal.items.len() {
                        modal.checked.clear();
                    } else {
                        modal.checked = modal.items.iter().map(|item| item.id.clone()).collect();
                    }
                }
                _ => modal.selected = move_index(modal.selected, modal.items.len(), &key),
            }
        } else {
            match key.code {
                KeyCode::Tab => {
                    modal.completions = modal.path.complete_path();
                }
                KeyCode::Down | KeyCode::BackTab if !modal.items.is_empty() => {
                    modal.focus_list = true
                }
                _ => {
                    if modal.path.handle_key(key) {
                        modal.overwrite_armed = false;
                        modal.error.clear();
                    }
                }
            }
        }
        Some(Modal::Backup(modal))
    }

    fn editor_key(&mut self, mut editor: EditorState, key: KeyEvent) -> Option<Modal> {
        editor.completions.clear();
        let field_count = EDITOR_FIXED_FIELDS + editor.step_inputs.len();
        let step_index = editor.focus.checked_sub(EDITOR_FIXED_FIELDS);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT)
            || key.modifiers.contains(KeyModifiers::ALT);

        if key.code == KeyCode::Esc {
            return None;
        }
        let at_last = editor.focus + 1 == field_count;
        if ctrl(&key, 's') || (key.code == KeyCode::Enter && at_last) {
            return match self.save_editor(&mut editor) {
                Ok(()) => None,
                Err(error) => {
                    editor.error = error;
                    Some(Modal::Editor(editor))
                }
            };
        }

        // Step management.
        let insert_at = step_index
            .map(|i| i + 1)
            .unwrap_or(editor.step_inputs.len());
        if ctrl(&key, 'n') || ctrl(&key, 'p') {
            if editor.step_inputs.len() >= MAX_AUTOMATION_STEPS {
                editor.error = format!(
                    "An automation can have at most {} steps.",
                    MAX_AUTOMATION_STEPS
                );
                return Some(Modal::Editor(editor));
            }
            let step = new_step(if ctrl(&key, 'n') { "command" } else { "wait" });
            editor.step_inputs.insert(insert_at, step_input(&step));
            editor.automation.steps.insert(insert_at, step);
            editor.focus = EDITOR_FIXED_FIELDS + insert_at;
            editor.error.clear();
            return Some(Modal::Editor(editor));
        }
        if let Some(index) = step_index {
            if ctrl(&key, 'x') || ctrl(&key, 'd') {
                editor.automation.steps.remove(index);
                editor.step_inputs.remove(index);
                let fields = EDITOR_FIXED_FIELDS + editor.step_inputs.len();
                editor.focus = editor.focus.min(fields - 1);
                return Some(Modal::Editor(editor));
            }
            if ctrl(&key, 't') && editor.automation.steps[index].kind == "command" {
                let step = &mut editor.automation.steps[index];
                step.behavior = if step.behavior == "background" {
                    "wait".to_string()
                } else {
                    "background".to_string()
                };
                return Some(Modal::Editor(editor));
            }
            if shift && matches!(key.code, KeyCode::Up | KeyCode::Down) {
                let target = if key.code == KeyCode::Up {
                    index.checked_sub(1)
                } else {
                    Some(index + 1)
                };
                if let Some(target) = target.filter(|&t| t < editor.step_inputs.len()) {
                    editor.automation.steps.swap(index, target);
                    editor.step_inputs.swap(index, target);
                    editor.focus = EDITOR_FIXED_FIELDS + target;
                }
                return Some(Modal::Editor(editor));
            }
        }

        match key.code {
            KeyCode::Down | KeyCode::Enter => {
                editor.focus = (editor.focus + 1).min(field_count - 1)
            }
            KeyCode::Up | KeyCode::BackTab => editor.focus = editor.focus.saturating_sub(1),
            KeyCode::Tab => {
                if editor.focus == 1 {
                    editor.completions = editor.path.complete_path();
                } else {
                    editor.focus = (editor.focus + 1).min(field_count - 1);
                }
            }
            _ => {
                let is_wait = step_index
                    .map(|i| editor.automation.steps[i].kind == "wait")
                    .unwrap_or(false);
                if is_wait && plain(&key) {
                    if let KeyCode::Char(c) = key.code {
                        if !c.is_ascii_digit() {
                            return Some(Modal::Editor(editor));
                        }
                    }
                }
                let input = match editor.focus {
                    0 => Some(&mut editor.name),
                    1 => Some(&mut editor.path),
                    2 => Some(&mut editor.group),
                    n => editor.step_inputs.get_mut(n - EDITOR_FIXED_FIELDS),
                };
                if let Some(input) = input {
                    if input.handle_key(key) {
                        editor.error.clear();
                    }
                }
            }
        }
        Some(Modal::Editor(editor))
    }

    fn schedule_key(&mut self, mut state: ScheduleState, key: KeyEvent) -> Option<Modal> {
        const FIELDS: usize = 4;
        if key.code == KeyCode::Esc {
            return None;
        }
        if ctrl(&key, 'd') && state.exists {
            let id = state.entry.automation_id.clone();
            return Some(Modal::Confirm {
                text: "Remove this automation's schedule?".to_string(),
                action: ConfirmAction::RemoveSchedule(id),
                back: Some(Box::new(Modal::Schedule(state))),
            });
        }
        if key.code == KeyCode::Enter || ctrl(&key, 's') {
            return match self.save_schedule(&mut state) {
                Ok(()) => None,
                Err(error) => {
                    state.error = error;
                    Some(Modal::Schedule(state))
                }
            };
        }
        let left = matches!(key.code, KeyCode::Left | KeyCode::Char('h'));
        let right = matches!(key.code, KeyCode::Right | KeyCode::Char('l'));
        match key.code {
            KeyCode::Down | KeyCode::Tab => state.focus = (state.focus + 1) % FIELDS,
            KeyCode::Up | KeyCode::BackTab => state.focus = (state.focus + FIELDS - 1) % FIELDS,
            _ => match state.focus {
                0 if left || right || key.code == KeyCode::Char(' ') => {
                    let current = TRIGGERS
                        .iter()
                        .position(|(k, _)| *k == state.entry.trigger_kind)
                        .unwrap_or(0);
                    let next = if left {
                        (current + 2) % 3
                    } else {
                        (current + 1) % 3
                    };
                    state.entry.trigger_kind = TRIGGERS[next].0.to_string();
                    state.error.clear();
                }
                1 if state.entry.trigger_kind == "clock" => {
                    let allowed = match key.code {
                        KeyCode::Char(c) if plain(&key) => c.is_ascii_digit() || c == ':',
                        _ => true,
                    };
                    if allowed && state.time.handle_key(key) {
                        state.error.clear();
                    }
                }
                1 if left || right => self.set_sun_region(right),
                2 => match key.code {
                    _ if left => state.day_cursor = (state.day_cursor + 6) % 7,
                    _ if right => state.day_cursor = (state.day_cursor + 1) % 7,
                    KeyCode::Char(' ') | KeyCode::Char('x') => {
                        toggle_day(&mut state.entry.days, WEEKDAYS[state.day_cursor])
                    }
                    KeyCode::Char(c @ '1'..='7') => {
                        let index = (c as usize) - ('1' as usize);
                        state.day_cursor = index;
                        toggle_day(&mut state.entry.days, WEEKDAYS[index]);
                    }
                    KeyCode::Char('a') | KeyCode::Char('e') => state.entry.days.clear(),
                    _ => {}
                },
                3 if left || right || key.code == KeyCode::Char(' ') => {
                    state.entry.enabled = !state.entry.enabled
                }
                _ => {}
            },
        }
        Some(Modal::Schedule(state))
    }

    fn run_key(&mut self, key: KeyEvent) -> Option<Modal> {
        let Some(run) = &mut self.run else {
            return None;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            KeyCode::Char('s') => run.stop(),
            KeyCode::Char('r') if !run.running => {
                let id = run.automation_id.clone();
                if let Some(automation) = self.automations.iter().find(|a| a.id == id).cloned() {
                    self.run = Some(super::runner::RunState::start(&automation));
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                run.follow = false;
                run.selected = run.selected.saturating_sub(1);
                run.scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                run.follow = false;
                run.selected = (run.selected + 1).min(run.steps.len().saturating_sub(1));
                run.scroll = 0;
            }
            KeyCode::PageUp => run.scroll = run.scroll.saturating_sub(10),
            KeyCode::PageDown => run.scroll = run.scroll.saturating_add(10),
            KeyCode::Char('f') => {
                run.follow = true;
                run.selected = run.current;
                run.scroll = 0;
            }
            _ => {}
        }
        Some(Modal::Run)
    }
}

fn toggle_day(days: &mut Vec<String>, day: &str) {
    if let Some(position) = days.iter().position(|d| d == day) {
        days.remove(position);
    } else {
        days.push(day.to_string());
        days.sort_by_key(|d| WEEKDAYS.iter().position(|w| w == d).unwrap_or(7));
    }
}
