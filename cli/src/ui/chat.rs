use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Text,
    widgets::{Paragraph, Widget},
};

use crate::ui::{Message, Role};

pub struct Chat<'a> {
    messages: &'a [Message],
}

impl<'a> Chat<'a> {
    pub fn new(messages: &'a [Message]) -> Self {
        Self { messages }
    }

    fn calc_msg_height(content: &str, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        let char_count = content.chars().count();
        let wrapped_lines = (char_count + width as usize - 1) / width as usize;
        wrapped_lines as u16
    }
}

impl<'a> Widget for Chat<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.messages.is_empty() {
            let text = "Type a message and press Enter to start...";
            let x = area.x + (area.width.saturating_sub(text.len() as u16)) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, text, Style::default());
            return;
        }

        let text_width = area.width.saturating_sub(2);
        let gap = 1;

        let total_height: u16 = self
            .messages
            .iter()
            .map(|m| Self::calc_msg_height(&m.content, text_width))
            .sum();

        let total_with_gaps = total_height + ((self.messages.len() as u16 - 1) * gap);
        let start_y = if total_with_gaps < area.height {
            area.bottom().saturating_sub(total_with_gaps)
        } else {
            area.y
        };

        let visible_messages: Vec<Message> = self.messages.to_vec();
        let mut y = start_y;

        for message in visible_messages {
            if y >= area.bottom() {
                break;
            }

            let msg_height = Self::calc_msg_height(&message.content, text_width);

            if y + msg_height > area.bottom() {
                break;
            }

            let content = match message.role {
                Role::User => format!("│ {}", message.content),
                Role::Assistant => message.content.clone(),
            };

            let style = match message.role {
                Role::User => Style::default().fg(ratatui::style::Color::White),
                Role::Assistant => Style::default(),
            };

            let paragraph = Paragraph::new(Text::raw(&content))
                .style(style)
                .wrap(ratatui::widgets::Wrap { trim: false });

            let msg_area = Rect::new(area.x + 1, y, area.width - 1, msg_height);
            paragraph.render(msg_area, buf);

            if message.role == Role::User {
                buf.get_mut(area.x + 1, y)
                    .set_style(Style::default().fg(ratatui::style::Color::LightGreen));
            }

            y += msg_height + gap;
        }
    }
}
