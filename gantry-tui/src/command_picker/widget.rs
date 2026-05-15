use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::picker::highlight_matched_chars;
use crate::theme;
use crate::widgets::table::TableWidget;
use crate::{command_picker::CommandPickerState, theme::title};

/// Fixed column gutter between the command name column and the description column.
const COLUMN_GAP: u16 = 4;

const MAX_VISIBLE: usize = 10;

/// Overhead rows: top border + prompt row + blank row + counter row + bottom border.
const CHROME_HEIGHT: u16 = 5;

const STYLE_TEXT: Style = Style::new().fg(Color::White);
const STYLE_MATCH: Style = Style::new().fg(Color::LightCyan);
const STYLE_SELECTED: Style = Style::new().fg(Color::LightCyan).bold();
const STYLE_DESC: Style = Style::new().fg(Color::Gray);

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
            .title(title("COMMANDS"))
            .borders(Borders::ALL)
            .border_set(theme::border_set())
            .border_style(Style::default().fg(Color::Gray));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let list_height = filtered.len().clamp(1, MAX_VISIBLE) as u16;
        let [prompt_area, _, list_area, counter_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(list_height),
                Constraint::Length(1),
            ])
            .areas(inner);

        Line::from(format!("> {}", picker.filter)).render(prompt_area, buf);

        if filtered.is_empty() {
            Line::styled("No matches", Style::default().fg(Color::DarkGray)).render(list_area, buf);
            return;
        }

        let selected = picker.selected_idx;
        let count = filtered.len();
        let max_visible = (list_area.height as usize).min(MAX_VISIBLE);

        // Bottom-anchor scroll: the selected item sits at the bottom of the visible window
        // until doing so would extend past the end of the list, at which point the window
        // is pinned to show the last `max_visible` items.
        let start = selected
            .saturating_sub(max_visible - 1)
            .min(count.saturating_sub(max_visible));

        let rows: Vec<Vec<Line>> = filtered
            .iter()
            .enumerate()
            .skip(start)
            .take(max_visible)
            .map(|(i, entry)| {
                let is_selected = i == selected;
                let cmd = &picker.items[entry.idx];
                let name_line = if is_selected {
                    Line::from(Span::styled(cmd.name.as_str(), STYLE_SELECTED))
                } else {
                    highlight_matched_chars(&cmd.name, &entry.indices, STYLE_TEXT, STYLE_MATCH)
                };
                let desc_line = Line::from(Span::styled(cmd.description.as_str(), STYLE_DESC));
                vec![name_line, desc_line]
            })
            .collect();

        TableWidget::new(vec![self.state.cmd_col_width], COLUMN_GAP, rows).render(list_area, buf);

        theme::counter_line(selected + 1, count).render(counter_area, buf);
    }
}
