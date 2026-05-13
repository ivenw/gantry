use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::command_picker::CommandPicker;
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

pub struct CommandPickerView<'a> {
    state: &'a CommandPicker,
}

impl<'a> CommandPickerView<'a> {
    /// Creates a `CommandPickerView` from picker state.
    pub fn new(state: &'a CommandPicker) -> Self {
        Self { state }
    }

    /// Returns the total height needed to render the picker.
    pub fn height(&self) -> u16 {
        CHROME_HEIGHT + self.state.filtered.len().clamp(1, MAX_VISIBLE) as u16
    }
}

impl Widget for CommandPickerView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let filtered = &self.state.filtered;

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

        let prompt = format!("> {}", self.state.filter);
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

        let selected = self.state.selected_idx;
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
            .map(|(i, cmd)| {
                let is_selected = i == selected;
                let name_line = if is_selected {
                    Line::from(Span::styled(cmd.name.clone(), STYLE_SELECTED))
                } else {
                    highlighted_line(&cmd.name, &cmd.indices, STYLE_TEXT, STYLE_MATCH)
                };
                let desc_line = Line::from(Span::styled(cmd.description.clone(), STYLE_DESC));
                vec![name_line, desc_line]
            })
            .collect();

        TableView::new(vec![self.state.cmd_col_width], CMD_DESC_GAP, rows).render(list, buf);
    }
}
