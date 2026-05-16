use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use super::CommandPickerState;
use crate::theme;
use crate::theme::title;
use crate::utils::highlight_matched_chars;
use crate::widgets::table::TableWidget;

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
        CHROME_HEIGHT + self.state.picker.matched_count().clamp(1, MAX_VISIBLE) as u16
    }
}

impl Widget for CommandPickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let picker = &self.state.picker;
        let count = picker.matched_count();

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

        let list_height = count.clamp(1, MAX_VISIBLE) as u16;
        let [prompt_area, _, list_area, counter_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(list_height),
                Constraint::Length(1),
            ])
            .areas(inner);

        Line::from(format!("> {}", picker.filter())).render(prompt_area, buf);

        if count == 0 {
            Line::styled("No matches", Style::default().fg(Color::DarkGray)).render(list_area, buf);
            return;
        }

        let selected = picker.cursor();
        let max_visible = (list_area.height as usize).min(MAX_VISIBLE);

        // Bottom-anchor scroll: the selected item sits at the bottom of the visible window
        // until doing so would extend past the end of the list, at which point the window
        // is pinned to show the last `max_visible` items.
        let start = selected
            .saturating_sub(max_visible - 1)
            .min(count.saturating_sub(max_visible));

        let rows: Vec<Vec<Line>> = picker
            .matched_items()
            .enumerate()
            .skip(start)
            .take(max_visible)
            .map(|(i, matched)| {
                let is_selected = i == selected;
                let name_line = if is_selected {
                    Line::from(Span::styled(matched.item.name(), STYLE_SELECTED))
                } else {
                    highlight_matched_chars(
                        matched.item.name(),
                        matched.match_positions,
                        STYLE_TEXT,
                        STYLE_MATCH,
                    )
                };
                let desc_line = Line::from(Span::styled(matched.item.description(), STYLE_DESC));
                vec![name_line, desc_line]
            })
            .collect();

        TableWidget::new(vec![self.state.cmd_col_width], COLUMN_GAP, rows).render(list_area, buf);

        theme::counter_line(selected + 1, count).render(counter_area, buf);
    }
}
