use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Borders, Widget},
};

pub struct Input<'a> {
    value: &'a str,
}

impl<'a> Input<'a> {
    pub fn new(value: &'a str) -> Self {
        Self { value }
    }

    pub fn calc_height(value: &str, width: u16) -> u16 {
        if value.is_empty() {
            return 3;
        }
        let text_width = (width - 4).max(1) as usize;
        let line_count = text_width.max(1);
        let char_count = value.chars().count();
        let wrapped_lines = (char_count + line_count - 1) / line_count;
        (wrapped_lines as u16 + 2).max(3)
    }

    pub fn calc_cursor_pos(value: &str, width: u16) -> (u16, u16) {
        let text_width = (width - 4).max(1) as usize;
        let mut col = 0usize;
        let mut row = 0usize;

        for c in value.chars() {
            if c == '\n' {
                row += 1;
                col = 0;
            } else if col >= text_width {
                row += 1;
                col = 0;
                if c != ' ' {
                    col = 1;
                }
            } else {
                col += 1;
            }
        }

        (col as u16, row as u16)
    }
}

impl<'a> Widget for Input<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner_area = Rect::new(area.x + 2, area.y + 1, area.width - 4, area.height - 2);

        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
            .render(area, buf);

        if !self.value.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(self.value)
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::White))
                .wrap(ratatui::widgets::Wrap { trim: false });
            paragraph.render(inner_area, buf);
        }

        let (col, row) = Self::calc_cursor_pos(self.value, area.width);
        let cursor_x = inner_area.x + col;
        let cursor_y = inner_area.y + row;

        if cursor_x < inner_area.right() && cursor_y < inner_area.bottom() {
            if let Some(cell) = buf.cell_mut((cursor_x, cursor_y)) {
                cell.set_char('█')
                    .set_style(ratatui::style::Style::default().fg(ratatui::style::Color::White));
            }
        }
    }
}
