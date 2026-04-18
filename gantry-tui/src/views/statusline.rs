use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Padding, Paragraph, Widget},
};

pub struct StatuslineView;

impl StatuslineView {
    pub fn new() -> Self {
        Self
    }
}

impl Widget for StatuslineView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new("<statusline_placeholder>")
            .style(Style::default().fg(Color::Gray))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
