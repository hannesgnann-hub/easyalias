//! The interactive terminal interface.

mod app;
mod input;
mod keys;
mod runner;
mod theme;
mod ui;

#[cfg(test)]
mod tests;

use app::App;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use std::io::stdout;
use std::time::Duration;

pub(crate) fn run() -> Result<(), String> {
    let mut app = App::load();
    let mut terminal = ratatui::init();
    let _ = execute!(stdout(), EnableBracketedPaste);

    let result = (|| -> Result<(), String> {
        loop {
            terminal
                .draw(|frame| ui::draw(frame, &mut app))
                .map_err(|error| error.to_string())?;
            if event::poll(Duration::from_millis(100)).map_err(|error| error.to_string())? {
                match event::read().map_err(|error| error.to_string())? {
                    Event::Key(key) if key.kind != KeyEventKind::Release => app.on_key(key),
                    Event::Paste(text) => app.on_paste(&text),
                    _ => {}
                }
            }
            app.on_tick();
            if app.quit {
                return Ok(());
            }
        }
    })();

    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    result
}
