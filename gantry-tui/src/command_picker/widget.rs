use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::command_picker::CommandPickerState;
use crate::theme;
use crate::widgets::table::{TableView, highlighted_line};

/// Minimum spaces between the end of a command name and the start of its description.
const CMD_DESC_GAP: u16 = 12;

const MAX_VISIBLE: usize = 10;

const STYLE_TEXT: Style = Style::new().fg(Color::White);
const STYLE_MATCH: Style = Style::new().fg(Color::LightCyan);
const STYLE_SELECTED: Style = Style::new().fg(Color::LightCyan).bold();
const STYLE_DESC: Style = Style::new().fg(Color::Gray);

/// Overhead rows: top border + search line + blank separator + bottom border.
const CHROME_HEIGHT: u16 = 4;

pub struct CommandPickerWidget<'a> {
    state: &'a CommandPickerState,
}

impl<'a> CommandPickerWidget<'a> {
    /// Creates a `CommandPickerWidget` from picker state.
    pub fn new(state: &'a CommandPickerState) -> Self {
        Self { state }
    }

    /// Returns the total height needed to render the picker.
    pub fn height(&self) -> u16 {
        CHROME_HEIGHT + self.state.picker.filtered.len().clamp(1, MAX_VISIBLE) as u16
    }
}

impl Widget for CommandPickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let picker = &self.state.picker;
        let filtered = &picker.filtered;

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(theme::border_set())
            .border_style(Style::default().fg(Color::Gray));
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

        let prompt = format!("> {}", picker.filter);
        buf.set_string(inner.x, inner.y, &prompt, STYLE_TEXT);

        // inner.y + 1 is the blank separator; list starts at inner.y + 2.
        let list = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );

        if list.height == 0 {
            return;
        }

        if filtered.is_empty() {
            buf.set_string(
                list.x,
                list.y,
                "No matches",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        let selected = picker.selected_idx;
        let count = filtered.len();
        let max_visible = list.height as usize;

        // Scroll window: keep selected_idx visible.
        let start = if count <= max_visible {
            0
        } else {
            (selected + 1).saturating_sub(max_visible)
        };

        let rows: Vec<Vec<Line>> = filtered
            .iter()
            .enumerate()
            .skip(start)
            .take(max_visible)
            .map(|(i, entry)| {
                let is_selected = i == selected;
                let cmd = &entry.item;
                let name_line = if is_selected {
                    Line::from(Span::styled(cmd.name.clone(), STYLE_SELECTED))
                } else {
                    highlighted_line(&cmd.name, &entry.indices, STYLE_TEXT, STYLE_MATCH)
                };
                let desc_line = Line::from(Span::styled(cmd.description.clone(), STYLE_DESC));
                vec![name_line, desc_line]
            })
            .collect();

        TableView::new(vec![self.state.cmd_col_width], CMD_DESC_GAP, rows).render(list, buf);
    }
}
