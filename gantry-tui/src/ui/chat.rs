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
    streaming_content: Option<String>,
}

impl<'a> Chat<'a> {
    const USER_PREFIX: &'static str = "> ";
    const ASSISTANT_PREFIX: &'static str = "< ";

    pub fn new(messages: &'a [Message], streaming_content: Option<String>) -> Self {
        Self {
            messages,
            streaming_content,
        }
    }

    fn calc_msg_height(content: &str, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        let width = width as usize;
        let mut line_count = 0usize;

        for line in content.split('\n') {
            let char_count = line.chars().count();
            // Keep explicit blank lines and account for soft wrapping.
            line_count += if char_count == 0 {
                1
            } else {
                char_count.div_ceil(width)
            };
        }

        line_count.max(1) as u16
    }

    fn streaming_message_idx(&self) -> Option<usize> {
        self.messages
            .iter()
            .rposition(|m| m.role == Role::Assistant)
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

        let gap = 1;

        let total_height: u16 = self
            .messages
            .iter()
            .map(|m| {
                let text_width = match m.role {
                    Role::User => area
                        .width
                        .saturating_sub(1 + Self::USER_PREFIX.len() as u16),
                    Role::Assistant => area
                        .width
                        .saturating_sub(1 + Self::ASSISTANT_PREFIX.len() as u16),
                    _ => area.width.saturating_sub(2),
                };
                Self::calc_msg_height(&m.content, text_width)
            })
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

            let text_width = match message.role {
                Role::User => area
                    .width
                    .saturating_sub(1 + Self::USER_PREFIX.len() as u16),
                Role::Assistant => area
                    .width
                    .saturating_sub(1 + Self::ASSISTANT_PREFIX.len() as u16),
                _ => area.width.saturating_sub(2),
            };

            let msg_height = Self::calc_msg_height(&message.content, text_width);

            if y + msg_height > area.bottom() {
                break;
            }

            let is_last_assistant = message.role == Role::Assistant
                && self.streaming_content.is_some()
                && self
                    .streaming_message_idx()
                    .map(|idx| message.content == self.messages[idx].content)
                    .unwrap_or(false);

            let mut content = match message.role {
                Role::User => message.content.clone(),
                Role::Assistant => message.content.clone(),
                Role::Error => message.content.clone(),
            };

            if is_last_assistant {
                content.push('▍');
            }

            let style = match message.role {
                Role::User => Style::default().fg(ratatui::style::Color::White),
                Role::Assistant => Style::default(),
                Role::Error => Style::default().fg(ratatui::style::Color::Red),
            };

            if message.role == Role::User {
                let prefix_x = area.x + 1;
                let text_x = prefix_x + Self::USER_PREFIX.len() as u16;
                let text_area = Rect::new(
                    text_x,
                    y,
                    area.width
                        .saturating_sub(1 + Self::USER_PREFIX.len() as u16),
                    msg_height,
                );
                buf.set_string(
                    prefix_x,
                    y,
                    Self::USER_PREFIX,
                    Style::default().fg(ratatui::style::Color::LightGreen),
                );

                let paragraph = Paragraph::new(Text::raw(&content))
                    .style(style)
                    .wrap(ratatui::widgets::Wrap { trim: false });
                paragraph.render(text_area, buf);
            } else if message.role == Role::Assistant {
                let prefix_x = area.x + 1;
                let text_x = prefix_x + Self::ASSISTANT_PREFIX.len() as u16;
                let text_area = Rect::new(
                    text_x,
                    y,
                    area.width
                        .saturating_sub(1 + Self::ASSISTANT_PREFIX.len() as u16),
                    msg_height,
                );
                buf.set_string(
                    prefix_x,
                    y,
                    Self::ASSISTANT_PREFIX,
                    Style::default().fg(ratatui::style::Color::DarkGray),
                );

                let paragraph = Paragraph::new(Text::raw(&content))
                    .style(style)
                    .wrap(ratatui::widgets::Wrap { trim: false });
                paragraph.render(text_area, buf);
            } else {
                let paragraph = Paragraph::new(Text::raw(&content))
                    .style(style)
                    .wrap(ratatui::widgets::Wrap { trim: false });
                let msg_area = Rect::new(area.x + 1, y, area.width.saturating_sub(1), msg_height);
                paragraph.render(msg_area, buf);
            }

            y += msg_height + gap;
        }
    }
}
