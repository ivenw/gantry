use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

use crate::model::SessionsView;

pub struct SessionsViewWidget<'a> {
    state: &'a SessionsView,
}

impl<'a> SessionsViewWidget<'a> {
    /// Creates a widget for the sessions browser overlay.
    pub fn new(state: &'a SessionsView) -> Self {
        Self { state }
    }
}

impl Widget for SessionsViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::NONE);
        block.render(area, buf);

        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let footer_height = 1u16;
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(footer_height)])
            .split(inner);

        let list_area = chunks[0];
        let footer_area = chunks[1];

        let viewport_height = list_area.height as usize;
        let sessions = &self.state.sessions;
        let selected = self.state.selected_idx;

        // Keep selected row in view.
        let scroll = if selected < viewport_height {
            0
        } else {
            selected.saturating_sub(viewport_height - 1)
        };

        for (i, session) in sessions.iter().enumerate() {
            let row = i.wrapping_sub(scroll);
            if i < scroll || row >= viewport_height {
                continue;
            }
            let y = list_area.y + row as u16;

            let is_selected = i == selected;
            let is_active = session.id == self.state.active_session_id;

            let base_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            // Fill the entire row background when selected.
            if is_selected {
                for x in 0..list_area.width {
                    if let Some(cell) = buf.cell_mut((list_area.x + x, y)) {
                        cell.set_style(base_style);
                    }
                }
            }

            let active_marker = if is_active { ">" } else { " " };
            let ts = session.timestamp.strftime("%Y-%m-%d %H:%M").to_string();
            let id_short = session.id.to_string();
            // Show the last 8 chars of the UUID so it's recognisable but compact.
            let id_suffix = &id_short[id_short.len().saturating_sub(8)..];
            let line = format!("{} {}  …{}", active_marker, ts, id_suffix);

            buf.set_string(list_area.x, y, &line, base_style);

            // Paint the active marker in cyan even when not selected.
            if is_active && !is_selected {
                buf.set_string(
                    list_area.x,
                    y,
                    active_marker,
                    Style::default().fg(Color::Cyan),
                );
            }
        }

        if sessions.is_empty() {
            buf.set_string(
                list_area.x,
                list_area.y,
                "No sessions",
                Style::default().fg(Color::DarkGray),
            );
        }

        let footer = " ↑↓ navigate   Enter resume   Esc cancel ";
        buf.set_string(
            footer_area.x,
            footer_area.y,
            footer,
            Style::default().fg(Color::DarkGray),
        );
    }
}
