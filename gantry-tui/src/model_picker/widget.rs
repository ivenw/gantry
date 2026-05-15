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
use crate::{
    model_picker::{ModelPickerState, format_context_length},
    theme::title,
};

pub const MAX_VISIBLE: usize = 10;

/// Overhead rows: top border + prompt row + blank separator + counter row + bottom border.
const CHROME_HEIGHT: u16 = 5;

const STYLE_TEXT: Style = Style::new().fg(Color::White);
const STYLE_MATCH: Style = Style::new().fg(Color::LightCyan);
const STYLE_SELECTED: Style = Style::new().fg(Color::LightCyan).bold();
const STYLE_ACTIVE: Style = Style::new().fg(Color::LightCyan);
const STYLE_PROVIDER: Style = Style::new().fg(Color::DarkGray);

pub struct ModelPickerWidget<'a> {
    state: &'a ModelPickerState,
}

impl<'a> ModelPickerWidget<'a> {
    /// Creates a `ModelPickerWidget` from picker state.
    pub fn new(state: &'a ModelPickerState) -> Self {
        Self { state }
    }

    /// Returns the total height needed to render the picker.
    pub fn height(&self) -> u16 {
        CHROME_HEIGHT + self.state.picker.filtered.len().clamp(1, MAX_VISIBLE) as u16
    }
}

impl Widget for ModelPickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let picker = &self.state.picker;
        let filtered = &picker.filtered;

        let block = Block::default()
            .title(title("MODELS"))
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
                let model_entry = &picker.items[entry.idx];
                let model_str = model_entry.selection.model_id.as_str().to_owned();
                let model_line = if is_selected {
                    Line::from(Span::styled(model_str, STYLE_SELECTED))
                } else if model_entry.is_active {
                    Line::from(Span::styled(model_str, STYLE_ACTIVE))
                } else {
                    highlight_matched_chars(&model_str, &entry.indices, STYLE_TEXT, STYLE_MATCH)
                };
                let provider_line = Line::from(Span::styled(
                    model_entry.selection.provider_alias.as_str().to_owned(),
                    STYLE_PROVIDER,
                ));
                let context_line = Line::from(Span::styled(
                    model_entry
                        .selection
                        .context_length
                        .map(format_context_length)
                        .unwrap_or_default(),
                    STYLE_PROVIDER,
                ));
                vec![model_line, provider_line, context_line]
            })
            .collect();

        TableWidget::new(
            vec![self.state.model_col_width, self.state.provider_col_width],
            12,
            rows,
        )
        .render(list_area, buf);

        theme::counter_line(selected + 1, count).render(counter_area, buf);
    }
}
