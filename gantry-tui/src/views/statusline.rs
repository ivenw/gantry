use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Padding, Paragraph, Widget},
};

pub struct StatuslineView<'a> {
    status: Option<&'a str>,
}

impl<'a> StatuslineView<'a> {
    pub fn new(status: Option<&'a str>) -> Self {
        Self { status }
    }
}

impl Widget for StatuslineView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = self.status.unwrap_or("<statusline_placeholder>");
        Paragraph::new(text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
