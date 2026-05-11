use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::model::ModelPickerView;

pub struct ModelPickerViewWidget<'a> {
    state: &'a ModelPickerView,
}

impl<'a> ModelPickerViewWidget<'a> {
    pub fn new(state: &'a ModelPickerView) -> Self {
        Self { state }
    }
}

impl Widget for ModelPickerViewWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Model ")
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(Color::DarkGray));
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

        let footer_y = inner.bottom().saturating_sub(1);
        let list_area = Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        );

        if self.state.models.is_empty() {
            buf.set_string(
                list_area.x,
                list_area.y,
                "No models available",
                Style::default().fg(Color::DarkGray),
            );
        } else {
            for (i, selection) in self.state.models.iter().enumerate() {
                if list_area.y + i as u16 >= list_area.bottom() {
                    break;
                }
                let y = list_area.y + i as u16;
                let is_cursor = i == self.state.selected_idx;
                let is_active = self.state.active_selection.as_ref() == Some(selection);
                let style = if is_cursor {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else if is_active {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                let label = format!(
                    "{}  {}",
                    selection.provider.as_str(),
                    selection.model.as_str()
                );
                let padded = format!("{:<width$}", label, width = inner.width as usize);
                buf.set_string(list_area.x, y, &padded, style);
            }
        }

        let footer = " ↑↓ navigate   Enter select   Esc close ";
        buf.set_string(
            inner.x,
            footer_y,
            footer,
            Style::default().fg(Color::DarkGray),
        );
    }
}
