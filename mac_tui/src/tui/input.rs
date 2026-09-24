//! Single-line text input with readline-style keys and path completion.

use crate::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::Frame;

#[derive(Debug, Clone, Default)]
pub(crate) struct TextInput {
    pub(crate) value: String,
    // Cursor position in chars, not bytes.
    pub(crate) cursor: usize,
}

impl TextInput {
    pub(crate) fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
            cursor: value.chars().count(),
        }
    }

    pub(crate) fn set(&mut self, value: &str) {
        self.value = value.to_string();
        self.cursor = value.chars().count();
    }

    pub(crate) fn clear(&mut self) {
        self.set("");
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map(|(index, _)| index)
            .unwrap_or(self.value.len())
    }

    fn len(&self) -> usize {
        self.value.chars().count()
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        let at = self.byte_index(self.cursor);
        self.value.insert_str(at, &clean);
        self.cursor += clean.chars().count();
    }

    fn delete_range(&mut self, from: usize, to: usize) {
        let (from_byte, to_byte) = (self.byte_index(from), self.byte_index(to));
        self.value.replace_range(from_byte..to_byte, "");
        self.cursor = from;
    }

    fn previous_word_start(&self) -> usize {
        let chars: Vec<char> = self.value.chars().collect();
        let mut index = self.cursor;
        while index > 0 && chars[index - 1].is_whitespace() {
            index -= 1;
        }
        while index > 0 && !chars[index - 1].is_whitespace() && chars[index - 1] != '/' {
            index -= 1;
        }
        if index == self.cursor && index > 0 {
            index -= 1;
        }
        index
    }

    // Returns true when the key was consumed.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.len(),
            KeyCode::Char('u') if ctrl => self.delete_range(0, self.cursor),
            KeyCode::Char('k') if ctrl => {
                let end = self.len();
                let cursor = self.cursor;
                self.delete_range(cursor, end);
            }
            KeyCode::Char('w') if ctrl => {
                let start = self.previous_word_start();
                let cursor = self.cursor;
                self.delete_range(start, cursor);
            }
            KeyCode::Backspace if alt || ctrl => {
                let start = self.previous_word_start();
                let cursor = self.cursor;
                self.delete_range(start, cursor);
            }
            KeyCode::Char(c) if !ctrl && !alt => self.insert_str(&c.to_string()),
            KeyCode::Char(c) if alt && !ctrl => self.insert_str(&c.to_string()),
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let cursor = self.cursor;
                    self.delete_range(cursor - 1, cursor);
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.len() {
                    let cursor = self.cursor;
                    self.delete_range(cursor, cursor + 1);
                    self.cursor = cursor;
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => return false,
        }
        true
    }

    // Renders the value into `area` (one line), scrolling horizontally so the
    // cursor stays visible, and places the terminal cursor when focused.
    pub(crate) fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        style: Style,
        focused: bool,
        placeholder: &str,
        placeholder_style: Style,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = area.width as usize;
        if self.value.is_empty() {
            let line = Line::from(Span::styled(placeholder.to_string(), placeholder_style));
            frame.render_widget(line, area);
            if focused {
                frame.set_cursor_position(Position::new(area.x, area.y));
            }
            return;
        }
        let chars: Vec<char> = self.value.chars().collect();
        let start = if self.cursor >= width {
            self.cursor + 1 - width
        } else {
            0
        };
        let visible: String = chars.iter().skip(start).take(width).collect();
        frame.render_widget(Line::from(Span::styled(visible, style)), area);
        if focused {
            let x = area.x + (self.cursor - start) as u16;
            frame.set_cursor_position(Position::new(x.min(area.x + area.width - 1), area.y));
        }
    }

    // Tab completion for file system paths. Completes a unique match (adding a
    // trailing "/" for folders) or the longest common prefix, and returns the
    // candidate names so the caller can show them.
    pub(crate) fn complete_path(&mut self) -> Vec<String> {
        let typed = self.value.clone();
        let (dir_part, prefix) = match typed.rfind('/') {
            Some(index) => (typed[..=index].to_string(), typed[index + 1..].to_string()),
            None if typed == "~" => {
                self.set("~/");
                return Vec::new();
            }
            None => (String::new(), typed.clone()),
        };
        let directory = if dir_part.is_empty() {
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            expand_home(&dir_part)
        };
        let Ok(entries) = fs::read_dir(&directory) else {
            return Vec::new();
        };
        let show_hidden = prefix.starts_with('.');
        let mut matches: Vec<(String, bool)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with(&prefix) || (!show_hidden && name.starts_with('.')) {
                    return None;
                }
                let is_dir = entry.path().is_dir();
                Some((name, is_dir))
            })
            .collect();
        matches.sort();
        match matches.len() {
            0 => Vec::new(),
            1 => {
                let (name, is_dir) = &matches[0];
                self.set(&format!(
                    "{}{}{}",
                    dir_part,
                    name,
                    if *is_dir { "/" } else { "" }
                ));
                Vec::new()
            }
            _ => {
                let names: Vec<String> = matches
                    .iter()
                    .map(|(name, is_dir)| {
                        if *is_dir {
                            format!("{}/", name)
                        } else {
                            name.clone()
                        }
                    })
                    .collect();
                let common = common_prefix(matches.iter().map(|(name, _)| name.as_str()));
                if common.len() > prefix.len() {
                    self.set(&format!("{}{}", dir_part, common));
                }
                names
            }
        }
    }
}

fn common_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = names.next() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for name in names {
        let chars: Vec<char> = name.chars().collect();
        let shared = prefix
            .iter()
            .zip(chars.iter())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(shared);
    }
    prefix.into_iter().collect()
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        let mut event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        event.kind = KeyEventKind::Press;
        event
    }

    #[test]
    fn edits_like_a_shell_prompt() {
        let mut input = TextInput::new("git status");
        input.handle_key(ctrl('w'));
        assert_eq!(input.value, "git ");
        input.handle_key(key(KeyCode::Char('ä')));
        assert_eq!(input.value, "git ä");
        input.handle_key(key(KeyCode::Home));
        input.handle_key(key(KeyCode::Delete));
        assert_eq!(input.value, "it ä");
        input.handle_key(ctrl('e'));
        input.handle_key(key(KeyCode::Backspace));
        assert_eq!(input.value, "it ");
        input.handle_key(ctrl('u'));
        assert_eq!(input.value, "");
    }

    #[test]
    fn completes_paths() {
        let dir = env::temp_dir().join(format!("easyalias-tui-complete-{}", std::process::id()));
        fs::create_dir_all(dir.join("projects-alpha")).unwrap();
        fs::create_dir_all(dir.join("projects-beta")).unwrap();
        fs::write(dir.join("readme.md"), "").unwrap();

        let mut input = TextInput::new(&format!("{}/re", dir.display()));
        assert!(input.complete_path().is_empty());
        assert_eq!(input.value, format!("{}/readme.md", dir.display()));

        let mut input = TextInput::new(&format!("{}/pro", dir.display()));
        let candidates = input.complete_path();
        assert_eq!(candidates, vec!["projects-alpha/", "projects-beta/"]);
        assert_eq!(input.value, format!("{}/projects-", dir.display()));

        let _ = fs::remove_dir_all(&dir);
    }
}
