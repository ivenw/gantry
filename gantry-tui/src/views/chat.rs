use crate::model::ChatMessage;
use ratatui::{
    buffer::Buffer,
    layout::{Margin, Rect},
    style::Style,
    symbols::scrollbar::Set as ScrollbarSet,
    text::Text,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};

const USER_PREFIX: &str = ">> ";
const REASONING_PREFIX: &str = "** ";
const ASSISTANT_PREFIX: &str = "<< ";
const TOOL_CALL_PREFIX: &str = ".. ";

pub struct ChatView<'a> {
    pub messages: &'a [ChatMessage],
    pub scroll_offset: u16,
    /// Current spinner character, shared with the statusline throbber.
    pub spinner: char,
}

#[derive(Default)]
pub struct ChatViewState {
    pub scrollbar: ScrollbarState,
    pub max_scroll: u16,
}

impl ChatView<'_> {
    pub fn calc_msg_height(content: &str, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        let width = width as usize;
        let mut line_count = 0usize;

        for line in content.split('\n') {
            let char_count = line.chars().count();
            line_count += if char_count == 0 {
                1
            } else {
                char_count.div_ceil(width)
            };
        }

        line_count.max(1) as u16
    }
}

impl StatefulWidget for ChatView<'_> {
    type State = ChatViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let messages = self.messages;

        if messages.is_empty() {
            let text = "Type a message and press Enter to start...";
            let x = area.x + (area.width.saturating_sub(text.len() as u16)) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, text, Style::default());
            return;
        }

        let gap = 1u16;

        let heights: Vec<u16> = messages
            .iter()
            .map(|m| {
                let prefix_len = match m {
                    ChatMessage::User { sender, .. } => {
                        USER_PREFIX.len()
                            + sender.as_ref().map(|s| s.as_str().len() + 2).unwrap_or(0)
                    }
                    ChatMessage::Reasoning { .. } => REASONING_PREFIX.len(),
                    ChatMessage::Assistant { .. } => ASSISTANT_PREFIX.len(),
                    ChatMessage::ToolCall { name, .. } => TOOL_CALL_PREFIX.len() + name.len() + 1,
                };
                let content = msg_content(m);
                let text_width = area.width.saturating_sub(prefix_len as u16);
                Self::calc_msg_height(content, text_width)
            })
            .collect();

        let total_content: u16 =
            heights.iter().sum::<u16>() + (messages.len() as u16).saturating_sub(1) * gap;

        let max_scroll = total_content.saturating_sub(area.height);
        let clamped_offset = self.scroll_offset.min(max_scroll);
        let scroll = max_scroll - clamped_offset;

        let virtual_start: u16 = area.height.saturating_sub(total_content);

        let mut vline = virtual_start;

        for (i, message) in messages.iter().enumerate() {
            let msg_height = heights[i];
            let vline_end = vline + msg_height;

            if vline_end <= scroll {
                vline += msg_height + gap;
                continue;
            }

            if vline >= scroll + area.height {
                break;
            }

            let clip_top = scroll.saturating_sub(vline);
            let visible_lines =
                (msg_height - clip_top).min(scroll + area.height - (vline + clip_top));

            let screen_y = area.y + (vline + clip_top).saturating_sub(scroll);
            let content = msg_content(message);

            match message {
                ChatMessage::User { sender, .. } => {
                    let prefix = match sender {
                        Some(id) => format!("{} {} ", id.as_str(), USER_PREFIX.trim_end()),
                        None => USER_PREFIX.to_string(),
                    };
                    let prefix_len = prefix.chars().count() as u16;
                    let text_width = area.width.saturating_sub(prefix_len);
                    let text_area =
                        Rect::new(area.x + prefix_len, screen_y, text_width, visible_lines);
                    buf.set_string(
                        area.x,
                        screen_y,
                        &prefix,
                        Style::default().fg(ratatui::style::Color::LightGreen),
                    );
                    Paragraph::new(Text::raw(content))
                        .style(Style::default().fg(ratatui::style::Color::White))
                        .wrap(ratatui::widgets::Wrap { trim: false })
                        .scroll((clip_top, 0))
                        .render(text_area, buf);
                }
                ChatMessage::Reasoning { .. } => {
                    let text_width = area.width.saturating_sub(REASONING_PREFIX.len() as u16);
                    let text_area = Rect::new(
                        area.x + REASONING_PREFIX.len() as u16,
                        screen_y,
                        text_width,
                        visible_lines,
                    );
                    buf.set_string(
                        area.x,
                        screen_y,
                        REASONING_PREFIX,
                        Style::default().fg(ratatui::style::Color::DarkGray),
                    );
                    Paragraph::new(Text::raw(content))
                        .style(Style::default().fg(ratatui::style::Color::DarkGray))
                        .wrap(ratatui::widgets::Wrap { trim: false })
                        .scroll((clip_top, 0))
                        .render(text_area, buf);
                }
                ChatMessage::Assistant { .. } => {
                    let text_width = area.width.saturating_sub(ASSISTANT_PREFIX.len() as u16);
                    let text_area = Rect::new(
                        area.x + ASSISTANT_PREFIX.len() as u16,
                        screen_y,
                        text_width,
                        visible_lines,
                    );
                    buf.set_string(
                        area.x,
                        screen_y,
                        ASSISTANT_PREFIX,
                        Style::default().fg(ratatui::style::Color::DarkGray),
                    );
                    Paragraph::new(Text::raw(content))
                        .style(Style::default())
                        .wrap(ratatui::widgets::Wrap { trim: false })
                        .scroll((clip_top, 0))
                        .render(text_area, buf);
                }
                ChatMessage::ToolCall { name, done, .. } => {
                    let indicator = if *done {
                        "✓"
                    } else {
                        &self.spinner.to_string()
                    };
                    let line = format!("{}{} {}", TOOL_CALL_PREFIX, indicator, name);
                    buf.set_string(
                        area.x,
                        screen_y,
                        &line,
                        Style::default().fg(if *done {
                            ratatui::style::Color::DarkGray
                        } else {
                            ratatui::style::Color::Cyan
                        }),
                    );
                }
            }

            vline += msg_height + gap;
        }

        state.max_scroll = max_scroll;

        if max_scroll > 0 {
            state.scrollbar = state
                .scrollbar
                .content_length(max_scroll as usize)
                .position(scroll as usize);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .symbols(ScrollbarSet {
                    track: "",
                    thumb: "▌",
                    begin: "",
                    end: "",
                })
                .thumb_style(Style::default().fg(ratatui::style::Color::DarkGray));
            StatefulWidget::render(
                scrollbar,
                area.inner(Margin {
                    vertical: 0,
                    horizontal: 0,
                }),
                buf,
                &mut state.scrollbar,
            );
        }
    }
}

fn msg_content(message: &ChatMessage) -> &str {
    match message {
        ChatMessage::User { content, .. }
        | ChatMessage::Reasoning { content }
        | ChatMessage::Assistant { content } => content.trim(),
        ChatMessage::ToolCall { .. } => "",
    }
}
