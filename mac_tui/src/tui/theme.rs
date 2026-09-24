//! Colors for the three theme settings shared with the desktop app.
//! "system" keeps the terminal's own colors; "light"/"dark" paint a fixed palette.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    pub(crate) bg: Color,
    pub(crate) fg: Color,
    pub(crate) dim: Color,
    pub(crate) accent: Color,
    pub(crate) ok: Color,
    pub(crate) err: Color,
    pub(crate) warn: Color,
    pub(crate) border: Color,
    pub(crate) sel_bg: Color,
    pub(crate) sel_fg: Color,
    pub(crate) star: Color,
}

impl Theme {
    pub(crate) fn from_setting(theme: &str) -> Self {
        match theme {
            "dark" => Self {
                bg: Color::Rgb(22, 22, 26),
                fg: Color::Rgb(228, 228, 231),
                dim: Color::Rgb(140, 140, 150),
                accent: Color::Rgb(96, 165, 250),
                ok: Color::Rgb(74, 222, 128),
                err: Color::Rgb(248, 113, 113),
                warn: Color::Rgb(250, 204, 21),
                border: Color::Rgb(70, 70, 80),
                sel_bg: Color::Rgb(37, 99, 235),
                sel_fg: Color::Rgb(255, 255, 255),
                star: Color::Rgb(250, 204, 21),
            },
            "light" => Self {
                bg: Color::Rgb(250, 250, 250),
                fg: Color::Rgb(24, 24, 27),
                dim: Color::Rgb(113, 113, 122),
                accent: Color::Rgb(37, 99, 235),
                ok: Color::Rgb(22, 163, 74),
                err: Color::Rgb(220, 38, 38),
                warn: Color::Rgb(180, 120, 0),
                border: Color::Rgb(200, 200, 208),
                sel_bg: Color::Rgb(37, 99, 235),
                sel_fg: Color::Rgb(255, 255, 255),
                star: Color::Rgb(202, 138, 4),
            },
            _ => Self {
                bg: Color::Reset,
                fg: Color::Reset,
                dim: Color::DarkGray,
                accent: Color::Cyan,
                ok: Color::Green,
                err: Color::Red,
                warn: Color::Yellow,
                border: Color::DarkGray,
                sel_bg: Color::Blue,
                sel_fg: Color::White,
                star: Color::Yellow,
            },
        }
    }

    pub(crate) fn base(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }
    pub(crate) fn dim(&self) -> Style {
        Style::default().fg(self.dim)
    }
    pub(crate) fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }
    pub(crate) fn bold(&self) -> Style {
        Style::default().fg(self.fg).add_modifier(Modifier::BOLD)
    }
    pub(crate) fn title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }
    pub(crate) fn err(&self) -> Style {
        Style::default().fg(self.err)
    }
    pub(crate) fn warn(&self) -> Style {
        Style::default().fg(self.warn)
    }
    pub(crate) fn border(&self) -> Style {
        Style::default().fg(self.border)
    }
    pub(crate) fn focus_border(&self) -> Style {
        Style::default().fg(self.accent)
    }
    pub(crate) fn selected(&self) -> Style {
        Style::default()
            .fg(self.sel_fg)
            .bg(self.sel_bg)
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn key(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
    pub(crate) fn star(&self) -> Style {
        Style::default().fg(self.star)
    }
}
