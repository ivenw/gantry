use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Borders, Widget},
};

const PREFIX: &str = "> ";
const PREFIX_WIDTH: u16 = PREFIX.len() as u16;
const BORDER_HEIGHT: u16 = 2;

pub struct InputView<'a> {
    value: &'a str,
    cursor: usize,
}

impl<'a> InputView<'a> {
    pub fn new(value: &'a str, cursor: usize) -> Self {
        Self { value, cursor }
    }

    pub fn calc_height(&self, width: u16) -> u16 {
        let text_width = width.saturating_sub(PREFIX_WIDTH).max(1) as usize;
        let wrapped_lines = Self::wrapped_line_count(self.value, text_width);
        (wrapped_lines as u16 + BORDER_HEIGHT).max(3)
    }

    /// Returns (col, row) of the cursor within the text area.
    fn calc_cursor_pos(&self, text_width: usize) -> (u16, u16) {
        let mut col = 0usize;
        let mut row = 0usize;

        for (i, c) in self.value.char_indices() {
            if i == self.cursor {
                break;
            }
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

impl Widget for InputView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_type(BorderType::LightDoubleDashed)
            .border_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray))
            .render(area, buf);

        let content_area = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(BORDER_HEIGHT),
        );
        if content_area.width == 0 || content_area.height == 0 {
            return;
        }

        buf.set_string(
            content_area.x,
            content_area.y,
            PREFIX,
            ratatui::style::Style::default().fg(ratatui::style::Color::LightGreen),
        );

        let text_area = Rect::new(
            content_area.x + PREFIX_WIDTH,
            content_area.y,
            content_area.width.saturating_sub(PREFIX_WIDTH),
            content_area.height,
        );

        if !self.value.is_empty() {
            let paragraph = ratatui::widgets::Paragraph::new(self.value)
                .style(ratatui::style::Style::default().fg(ratatui::style::Color::White))
                .wrap(ratatui::widgets::Wrap { trim: false });
            paragraph.render(text_area, buf);
        }

        let text_width = text_area.width as usize;
        let (col, row) = self.calc_cursor_pos(text_width);
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
