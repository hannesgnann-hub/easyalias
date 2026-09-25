//! Rendering of the tabs, the footer and every modal.

use super::app::*;
use crate::help::TOPICS;
use super::input::TextInput;
use super::keys::LINKS;
use super::runner::StepStatus;
use super::theme::Theme;
use crate::suggestions::SUGGESTIONS;
use crate::*;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap,
};
use ratatui::Frame;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(crate) fn draw(frame: &mut Frame, app: &mut App) {
    let theme = app.theme;
    let area = frame.area();
    frame.render_widget(Block::default().style(theme.base()), area);

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_header(frame, app, header);
    match app.tab {
        Tab::Aliases => draw_aliases(frame, app, body),
        Tab::Automations => draw_automations(frame, app, body),
        Tab::Settings => draw_settings(frame, app, body),
    }
    draw_footer(frame, app, footer);

    if app.modal.is_some() {
        draw_modal(frame, app, area);
    }
}

// ----- chrome ------------------------------------------------------------

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let tabs = [
        (Tab::Aliases, "1", "Aliases"),
        (Tab::Automations, "2", "Automations"),
        (Tab::Settings, "3", "Settings"),
    ];
    let mut spans = vec![Span::styled(" EasyAlias ", theme.title()), Span::raw("  ")];
    for (tab, number, label) in tabs {
        if app.tab == tab {
            spans.push(Span::styled(
                format!(" {} {} ", number, label),
                theme.selected(),
            ));
        } else {
            spans.push(Span::styled(format!(" {} ", number), theme.key()));
            spans.push(Span::styled(format!("{} ", label), theme.dim()));
        }
        spans.push(Span::raw(" "));
    }
    let mut right_spans = Vec::new();
    if let Some(run) = &app.run {
        if run.running {
            right_spans.push(Span::styled(
                format!("{} ", SPINNER[(app.tick as usize) % SPINNER.len()]),
                theme.accent(),
            ));
            right_spans.push(Span::styled(truncate(&run.name, 16), theme.bold()));
            right_spans.push(Span::styled(
                format!(" {}/{}  ", run.current + 1, run.steps.len()),
                theme.dim(),
            ));
        }
    }
    right_spans.push(Span::styled("?", theme.key()));
    right_spans.push(Span::styled(" help ", theme.dim()));
    let right_line = Line::from(right_spans);
    let right_width = (right_line.width() as u16).min(area.width / 2);
    let [left, right] = Layout::horizontal([Constraint::Min(10), Constraint::Length(right_width)])
        .areas(Rect { height: 1, ..area });
    frame.render_widget(Line::from(spans), left);
    frame.render_widget(right_line.alignment(Alignment::Right), right);

    let rule = "─".repeat(area.width as usize);
    frame.render_widget(
        Line::styled(rule, theme.border()),
        Rect {
            y: area.y + 1,
            height: 1,
            ..area
        },
    );
}

fn hint(theme: &Theme, pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, label)) in pairs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", theme.dim()));
        }
        spans.push(Span::styled(key.to_string(), theme.key()));
        spans.push(Span::styled(format!(" {}", label), theme.dim()));
    }
    Line::from(spans)
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    if let Some(message) = &app.message {
        let (icon, style) = if message.error {
            ("✗ ", theme.err())
        } else {
            ("✓ ", theme.ok())
        };
        frame.render_widget(
            Line::from(vec![
                Span::raw(" "),
                Span::styled(icon, style),
                Span::styled(message.text.clone(), style),
            ]),
            area,
        );
        return;
    }
    if app.modal.is_some() {
        return;
    }
    let pairs: &[(&str, &str)] = match app.tab {
        Tab::Aliases if app.alias_search_active => {
            &[("Enter", "done"), ("Esc", "clear"), ("↑↓", "move")]
        }
        Tab::Automations if app.automation_search_active => {
            &[("Enter", "done"), ("Esc", "clear"), ("↑↓", "move")]
        }
        Tab::Aliases => &[
            ("n", "new"),
            ("⏎", "edit"),
            ("␣", "fav"),
            ("d", "trash"),
            ("/", "search"),
            ("f", "filter"),
            ("s", "suggest"),
            ("i", "import"),
            ("b/B", "backup"),
            ("t", "trash bin"),
            ("q", "quit"),
        ],
        Tab::Automations => &[
            ("n", "new"),
            ("⏎", "run"),
            ("e", "edit"),
            ("␣", "fav"),
            ("g", "group"),
            ("c", "schedule"),
            ("d", "trash"),
            ("/", "search"),
            ("f", "filter"),
            ("b/B", "backup"),
            ("t", "trash bin"),
            ("o", "last run"),
        ],
        Tab::Settings => &[("↑↓", "select"), ("←→/⏎", "change"), ("q", "quit")],
    };
    frame.render_widget(hint(theme, pairs), area);
}

fn truncate(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else if max <= 1 {
        "…".to_string()
    } else {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    }
}

fn search_line(
    theme: &Theme,
    frame: &mut Frame,
    area: Rect,
    input: &TextInput,
    active: bool,
    filter: &str,
    count: &str,
) {
    let [search_area, rest] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Min(10)]).areas(area);
    let prefix = Span::styled(" / ", if active { theme.key() } else { theme.dim() });
    frame.render_widget(Line::from(prefix), search_area);
    let input_area = Rect {
        x: search_area.x + 3,
        width: search_area.width.saturating_sub(4),
        ..search_area
    };
    input.render(
        frame,
        input_area,
        theme.bold(),
        active,
        if active { "type to search" } else { "search" },
        theme.dim(),
    );
    frame.render_widget(
        Line::from(vec![
            Span::styled("f ", theme.key()),
            Span::styled(
                filter.to_string(),
                if filter.starts_with("All") {
                    theme.dim()
                } else {
                    theme.accent()
                },
            ),
            Span::styled(format!("   {}", count), theme.dim()),
        ])
        .alignment(Alignment::Right),
        Rect {
            width: rest.width.saturating_sub(1),
            ..rest
        },
    );
}

fn count_label(
    total: usize,
    shown: usize,
    active_filter: bool,
    singular: &str,
    plural: &str,
) -> String {
    let noun = if total == 1 { singular } else { plural };
    if active_filter {
        format!("{} of {} {}", shown, total, noun)
    } else {
        format!("{} {}", total, noun)
    }
}

fn panel<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border())
        .title(Span::styled(format!(" {} ", title), theme.title()))
}

// ----- aliases -----------------------------------------------------------

fn draw_aliases(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let [status, search, list, detail] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(5),
    ])
    .areas(area);

    let status_line = match platform::status_line(&app.alias_state) {
        (true, text) => Line::from(vec![
            Span::styled(" ✓ ", theme.ok()),
            Span::styled(text, theme.dim()),
        ]),
        (false, text) => Line::from(vec![
            Span::styled(" ⚠ ", theme.warn()),
            Span::styled(text, theme.warn()),
        ]),
    };
    frame.render_widget(status_line, status);

    let visible = app.visible_aliases();
    let filter_label = ALIAS_FILTERS[app.alias_filter].1;
    let active = app.alias_filter != 0 || !app.alias_search.value.trim().is_empty();
    let count = count_label(
        app.alias_state.aliases.len(),
        visible.len(),
        active,
        "alias",
        "aliases",
    );
    search_line(
        &theme,
        frame,
        search,
        &app.alias_search,
        app.alias_search_active,
        filter_label,
        &count,
    );

    let block = panel(&theme, "Aliases");
    if visible.is_empty() {
        let text = if app.alias_state.aliases.is_empty() {
            vec![
                Line::from(""),
                Line::styled("No aliases yet.", theme.bold()),
                Line::from(""),
                Line::from(vec![
                    Span::styled("n", theme.key()),
                    Span::styled(" create one   ", theme.dim()),
                    Span::styled("s", theme.key()),
                    Span::styled(" suggestions   ", theme.dim()),
                    Span::styled("i", theme.key()),
                    Span::styled(" import from your shell", theme.dim()),
                ]),
            ]
        } else {
            vec![
                Line::from(""),
                Line::styled("No aliases match this search or filter.", theme.dim()),
                Line::from(vec![
                    Span::styled("Esc", theme.key()),
                    Span::styled(" clear", theme.dim()),
                ]),
            ]
        };
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .block(block),
            list,
        );
    } else {
        let rows: Vec<Row> = visible
            .iter()
            .map(|&index| {
                let alias = &app.alias_state.aliases[index];
                Row::new(vec![
                    Cell::from(if alias.favorite { "★" } else { " " }).style(theme.star()),
                    Cell::from(alias.name.clone()).style(theme.bold()),
                    Cell::from(action_label(&alias.action)).style(theme.dim()),
                    Cell::from(alias.command_preview.clone()),
                ])
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Length(18),
                Constraint::Length(15),
                Constraint::Min(10),
            ],
        )
        .header(
            Row::new(vec!["", "Name", "Action", "Command"])
                .style(theme.dim().add_modifier(Modifier::UNDERLINED)),
        )
        .column_spacing(2)
        .block(block)
        .row_highlight_style(theme.selected())
        .highlight_symbol("▸ ");
        frame.render_stateful_widget(table, list, &mut app.alias_table);
    }

    let detail_block = panel(&theme, "Details");
    let lines = match app.selected_alias() {
        Some(alias) => vec![
            Line::from(vec![
                Span::styled(format!("{} ", alias.name), theme.title()),
                Span::styled(format!("· {}", action_label(&alias.action)), theme.dim()),
            ]),
            Line::from(vec![
                Span::styled("$ ", theme.dim()),
                Span::styled(alias.command_preview.clone(), theme.bold()),
            ]),
            Line::styled(
                format!(
                    "{}updated {}",
                    if alias.path.trim().is_empty() || alias.action == "custom" {
                        String::new()
                    } else {
                        format!("{} · ", alias.path)
                    },
                    alias.updated_at.get(..10).unwrap_or(&alias.updated_at)
                ),
                theme.dim(),
            ),
        ],
        None => vec![Line::styled(
            "Select an alias to see its details.",
            theme.dim(),
        )],
    };
    frame.render_widget(
        Paragraph::new(lines).block(detail_block.padding(Padding::horizontal(1))),
        detail,
    );
}

// ----- automations -------------------------------------------------------

fn schedule_summary(app: &App, automation_id: &str) -> (String, Style) {
    match app.schedule_for(automation_id) {
        Some(entry) => {
            let icon = match entry.trigger_kind.as_str() {
                "sunrise" => "☀",
                "sunset" => "☾",
                _ => "⏰",
            };
            let days = if entry.days.is_empty() {
                String::new()
            } else {
                format!(" {}", format_days(&entry.days))
            };
            let text = format!("{} {}{}", icon, format_trigger(entry), days);
            (
                text,
                if entry.enabled {
                    app.theme.accent()
                } else {
                    app.theme.dim().add_modifier(Modifier::CROSSED_OUT)
                },
            )
        }
        None => ("-".to_string(), app.theme.dim()),
    }
}

fn last_run_summary(app: &App, automation: &Automation) -> (String, Style) {
    if let Some(run) = &app.run {
        if run.automation_id == automation.id {
            if run.running {
                return (
                    format!("{} running", SPINNER[(app.tick as usize) % SPINNER.len()]),
                    app.theme.accent(),
                );
            }
            return if run.succeeded() {
                ("✓ just now".to_string(), app.theme.ok())
            } else {
                ("✗ just now".to_string(), app.theme.err())
            };
        }
    }
    if let Some(entry) = app.schedule_for(&automation.id) {
        if let Some(at) = entry.last_run_at {
            let ok = entry.last_run_status.as_deref() == Some("success");
            return (
                format!(
                    "{} {} (scheduled)",
                    if ok { "✓" } else { "✗" },
                    relative_time(at)
                ),
                if ok { app.theme.ok() } else { app.theme.err() },
            );
        }
    }
    (String::new(), app.theme.dim())
}

fn draw_automations(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let [search, list, detail] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(8),
    ])
    .areas(area);

    let rows_model = app.automation_rows();
    let shown = rows_model
        .iter()
        .filter(|row| matches!(row, AutomationRow::Item(_)))
        .count();
    let active = app.automation_filter != "all" || !app.automation_search.value.trim().is_empty();
    let count = if app.automation_filter == "groups" {
        let headers = rows_model
            .iter()
            .filter(|row| matches!(row, AutomationRow::Header(..)))
            .count();
        format!(
            "{} {}",
            headers,
            if headers == 1 { "group" } else { "groups" }
        )
    } else {
        count_label(
            app.automations.len(),
            shown,
            active,
            "automation",
            "automations",
        )
    };
    let filter_label = app.automation_filter_label();
    search_line(
        &theme,
        frame,
        search,
        &app.automation_search,
        app.automation_search_active,
        &filter_label,
        &count,
    );

    let block = panel(&theme, "Automations");
    if shown == 0 {
        let text = if app.automations.is_empty() {
            vec![
                Line::from(""),
                Line::styled("No automations yet.", theme.bold()),
                Line::styled(
                    "Chain commands into one workflow and run it with one key.",
                    theme.dim(),
                ),
                Line::from(""),
                Line::from(vec![
                    Span::styled("n", theme.key()),
                    Span::styled(" create one   ", theme.dim()),
                    Span::styled("?", theme.key()),
                    Span::styled(" how it works", theme.dim()),
                ]),
            ]
        } else {
            vec![
                Line::from(""),
                Line::styled("No automations match this search or filter.", theme.dim()),
                Line::from(vec![
                    Span::styled("Esc", theme.key()),
                    Span::styled(" clear", theme.dim()),
                ]),
            ]
        };
        frame.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .block(block),
            list,
        );
    } else {
        let rows: Vec<Row> = rows_model
            .iter()
            .map(|row| match row {
                AutomationRow::Header(name, count) => Row::new(vec![
                    Cell::from(""),
                    Cell::from(Line::from(vec![
                        Span::styled(format!("▾ {}", name), theme.title()),
                        Span::styled(format!("  {}", count), theme.dim()),
                    ])),
                ]),
                AutomationRow::Item(index) => {
                    let automation = &app.automations[*index];
                    let steps = automation.steps.len();
                    let (schedule, schedule_style) = schedule_summary(app, &automation.id);
                    let (last, last_style) = last_run_summary(app, automation);
                    let background = automation
                        .steps
                        .iter()
                        .any(|s| s.kind == "command" && s.behavior == "background");
                    Row::new(vec![
                        Cell::from(if automation.favorite { "★" } else { " " }).style(theme.star()),
                        Cell::from(automation.name.clone()).style(theme.bold()),
                        Cell::from(automation.group.clone()).style(theme.dim()),
                        Cell::from(format!(
                            "{} step{}{}",
                            steps,
                            if steps == 1 { "" } else { "s" },
                            if background { " ⇢" } else { "" }
                        ))
                        .style(theme.dim()),
                        Cell::from(schedule).style(schedule_style),
                        Cell::from(last).style(last_style),
                    ])
                }
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(1),
                Constraint::Min(14),
                Constraint::Length(14),
                Constraint::Length(10),
                Constraint::Length(22),
                Constraint::Length(22),
            ],
        )
        .header(
            Row::new(vec!["", "Name", "Group", "Steps", "Schedule", "Last run"])
                .style(theme.dim().add_modifier(Modifier::UNDERLINED)),
        )
        .column_spacing(2)
        .block(block)
        .row_highlight_style(theme.selected())
        .highlight_symbol("▸ ");
        frame.render_stateful_widget(table, list, &mut app.automation_table);
    }

    let detail_block = panel(&theme, "Details").padding(Padding::horizontal(1));
    let lines = match app.selected_automation() {
        Some(automation) => {
            let mut lines = vec![Line::from(vec![
                Span::styled(format!("{} ", automation.name), theme.title()),
                Span::styled(format!("in {}", automation.path), theme.dim()),
            ])];
            let available = detail.height.saturating_sub(4) as usize;
            for (index, step) in automation.steps.iter().enumerate().take(available) {
                let body = if step.kind == "wait" {
                    Span::styled(format!("wait {}s", step.seconds), theme.dim())
                } else {
                    Span::raw(step.command.clone())
                };
                let mut spans = vec![
                    Span::styled(format!("{:>2}. ", index + 1), theme.dim()),
                    body,
                ];
                if step.kind == "command" && step.behavior == "background" {
                    spans.push(Span::styled("  (background)", theme.accent()));
                }
                lines.push(Line::from(spans));
            }
            if automation.steps.len() > available {
                lines.push(Line::styled(
                    format!("    … {} more", automation.steps.len() - available),
                    theme.dim(),
                ));
            }
            if let Some(entry) = app.schedule_for(&automation.id) {
                if let (Some(status), Some(output)) =
                    (&entry.last_run_status, &entry.last_run_output)
                {
                    if status != "success" && lines.len() < detail.height.saturating_sub(2) as usize
                    {
                        lines.push(Line::styled(
                            format!("Last scheduled run: {}", truncate(output, 120)),
                            theme.err(),
                        ));
                    }
                }
            }
            lines
        }
        None => vec![Line::styled(
            "Select an automation to see its steps.",
            theme.dim(),
        )],
    };
    frame.render_widget(Paragraph::new(lines).block(detail_block), detail);
}

// ----- settings ----------------------------------------------------------

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let [top, bottom] = Layout::vertical([Constraint::Min(14), Constraint::Length(8)]).areas(area);

    let region_label = SUN_REGIONS
        .iter()
        .find(|(key, ..)| *key == app.sun_location.region)
        .map(|(_, label, ..)| *label)
        .unwrap_or("Unknown");
    let theme_label = match app.settings.theme.as_str() {
        "light" => "Light",
        "dark" => "Dark",
        _ => "System (terminal colors)",
    };
    let mut rows: Vec<(String, String, &str)> = vec![
        (
            "Theme".to_string(),
            theme_label.to_string(),
            "Colors of this interface. System keeps your terminal's palette.",
        ),
        (
            "Alias suggestions".to_string(),
            if app.settings.show_suggestions {
                "On"
            } else {
                "Off"
            }
            .to_string(),
            "Offer the built-in catalog of ready-made aliases (s).",
        ),
        (
            "Sunrise/sunset region".to_string(),
            region_label.to_string(),
            "Used by every sunrise and sunset schedule.",
        ),
    ];
    for (label, url) in LINKS {
        rows.push((
            label.to_string(),
            url.trim_start_matches("https://").to_string(),
            "Press Enter to open in your browser.",
        ));
    }

    let mut lines = Vec::new();
    for (index, (label, value, _)) in rows.iter().enumerate() {
        let selected = index == app.settings_selected;
        let is_link = index >= 3;
        if index == 3 {
            lines.push(Line::from(""));
        }
        let value_span = if is_link {
            Span::styled(value.clone(), theme.accent())
        } else {
            Span::styled(
                format!("‹ {} ›", value),
                if selected {
                    theme.bold()
                } else {
                    Style::default().fg(theme.fg)
                },
            )
        };
        let marker = if selected {
            Span::styled("▸ ", theme.key())
        } else {
            Span::raw("  ")
        };
        let label_style = if selected { theme.bold() } else { theme.dim() };
        lines.push(Line::from(vec![
            marker,
            Span::styled(format!("{:<26}", label), label_style),
            value_span,
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        format!("  {}", rows[app.settings_selected].2),
        theme.dim(),
    ));
    frame.render_widget(
        Paragraph::new(lines).block(panel(&theme, "Settings").padding(Padding::uniform(1))),
        top,
    );

    let state = &app.alias_state;
    let row = |label: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!("{:<16}", label), theme.dim()),
            value,
        ])
    };
    let mut info: Vec<Line> = platform::about_rows(state)
        .into_iter()
        .map(|(label, value)| row(label, Span::raw(value)))
        .collect();
    let (connected, status) = platform::status_line(state);
    info.push(row(
        "Status",
        if connected {
            Span::styled(format!("✓ {}", status), theme.ok())
        } else {
            Span::styled(format!("⚠ {}", status), theme.warn())
        },
    ));
    info.extend([
        row(
            "Version",
            Span::raw(format!("easyalias-tui {}", env!("CARGO_PKG_VERSION"))),
        ),
        Line::styled(
            "Aliases, automations and schedules are shared with the EasyAlias desktop app.",
            theme.dim(),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(info)
            .wrap(Wrap { trim: false })
            .block(panel(&theme, "About").padding(Padding::horizontal(1))),
        bottom,
    );
}

// ----- modals ------------------------------------------------------------

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width
        .min(area.width.saturating_sub(2))
        .max(10.min(area.width));
    let height = height
        .min(area.height.saturating_sub(2))
        .max(3.min(area.height));
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn modal_block<'a>(theme: &Theme, title: String) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.focus_border())
        .title(Span::styled(format!(" {} ", title), theme.title()))
        .style(Style::default().bg(theme.bg).fg(theme.fg))
        .padding(Padding::horizontal(1))
}

fn open_modal(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    width: u16,
    height: u16,
    title: String,
) -> Rect {
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let block = modal_block(theme, title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

fn field_row(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    label: &str,
    input: &TextInput,
    focused: bool,
    placeholder: &str,
) {
    let [label_area, input_area] =
        Layout::horizontal([Constraint::Length(16), Constraint::Min(4)]).areas(area);
    let marker = if focused { "▸ " } else { "  " };
    frame.render_widget(
        Line::from(vec![
            Span::styled(marker, theme.key()),
            Span::styled(
                label.to_string(),
                if focused { theme.bold() } else { theme.dim() },
            ),
        ]),
        label_area,
    );
    let style = if focused {
        theme.bold().add_modifier(Modifier::UNDERLINED)
    } else {
        Style::default().fg(theme.fg)
    };
    input.render(frame, input_area, style, focused, placeholder, theme.dim());
}

fn choice_row(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    label: &str,
    value: &str,
    focused: bool,
) {
    let marker = if focused { "▸ " } else { "  " };
    frame.render_widget(
        Line::from(vec![
            Span::styled(marker, theme.key()),
            Span::styled(
                format!("{:<14}", label),
                if focused { theme.bold() } else { theme.dim() },
            ),
            Span::styled(
                format!("‹ {} ›", value),
                if focused {
                    theme.accent().add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg)
                },
            ),
        ]),
        area,
    );
}

fn line_at(area: Rect, offset: u16) -> Rect {
    Rect {
        y: area.y + offset.min(area.height.saturating_sub(1)),
        height: 1,
        ..area
    }
}

fn error_and_completions(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    error: &str,
    completions: &[String],
) {
    if !error.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(format!("✗ {}", error), theme.err()))
                .wrap(Wrap { trim: true }),
            area,
        );
    } else if !completions.is_empty() {
        let text = completions
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("  ");
        let more = if completions.len() > 12 {
            format!("  … +{}", completions.len() - 12)
        } else {
            String::new()
        };
        frame.render_widget(
            Paragraph::new(Line::styled(format!("{}{}", text, more), theme.dim()))
                .wrap(Wrap { trim: true }),
            area,
        );
    }
}

fn checklist_rows<'a>(
    theme: &Theme,
    items: impl Iterator<Item = (bool, String, String)>,
) -> Vec<Row<'a>> {
    items
        .map(|(checked, label, detail)| {
            Row::new(vec![
                Cell::from(if checked { "[x]" } else { "[ ]" }).style(if checked {
                    theme.accent()
                } else {
                    theme.dim()
                }),
                Cell::from(label).style(theme.bold()),
                Cell::from(detail).style(theme.dim()),
            ])
        })
        .collect()
}

fn draw_modal(frame: &mut Frame, app: &mut App, area: Rect) {
    let theme = app.theme;
    let Some(modal) = app.modal.take() else {
        return;
    };
    match &modal {
        Modal::AliasForm(form) => draw_alias_form(frame, &theme, area, form),
        Modal::Suggestions { selected } => draw_suggestions(frame, app, area, *selected),
        Modal::ShellImport { selected, checked } => {
            let candidates = platform::import_candidates(&app.alias_state);
            let height = (candidates.len() as u16 + 9).min(26);
            let inner = open_modal(
                frame,
                &theme,
                area,
                100,
                height,
                platform::IMPORT_TITLE.to_string(),
            );
            let [intro, list, footer] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .areas(inner);
            frame.render_widget(
                Paragraph::new(platform::import_intro(&app.alias_state, candidates.len()))
                    .style(theme.dim())
                    .wrap(Wrap { trim: true }),
                intro,
            );
            let rows = checklist_rows(
                &theme,
                candidates
                    .iter()
                    .map(|c| (checked.contains(&c.id), c.name.clone(), c.detail.clone())),
            );
            let table = Table::new(
                rows,
                [
                    Constraint::Length(3),
                    Constraint::Length(16),
                    Constraint::Min(10),
                ],
            )
            .column_spacing(1)
            .row_highlight_style(theme.selected())
            .highlight_symbol("▸ ");
            let mut state = TableState::default().with_selected(Some(*selected));
            frame.render_stateful_widget(table, list, &mut state);
            frame.render_widget(
                hint(
                    &theme,
                    &[
                        ("␣", "toggle"),
                        ("a", "all"),
                        ("⏎", "import selected"),
                        ("d", "don't import, don't ask again"),
                        ("Esc", "later"),
                    ],
                ),
                line_at(footer, 1),
            );
        }
        Modal::AliasTrash { selected } => {
            let rows = app.alias_trash.iter().map(|entry| {
                Row::new(vec![
                    Cell::from(entry.alias.name.clone()).style(theme.bold()),
                    Cell::from(entry.alias.command_preview.clone()).style(theme.dim()),
                    Cell::from(format!("{} days left", days_left(entry.deleted_at)))
                        .style(theme.warn()),
                ])
            });
            draw_trash(
                frame,
                &theme,
                area,
                "Alias Trash",
                rows.collect(),
                app.alias_trash.len(),
                *selected,
                [18, 0, 13],
            );
        }
        Modal::AutomationTrash { selected } => {
            let rows = app.automation_trash.iter().map(|entry| {
                let steps = entry.automation.steps.len();
                Row::new(vec![
                    Cell::from(entry.automation.name.clone()).style(theme.bold()),
                    Cell::from(format!(
                        "{} step{} · {}",
                        steps,
                        if steps == 1 { "" } else { "s" },
                        entry.automation.path
                    ))
                    .style(theme.dim()),
                    Cell::from(format!("{} days left", days_left(entry.deleted_at)))
                        .style(theme.warn()),
                ])
            });
            draw_trash(
                frame,
                &theme,
                area,
                "Automation Trash",
                rows.collect(),
                app.automation_trash.len(),
                *selected,
                [22, 0, 13],
            );
        }
        Modal::Backup(modal) => draw_backup(frame, &theme, area, modal),
        Modal::Editor(editor) => draw_editor(frame, &theme, area, editor),
        Modal::Group {
            input, selected, ..
        } => {
            let groups = app.automation_groups();
            let height = 8 + groups.len().min(8) as u16;
            let inner = open_modal(frame, &theme, area, 60, height, "Group".to_string());
            frame.render_widget(
                Line::styled(
                    "Free-text label to group and filter automations. Empty removes it.",
                    theme.dim(),
                ),
                line_at(inner, 0),
            );
            field_row(
                frame,
                &theme,
                line_at(inner, 2),
                "Group",
                input,
                true,
                "no group",
            );
            if !groups.is_empty() {
                frame.render_widget(
                    Line::styled("Existing groups (↑↓):", theme.dim()),
                    line_at(inner, 4),
                );
                for (index, group) in groups.iter().take(8).enumerate() {
                    let style = if Some(index) == *selected {
                        theme.selected()
                    } else {
                        Style::default().fg(theme.fg)
                    };
                    frame.render_widget(
                        Line::styled(format!("  {}", group), style),
                        line_at(inner, 5 + index as u16),
                    );
                }
            }
            frame.render_widget(
                hint(&theme, &[("⏎", "save"), ("Esc", "cancel")]),
                line_at(inner, inner.height.saturating_sub(1)),
            );
        }
        Modal::Schedule(state) => draw_schedule(frame, app, area, state),
        Modal::Run => draw_run(frame, app, area),
        Modal::Confirm { text, .. } => {
            let inner = open_modal(frame, &theme, area, 64, 7, "Confirm".to_string());
            frame.render_widget(
                Paragraph::new(text.clone())
                    .style(theme.bold())
                    .wrap(Wrap { trim: true }),
                Rect { height: 3, ..inner },
            );
            frame.render_widget(
                hint(&theme, &[("y", "yes"), ("n/Esc", "no")]),
                line_at(inner, inner.height.saturating_sub(1)),
            );
        }
        Modal::Help { topic, step } => draw_help(frame, &theme, area, *topic, *step),
    }
    app.modal = Some(modal);
}

fn draw_trash(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    title: &str,
    rows: Vec<Row>,
    len: usize,
    selected: usize,
    widths: [u16; 3],
) {
    let inner = open_modal(
        frame,
        theme,
        area,
        96,
        (len.max(1) as u16 + 8).min(24),
        title.to_string(),
    );
    let [intro, list, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Line::styled(
            "Deleted items stay here for 30 days, then they are removed for good.",
            theme.dim(),
        ),
        intro,
    );
    if len == 0 {
        frame.render_widget(
            Paragraph::new("Trash is empty.")
                .style(theme.dim())
                .alignment(Alignment::Center),
            list,
        );
    } else {
        let table = Table::new(
            rows,
            [
                Constraint::Length(widths[0]),
                Constraint::Min(10),
                Constraint::Length(widths[2]),
            ],
        )
        .column_spacing(2)
        .row_highlight_style(theme.selected())
        .highlight_symbol("▸ ");
        let mut state = TableState::default().with_selected(Some(selected.min(len - 1)));
        frame.render_stateful_widget(table, list, &mut state);
    }
    frame.render_widget(
        hint(
            theme,
            &[
                ("r/⏎", "restore"),
                ("d", "delete forever"),
                ("E", "empty trash"),
                ("Esc", "close"),
            ],
        ),
        line_at(footer, 1),
    );
}

fn draw_alias_form(frame: &mut Frame, theme: &Theme, area: Rect, form: &AliasForm) {
    let title = if form.editing_id.is_some() {
        "Edit alias"
    } else {
        "New alias"
    };
    let inner = open_modal(frame, theme, area, 84, 15, title.to_string());
    field_row(
        frame,
        theme,
        line_at(inner, 1),
        "Command name",
        &form.name,
        form.focus == 0,
        "e.g. myproj",
    );
    choice_row(
        frame,
        theme,
        line_at(inner, 3),
        "Action",
        action_label(form.action()),
        form.focus == 1,
    );
    if form.action() == "custom" {
        field_row(
            frame,
            theme,
            line_at(inner, 5),
            "Command",
            &form.custom,
            form.focus == 2,
            "e.g. git status --short",
        );
    } else {
        let placeholder = match form.action() {
            "navigate" | "compile_gradle" | "compile_maven" => platform::FOLDER_PLACEHOLDER,
            _ => platform::FILE_PLACEHOLDER,
        };
        field_row(
            frame,
            theme,
            line_at(inner, 5),
            "Path",
            &form.path,
            form.focus == 2,
            placeholder,
        );
    }
    let preview = form.preview();
    frame.render_widget(
        Line::from(vec![
            Span::styled("  Preview       ", theme.dim()),
            if preview.is_empty() {
                Span::styled("-", theme.dim())
            } else {
                Span::styled(
                    platform::preview_line(
                        if form.name.value.trim().is_empty() {
                            "name"
                        } else {
                            form.name.value.trim()
                        },
                        &preview,
                    ),
                    theme.accent(),
                )
            },
        ]),
        line_at(inner, 7),
    );
    error_and_completions(
        frame,
        theme,
        Rect {
            y: inner.y + 9,
            height: 2,
            ..inner
        },
        &form.error,
        &form.completions,
    );
    frame.render_widget(
        hint(
            theme,
            &[
                ("↑↓", "field"),
                ("←→", "action"),
                ("Tab", "complete"),
                ("⏎", "next/save"),
                ("Esc", "cancel"),
            ],
        ),
        line_at(inner, inner.height.saturating_sub(1)),
    );
}

fn draw_suggestions(frame: &mut Frame, app: &App, area: Rect, selected: usize) {
    let theme = app.theme;
    let inner = open_modal(frame, &theme, area, 100, 28, "Suggestions".to_string());
    let [intro, list, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Line::styled(
            "Ready-made aliases. Enter adds the selected one right away.",
            theme.dim(),
        ),
        intro,
    );
    let rows: Vec<Row> = SUGGESTIONS
        .iter()
        .map(|suggestion| {
            let exists = app
                .alias_state
                .aliases
                .iter()
                .any(|alias| platform::same_alias_name(&alias.name, suggestion.name));
            let preview = platform::preview_command(
                suggestion.action,
                suggestion.path,
                suggestion.custom_command,
            );
            Row::new(vec![
                Cell::from(if exists { "✓" } else { " " }).style(theme.ok()),
                Cell::from(suggestion.name).style(if exists { theme.dim() } else { theme.bold() }),
                Cell::from(preview).style(if exists {
                    theme.dim()
                } else {
                    Style::default().fg(theme.fg)
                }),
                Cell::from(suggestion.description).style(theme.dim()),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(30),
        ],
    )
    .column_spacing(2)
    .row_highlight_style(theme.selected())
    .highlight_symbol("▸ ");
    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(table, list, &mut state);
    frame.render_widget(
        hint(&theme, &[("⏎", "add"), ("↑↓", "move"), ("Esc", "close")]),
        line_at(footer, 1),
    );
}

fn draw_backup(frame: &mut Frame, theme: &Theme, area: Rect, modal: &BackupModal) {
    let noun = if modal.kind == BackupKind::Aliases {
        "aliases"
    } else {
        "automations"
    };
    let title = match modal.mode {
        BackupMode::Export => format!("Export {}", noun),
        BackupMode::Import => format!("Import {}", noun),
    };
    let inner = open_modal(frame, theme, area, 96, 24, title);
    let [path_area, _, list_area, message, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(inner);
    let label = if modal.mode == BackupMode::Export {
        "Save to"
    } else {
        "Backup file"
    };
    field_row(
        frame,
        theme,
        path_area,
        label,
        &modal.path,
        !modal.focus_list,
        "~/easyalias-backup.json",
    );

    if modal.items.is_empty() {
        let text = if modal.mode == BackupMode::Import {
            "Enter the path of an EasyAlias JSON backup and press Enter to open it."
        } else {
            "Nothing to export."
        };
        frame.render_widget(
            Paragraph::new(text)
                .style(theme.dim())
                .wrap(Wrap { trim: true }),
            list_area,
        );
    } else {
        let selected_count = modal.checked.len();
        let caption = Line::styled(
            format!("{} of {} selected", selected_count, modal.items.len()),
            theme.dim(),
        );
        frame.render_widget(caption, line_at(list_area, 0));
        let rows = checklist_rows(
            theme,
            modal.items.iter().map(|item| {
                (
                    modal.checked.contains(&item.id),
                    item.label.clone(),
                    item.detail.clone(),
                )
            }),
        );
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(22),
                Constraint::Min(10),
            ],
        )
        .column_spacing(1)
        .row_highlight_style(if modal.focus_list {
            theme.selected()
        } else {
            Style::default()
        })
        .highlight_symbol(if modal.focus_list { "▸ " } else { "  " });
        let mut state = TableState::default().with_selected(Some(modal.selected));
        frame.render_stateful_widget(
            table,
            Rect {
                y: list_area.y + 1,
                height: list_area.height.saturating_sub(1),
                ..list_area
            },
            &mut state,
        );
    }
    error_and_completions(frame, theme, message, &modal.error, &modal.completions);
    let action = match modal.mode {
        BackupMode::Export => "export",
        BackupMode::Import if modal.loaded_path.is_none() || !modal.focus_list => "open / import",
        BackupMode::Import => "import",
    };
    frame.render_widget(
        hint(
            theme,
            &[
                ("Tab", if modal.focus_list { "path" } else { "complete" }),
                ("↓", "list"),
                ("␣", "toggle"),
                ("a", "all"),
                ("⏎", action),
                ("Esc", "cancel"),
            ],
        ),
        footer,
    );
}

fn draw_editor(frame: &mut Frame, theme: &Theme, area: Rect, editor: &EditorState) {
    let title = if editor.is_new {
        "New automation"
    } else {
        "Edit automation"
    };
    let height = (editor.step_inputs.len().max(3) as u16) + 17;
    let inner = open_modal(frame, theme, area, 100, height.min(40), title.to_string());
    let [fields, _, steps_title, steps_area, message, footer] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .areas(inner);
    field_row(
        frame,
        theme,
        line_at(fields, 0),
        "Name",
        &editor.name,
        editor.focus == 0,
        "e.g. Start dev",
    );
    field_row(
        frame,
        theme,
        line_at(fields, 2),
        "Working dir",
        &editor.path,
        editor.focus == 1,
        platform::FOLDER_PLACEHOLDER,
    );
    field_row(
        frame,
        theme,
        line_at(fields, 4),
        "Group",
        &editor.group,
        editor.focus == 2,
        "optional",
    );

    frame.render_widget(
        Line::from(vec![
            Span::styled(
                format!("Steps ({})", editor.step_inputs.len()),
                theme.title(),
            ),
            Span::styled("   all commands share one shell session", theme.dim()),
        ]),
        steps_title,
    );

    let visible = steps_area.height as usize;
    let focus_step = editor.focus.checked_sub(EDITOR_FIXED_FIELDS);
    let total = editor.step_inputs.len();
    let start = match focus_step {
        Some(index) if index >= visible => index + 1 - visible,
        _ => 0,
    };
    if total == 0 {
        frame.render_widget(
            Line::styled(
                "No steps yet - Ctrl+N adds a command, Ctrl+P a wait.",
                theme.dim(),
            ),
            line_at(steps_area, 0),
        );
    }
    for (row, index) in (start..total).take(visible).enumerate() {
        let step = &editor.automation.steps[index];
        let focused = focus_step == Some(index);
        let line = line_at(steps_area, row as u16);
        let marker = if focused { "▸" } else { " " };
        if step.kind == "wait" {
            frame.render_widget(
                Line::from(vec![
                    Span::styled(
                        format!("{} {:>2}. ", marker, index + 1),
                        if focused { theme.key() } else { theme.dim() },
                    ),
                    Span::styled("wait ", theme.accent()),
                ]),
                line,
            );
            let input_area = Rect {
                x: line.x + 11,
                width: 7,
                ..line
            };
            editor.step_inputs[index].render(
                frame,
                input_area,
                if focused {
                    theme.bold().add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default().fg(theme.fg)
                },
                focused,
                "10",
                theme.dim(),
            );
            frame.render_widget(
                Line::styled("seconds", theme.dim()),
                Rect {
                    x: line.x + 19,
                    width: 10,
                    ..line
                },
            );
        } else {
            let behavior = if step.behavior == "background" {
                "⇢ background"
            } else {
                "waits"
            };
            let behavior_width = 14u16;
            frame.render_widget(
                Line::from(vec![
                    Span::styled(
                        format!("{} {:>2}. ", marker, index + 1),
                        if focused { theme.key() } else { theme.dim() },
                    ),
                    Span::styled("$ ", theme.dim()),
                ]),
                line,
            );
            let input_area = Rect {
                x: line.x + 8,
                width: line.width.saturating_sub(8 + behavior_width + 1),
                ..line
            };
            editor.step_inputs[index].render(
                frame,
                input_area,
                if focused {
                    theme.bold().add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default().fg(theme.fg)
                },
                focused,
                "command",
                theme.dim(),
            );
            frame.render_widget(
                Line::styled(
                    behavior,
                    if step.behavior == "background" {
                        theme.accent()
                    } else {
                        theme.dim()
                    },
                )
                .alignment(Alignment::Right),
                Rect {
                    x: line.x + line.width.saturating_sub(behavior_width),
                    width: behavior_width,
                    ..line
                },
            );
        }
    }
    error_and_completions(frame, theme, message, &editor.error, &editor.completions);
    frame.render_widget(
        hint(
            theme,
            &[
                ("^N", "+command"),
                ("^P", "+wait"),
                ("^T", "wait/background"),
                ("⇧↑↓", "move"),
                ("^X", "remove"),
            ],
        ),
        line_at(footer, 0),
    );
    frame.render_widget(
        hint(
            theme,
            &[
                ("↑↓", "field"),
                ("Tab", "complete path"),
                ("^S", "save"),
                ("Esc", "cancel"),
            ],
        ),
        line_at(footer, 1),
    );
}

fn draw_schedule(frame: &mut Frame, app: &App, area: Rect, state: &ScheduleState) {
    let theme = app.theme;
    let inner = open_modal(
        frame,
        &theme,
        area,
        76,
        20,
        format!("Schedule \"{}\"", truncate(&state.automation_name, 30)),
    );
    let trigger_label = TRIGGERS
        .iter()
        .find(|(k, _)| *k == state.entry.trigger_kind)
        .map(|(_, l)| *l)
        .unwrap_or("Time");
    choice_row(
        frame,
        &theme,
        line_at(inner, 1),
        "Trigger",
        trigger_label,
        state.focus == 0,
    );

    if state.entry.trigger_kind == "clock" {
        let [label, input] = Layout::horizontal([Constraint::Length(16), Constraint::Length(8)])
            .areas(line_at(inner, 3));
        let focused = state.focus == 1;
        frame.render_widget(
            Line::from(vec![
                Span::styled(if focused { "▸ " } else { "  " }, theme.key()),
                Span::styled("Time", if focused { theme.bold() } else { theme.dim() }),
            ]),
            label,
        );
        state.time.render(
            frame,
            input,
            if focused {
                theme.bold().add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(theme.fg)
            },
            focused,
            "HH:MM",
            theme.dim(),
        );
    } else {
        let region = SUN_REGIONS
            .iter()
            .find(|(k, ..)| *k == app.sun_location.region)
            .map(|(_, l, ..)| *l)
            .unwrap_or("Unknown");
        choice_row(
            frame,
            &theme,
            line_at(inner, 3),
            "Region",
            region,
            state.focus == 1,
        );
        let today = local_now();
        let preview =
            resolve_trigger_time_today(&state.entry, today.day_of_year, today.utc_offset_minutes);
        let text = match preview {
            Ok(Some((hour, minute))) => format!(
                "                Today's {}: {:02}:{:02}",
                trigger_label.to_lowercase(),
                hour,
                minute
            ),
            Ok(None) => format!(
                "                No {} today at this latitude.",
                trigger_label.to_lowercase()
            ),
            Err(error) => format!("                {}", error),
        };
        frame.render_widget(Line::styled(text, theme.dim()), line_at(inner, 4));
    }

    let days_focused = state.focus == 2;
    let mut day_spans = vec![
        Span::styled(if days_focused { "▸ " } else { "  " }, theme.key()),
        Span::styled(
            format!("{:<14}", "Days"),
            if days_focused {
                theme.bold()
            } else {
                theme.dim()
            },
        ),
    ];
    for (index, day) in WEEKDAYS.iter().enumerate() {
        let on = state.entry.days.iter().any(|d| d == day);
        let mut style = if on {
            theme.accent().add_modifier(Modifier::BOLD)
        } else {
            theme.dim()
        };
        if days_focused && index == state.day_cursor {
            style = style.add_modifier(Modifier::REVERSED);
        }
        day_spans.push(Span::styled(format!(" {} ", weekday_label(day)), style));
    }
    frame.render_widget(Line::from(day_spans), line_at(inner, 6));
    frame.render_widget(
        Line::styled(
            format!("                {}", format_days(&state.entry.days)),
            theme.dim(),
        ),
        line_at(inner, 7),
    );
    choice_row(
        frame,
        &theme,
        line_at(inner, 9),
        "Enabled",
        if state.entry.enabled { "Yes" } else { "No" },
        state.focus == 3,
    );

    let mut info = Vec::new();
    if let Some(at) = state.entry.last_run_at {
        let ok = state.entry.last_run_status.as_deref() == Some("success");
        info.push(Line::from(vec![
            Span::styled("Last run: ", theme.dim()),
            Span::styled(
                format!(
                    "{} {}",
                    if ok { "✓ success" } else { "✗ failed" },
                    relative_time(at)
                ),
                if ok { theme.ok() } else { theme.err() },
            ),
        ]));
        if let Some(output) = &state.entry.last_run_output {
            info.push(Line::styled(truncate(output, 200), theme.dim()));
        }
    } else {
        info.push(Line::styled(platform::SCHEDULER_NOTE, theme.dim()));
    }
    frame.render_widget(
        Paragraph::new(info).wrap(Wrap { trim: true }),
        Rect {
            y: inner.y + 11,
            height: 3,
            ..inner
        },
    );
    if !state.error.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(format!("✗ {}", state.error), theme.err()))
                .wrap(Wrap { trim: true }),
            Rect {
                y: inner.y + 14,
                height: 2,
                ..inner
            },
        );
    }
    let mut pairs = vec![
        ("↑↓", "field"),
        ("←→", "change"),
        ("␣/1-7", "day"),
        ("a", "every day"),
        ("⏎", "save"),
    ];
    if state.exists {
        pairs.push(("^D", "remove"));
    }
    pairs.push(("Esc", "cancel"));
    frame.render_widget(
        hint(&theme, &pairs),
        line_at(inner, inner.height.saturating_sub(1)),
    );
}

fn draw_run(frame: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let Some(run) = &app.run else { return };
    let status = if run.running {
        if run.cancel_requested {
            "Stopping"
        } else {
            "Running"
        }
    } else if run.succeeded() {
        "Finished"
    } else if run.cancel_requested {
        "Stopped"
    } else {
        "Failed"
    };
    let elapsed = run
        .finished
        .unwrap_or_else(std::time::Instant::now)
        .duration_since(run.started)
        .as_secs();
    let inner = open_modal(
        frame,
        &theme,
        area,
        110,
        32,
        format!("{}: {}  ({}s)", status, truncate(&run.name, 40), elapsed),
    );
    let steps_height = (run.steps.len() as u16).min(10).max(1);
    let [steps_area, _, output_title, output_area, message, footer] = Layout::vertical([
        Constraint::Length(steps_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(inner);

    let rows: Vec<Row> = run
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let (icon, style) = match step.status {
                StepStatus::Pending => ("○".to_string(), theme.dim()),
                StepStatus::Running => (
                    SPINNER[(app.tick as usize) % SPINNER.len()].to_string(),
                    theme.accent(),
                ),
                StepStatus::Success => ("✓".to_string(), theme.ok()),
                StepStatus::Error => ("✗".to_string(), theme.err()),
                StepStatus::Skipped => ("–".to_string(), theme.dim()),
            };
            Row::new(vec![
                Cell::from(icon).style(style),
                Cell::from(format!("{}.", index + 1)).style(theme.dim()),
                Cell::from(step.label.clone()).style(if step.status == StepStatus::Skipped {
                    theme.dim()
                } else {
                    Style::default().fg(theme.fg)
                }),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Min(10),
        ],
    )
    .column_spacing(1)
    .row_highlight_style(theme.selected())
    .highlight_symbol("▸ ");
    let mut state = TableState::default().with_selected(Some(run.selected));
    frame.render_stateful_widget(table, steps_area, &mut state);

    let selected = run.steps.get(run.selected);
    frame.render_widget(
        Line::from(vec![
            Span::styled(
                format!("Output of step {}", run.selected + 1),
                theme.title(),
            ),
            Span::styled(if run.follow { "  (following)" } else { "" }, theme.dim()),
        ]),
        output_title,
    );
    let output = selected
        .map(|step| {
            if step.output.is_empty() {
                match step.status {
                    StepStatus::Pending => "Not started yet.".to_string(),
                    StepStatus::Running => "Running…".to_string(),
                    StepStatus::Skipped => "Skipped.".to_string(),
                    _ => String::new(),
                }
            } else {
                step.output.clone()
            }
        })
        .unwrap_or_default();
    let output_style = match selected.map(|s| s.status) {
        Some(StepStatus::Error) => theme.err(),
        _ => Style::default().fg(theme.fg),
    };
    frame.render_widget(
        Paragraph::new(Text::styled(output, output_style))
            .wrap(Wrap { trim: false })
            .scroll((run.scroll, 0))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(theme.border()),
            ),
        output_area,
    );
    if !run.error.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(format!("✗ {}", run.error), theme.err()))
                .wrap(Wrap { trim: true }),
            message,
        );
    } else if !run.running {
        frame.render_widget(Line::styled("✓ All steps completed.", theme.ok()), message);
    }
    let pairs: Vec<(&str, &str)> = if run.running {
        vec![
            ("s", "stop"),
            ("↑↓", "step"),
            ("PgUp/PgDn", "scroll"),
            ("f", "follow"),
            ("Esc", "hide (keeps running)"),
        ]
    } else {
        vec![
            ("r", "run again"),
            ("↑↓", "step"),
            ("PgUp/PgDn", "scroll"),
            ("Esc", "close"),
        ]
    };
    frame.render_widget(hint(&theme, &pairs), footer);
}

// Very small inline markup for help texts: `code` is highlighted.
fn help_line(theme: &Theme, text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, part) in text.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        if index % 2 == 1 {
            spans.push(Span::styled(part.to_string(), theme.key()));
        } else {
            spans.push(Span::styled(
                part.to_string(),
                Style::default().fg(theme.fg),
            ));
        }
    }
    Line::from(spans)
}

fn draw_help(frame: &mut Frame, theme: &Theme, area: Rect, topic: usize, step: usize) {
    let inner = open_modal(frame, theme, area, 92, 26, "Help".to_string());
    let [tabs, _, heading, blurb, _, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner);
    let mut spans = Vec::new();
    for (index, item) in TOPICS.iter().enumerate() {
        let style = if index == topic {
            theme.selected()
        } else {
            theme.dim()
        };
        spans.push(Span::styled(
            format!(" {} {} ", index + 1, item.label),
            style,
        ));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Line::from(spans), tabs);
    let current = &TOPICS[topic];
    let (title, text) = current.steps[step.min(current.steps.len() - 1)];
    frame.render_widget(
        Line::from(vec![
            Span::styled(title.to_string(), theme.title()),
            Span::styled(
                format!("   {}/{}", step + 1, current.steps.len()),
                theme.dim(),
            ),
        ]),
        heading,
    );
    frame.render_widget(Line::styled(current.blurb, theme.dim()), blurb);
    let lines: Vec<Line> = text
        .split('\n')
        .map(|line| help_line(theme, line))
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
    let mut pairs = vec![("←→", "page"), ("Tab/1-4", "topic")];
    if current.id == "support" {
        pairs.push(("s", "sponsor"));
    }
    pairs.push(("Esc", "close"));
    frame.render_widget(hint(theme, &pairs), footer);
}
