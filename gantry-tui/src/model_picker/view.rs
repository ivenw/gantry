use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use crate::model_picker::{ModelPickerView, format_context_length};
use crate::theme;
use crate::views::table::{TableView, highlighted_line};

pub const MAX_VISIBLE: usize = 10;

const STYLE_TEXT: Style = Style::new().fg(Color::White);
const STYLE_MATCH: Style = Style::new().fg(Color::LightCyan);
const STYLE_SELECTED: Style = Style::new().fg(Color::LightCyan).bold();
const STYLE_ACTIVE: Style = Style::new().fg(Color::LightCyan);
const STYLE_PROVIDER: Style = Style::new().fg(Color::DarkGray);

/// Overhead rows: top border + search line + blank separator + bottom border.
const CHROME_HEIGHT: u16 = 4;

pub struct ModelPickerWidget<'a> {
    state: &'a ModelPickerView,
}

impl<'a> ModelPickerWidget<'a> {
    /// Creates a `ModelPickerWidget` from picker state.
    pub fn new(state: &'a ModelPickerView) -> Self {
        Self { state }
    }

    /// Returns the total height needed to render the picker.
    pub fn height(&self) -> u16 {
        CHROME_HEIGHT + self.state.filtered.len().clamp(1, MAX_VISIBLE) as u16
    }
}

impl Widget for ModelPickerWidget<'_> {
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
            .map(|(i, entry)| {
                let is_cursor = i == selected;
                let model_str = entry.selection.model_id.as_str().to_owned();
                let model_line = if is_cursor {
                    Line::from(Span::styled(model_str, STYLE_SELECTED))
                } else if entry.is_active {
                    Line::from(Span::styled(model_str, STYLE_ACTIVE))
                } else {
                    highlighted_line(&model_str, &entry.indices, STYLE_TEXT, STYLE_MATCH)
                };
                let provider_line = Line::from(Span::styled(
                    entry.selection.provider_alias.as_str().to_owned(),
                    STYLE_PROVIDER,
                ));
                let context_line = Line::from(Span::styled(
                    entry
                        .selection
                        .context_length
                        .map(format_context_length)
                        .unwrap_or_default(),
                    STYLE_PROVIDER,
                ));
                vec![model_line, provider_line, context_line]
            })
            .collect();

        TableView::new(
            vec![self.state.model_col_width, self.state.provider_col_width],
            12,
            rows,
        )
        .render(list, buf);
    }
}
