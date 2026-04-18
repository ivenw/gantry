use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Padding, Paragraph, Widget},
};

pub struct StatusMessageView<'a> {
    message: &'a str,
}

impl<'a> StatusMessageView<'a> {
    pub fn new(message: &'a str) -> Self {
        Self { message }
    }
}

impl Widget for StatusMessageView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.message)
            .style(Style::default().fg(Color::Gray))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
