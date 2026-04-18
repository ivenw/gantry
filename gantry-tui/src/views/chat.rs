use ratatui::{
    buffer::Buffer,
    layout::{Margin, Rect},
    style::Style,
    symbols::scrollbar::Set as ScrollbarSet,
    text::Text,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};

use gantry_core::{Message, Role};

const USER_PREFIX: &str = "> ";
const ASSISTANT_PREFIX: &str = "< ";

pub struct ChatView<'a> {
    pub messages: &'a [Message],
    pub scroll_offset: u16,
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
        if self.messages.is_empty() {
            let text = "Type a message and press Enter to start...";
            let x = area.x + (area.width.saturating_sub(text.len() as u16)) / 2;
            let y = area.y + area.height / 2;
            buf.set_string(x, y, text, Style::default());
            return;
        }

        let gap = 1u16;

        // Compute per-message heights.
        let heights: Vec<u16> = self
            .messages
            .iter()
            .map(|m| {
                let text_width = match m.role {
                    Role::User => area.width.saturating_sub(USER_PREFIX.len() as u16),
                    Role::Assistant => area.width.saturating_sub(ASSISTANT_PREFIX.len() as u16),
                    _ => area.width,
                };
                Self::calc_msg_height(&m.content, text_width)
            })
            .collect();

        let total_content: u16 =
            heights.iter().sum::<u16>() + (self.messages.len() as u16).saturating_sub(1) * gap;

        let max_scroll = total_content.saturating_sub(area.height);
        // scroll is the top-down viewport offset into virtual content.
        // scroll_offset=0 → pinned to bottom → scroll=max_scroll.
        // scroll_offset=max_scroll → scrolled to top → scroll=0.
        let clamped_offset = self.scroll_offset.min(max_scroll);
        let scroll = max_scroll - clamped_offset;

        // When content fits the viewport, pad from the top so messages sit at the bottom.
        let virtual_start: u16 = area.height.saturating_sub(total_content);

        // The viewport shows virtual lines [scroll, scroll + area.height).
        // Map a virtual line to a screen row: screen_row = area.y + virtual_line - scroll.
        // A message at virtual line `vline` with height `h` occupies virtual rows
        // [vline, vline + h).  We render only the portion that falls inside the viewport.

        let mut vline = virtual_start;

        for (i, message) in self.messages.iter().enumerate() {
            let msg_height = heights[i];

            let vline_end = vline + msg_height;

            // Skip messages entirely above the viewport.
            if vline_end <= scroll {
                vline += msg_height + gap;
                continue;
            }

            // Stop once past the bottom of the viewport.
            if vline >= scroll + area.height {
                break;
            }

            // Portion of this message visible in the viewport.
            let clip_top = scroll.saturating_sub(vline);
            let visible_lines =
                (msg_height - clip_top).min(scroll + area.height - (vline + clip_top));

            let screen_y = area.y + (vline + clip_top).saturating_sub(scroll);

            let content = message.content.clone();
            let style = match message.role {
                Role::User => Style::default().fg(ratatui::style::Color::White),
                Role::Assistant => Style::default(),
                Role::Error => Style::default().fg(ratatui::style::Color::Red),
            };

            if message.role == Role::User {
                let text_width = area.width.saturating_sub(USER_PREFIX.len() as u16);
                let text_area = Rect::new(
                    area.x + USER_PREFIX.len() as u16,
                    screen_y,
                    text_width,
                    visible_lines,
                );
                buf.set_string(
                    area.x,
                    screen_y,
                    USER_PREFIX,
                    Style::default().fg(ratatui::style::Color::LightGreen),
                );
                let paragraph = Paragraph::new(Text::raw(&content))
                    .style(style)
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .scroll((clip_top, 0));
                paragraph.render(text_area, buf);
            } else if message.role == Role::Assistant {
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
                let paragraph = Paragraph::new(Text::raw(&content))
                    .style(style)
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .scroll((clip_top, 0));
                paragraph.render(text_area, buf);
            } else {
                let msg_area = Rect::new(area.x, screen_y, area.width, visible_lines);
                let paragraph = Paragraph::new(Text::raw(&content))
                    .style(style)
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .scroll((clip_top, 0));
                paragraph.render(msg_area, buf);
            }

            vline += msg_height + gap;
        }

        // Write max_scroll back so update.rs can clamp scroll_offset.
        state.max_scroll = max_scroll;

        // Update and render the scrollbar whenever content overflows, regardless of scroll position.
        if max_scroll > 0 {
            // scroll is top-down; thumb at bottom when scroll_offset=0.
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
