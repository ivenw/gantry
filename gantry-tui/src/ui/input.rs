use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Borders, Widget},
};

pub struct Input<'a> {
    value: &'a str,
}

impl<'a> Input<'a> {
    const PREFIX: &'static str = "> ";

    pub fn new(value: &'a str) -> Self {
        Self { value }
    }

    pub fn calc_height(value: &str, width: u16) -> u16 {
        let text_width = width.saturating_sub(1 + Self::PREFIX.len() as u16).max(1) as usize;
        let wrapped_lines = Self::wrapped_line_count(value, text_width);
        (wrapped_lines as u16 + 2).max(3)
    }

    pub fn calc_cursor_pos(value: &str, width: u16) -> (u16, u16) {
        let text_width = width.saturating_sub(1 + Self::PREFIX.len() as u16).max(1) as usize;
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

    fn wrapped_line_count(value: &str, text_width: usize) -> usize {
        if value.is_empty() {
            return 1;
        }

        value
            .split('\n')
            .map(|line| {
                let char_count = line.chars().count();
                if char_count == 0 {
                    1
                } else {
                    char_count.div_ceil(text_width)
                }
            })
            .sum::<usize>()
            .max(1)
    }
}

impl<'a> Widget for Input<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner_area = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(1),
            area.height.saturating_sub(2),
        );
        let prefix_width = Self::PREFIX.len() as u16;
        let text_area = Rect::new(
            inner_area.x.saturating_add(prefix_width),
            inner_area.y,
            inner_area.width.saturating_sub(prefix_width),
            inner_area.height,
        );

        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::Plain)
            .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
            .render(area, buf);

        if inner_area.width > 0 && inner_area.height > 0 {
            buf.set_string(
                inner_area.x,
                inner_area.y,
                Self::PREFIX,
                ratatui::style::Style::default().fg(ratatui::style::Color::LightGreen),
            );
        }

        if !self.value.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(self.value)
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::White))
                .wrap(ratatui::widgets::Wrap { trim: false });
            paragraph.render(text_area, buf);
        }

        let (col, row) = Self::calc_cursor_pos(self.value, area.width);
        let cursor_x = text_area.x + col;
        let cursor_y = text_area.y + row;

        if cursor_x < text_area.right()
            && cursor_y < text_area.bottom()
            && let Some(cell) = buf.cell_mut((cursor_x, cursor_y))
        {
            cell.set_char('█')
                .set_style(ratatui::style::Style::default().fg(ratatui::style::Color::White));
        }
    }
}
