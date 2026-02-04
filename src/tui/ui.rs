//! UI rendering for TUI mode
//!
//! Uses ratatui to render tab bar and content area.

use crate::tui::app::{App, TabStatus};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs, Widget};

/// Render the entire TUI
pub fn render(frame: &mut Frame, app: &App) {
    // Layout: tab bar (3 lines including borders) + content (remaining)
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(frame.area());

    render_tab_bar(frame, app, chunks[0]);
    render_content(frame, app, chunks[1]);
}

/// Render the tab bar
fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let status_indicator = match tab.status {
                TabStatus::Running => "",
                TabStatus::TaskComplete => " [done]",
                TabStatus::AllComplete => " [ALL]",
                TabStatus::Stopped => " [X]",
            };

            let style = if i == app.active_tab_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            Line::from(Span::styled(
                format!("{}{}", tab.id, status_indicator),
                style,
            ))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" hydra tui - Ctrl+Q to quit "),
        )
        .select(app.active_tab_index)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" | ");

    frame.render_widget(tabs, area);
}

/// Convert vt100::Color to ratatui::Color
fn convert_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Widget that renders a vt100 screen to a ratatui buffer
struct Vt100Widget<'a> {
    screen: &'a vt100::Screen,
}

impl Widget for Vt100Widget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (rows, cols) = self.screen.size();

        for row in 0..rows.min(area.height) {
            for col in 0..cols.min(area.width) {
                if let Some(cell) = self.screen.cell(row, col) {
                    // Skip wide character continuations (second cell of wide char)
                    if cell.is_wide_continuation() {
                        continue;
                    }

                    let contents = cell.contents();
                    let char_to_draw = if contents.is_empty() {
                        ' '
                    } else {
                        contents.chars().next().unwrap_or(' ')
                    };

                    // Build style from cell attributes
                    let mut style = Style::default()
                        .fg(convert_color(cell.fgcolor()))
                        .bg(convert_color(cell.bgcolor()));

                    let mut modifier = Modifier::empty();
                    if cell.bold() {
                        modifier |= Modifier::BOLD;
                    }
                    if cell.italic() {
                        modifier |= Modifier::ITALIC;
                    }
                    if cell.underline() {
                        modifier |= Modifier::UNDERLINED;
                    }
                    if cell.inverse() {
                        modifier |= Modifier::REVERSED;
                    }
                    style = style.add_modifier(modifier);

                    // Write to ratatui buffer
                    let buf_cell = buf.cell_mut((area.x + col, area.y + row));
                    if let Some(buf_cell) = buf_cell {
                        buf_cell.set_char(char_to_draw).set_style(style);
                    }
                }
            }
        }
    }
}

/// Render the content area (active tab's output)
fn render_content(frame: &mut Frame, app: &App, area: Rect) {
    let status_text = if let Some(tab) = app.active_tab() {
        match tab.status {
            TabStatus::Running => " Running ",
            TabStatus::TaskComplete => " Task Complete ",
            TabStatus::AllComplete => " All Tasks Complete ",
            TabStatus::Stopped => " Stopped ",
        }
    } else {
        ""
    };

    let block = Block::default().borders(Borders::ALL).title(format!(
        " Tab {} {}",
        app.active_tab().map(|t| t.id).unwrap_or(0),
        status_text
    ));

    // Calculate inner area (inside the borders)
    let inner_area = block.inner(area);

    // Render the block border
    frame.render_widget(block, area);

    if let Some(tab) = app.active_tab() {
        // Render vt100 screen contents directly to the inner area
        let widget = Vt100Widget {
            screen: tab.parser.screen(),
        };
        frame.render_widget(widget, inner_area);
    } else {
        // No active tab - show help message
        let help_text = "No active tab. Press Ctrl+T to create one.";
        if inner_area.width > 0 && inner_area.height > 0 {
            let buf = frame.buffer_mut();
            for (i, ch) in help_text.chars().enumerate() {
                if i as u16 >= inner_area.width {
                    break;
                }
                if let Some(cell) = buf.cell_mut((inner_area.x + i as u16, inner_area.y)) {
                    cell.set_char(ch);
                }
            }
        }
    }
}
