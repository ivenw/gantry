use gantry_core::DiffHunk;

use super::{AttachmentLabel, ChatMessage, ChatState};
use crate::utils::wrapped_line_count;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    symbols::scrollbar::Set as ScrollbarSet,
    text::{Line, Span, Text},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};

const USER_PREFIX: &str = "> ";
const REASONING_PREFIX: &str = "* ";
const ASSISTANT_PREFIX: &str = "< ";
const TOOL_SUCCESS_INDICATOR: &str = "+";
const TOOL_ERROR_INDICATOR: &str = "-";

pub struct ChatWidget<'a> {
    state: &'a ChatState,
    spinner: char,
}

impl<'a> ChatWidget<'a> {
    /// Creates a new chat widget for the given chat state and spinner frame.
    pub fn new(state: &'a ChatState, spinner: char) -> Self {
        Self { state, spinner }
    }

    fn calc_msg_height(content: &str, width: u16) -> u16 {
        if width == 0 {
            return 1;
        }
        wrapped_line_count(content, width as usize) as u16
    }
}

#[derive(Default)]
pub struct ChatWidgetState {
    pub scrollbar: ScrollbarState,
    pub max_scroll: u16,
}

impl StatefulWidget for ChatWidget<'_> {
    type State = ChatWidgetState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let messages = &self.state.messages;

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
            .map(|m| match m {
                ChatMessage::ToolCall {
                    name,
                    arguments,
                    hunks,
                    ..
                } => tool_call_line_count(name, arguments, hunks),
                ChatMessage::User {
                    sender,
                    content,
                    attachments,
                    ..
                } => {
                    let prefix_len = USER_PREFIX.len()
                        + sender.as_ref().map(|s| s.as_str().len() + 2).unwrap_or(0);
                    let text_width = area.width.saturating_sub(prefix_len as u16);
                    let text_lines = Self::calc_msg_height(content, text_width);
                    text_lines + attachments.len() as u16
                }
                ChatMessage::Reasoning { content } => {
                    let text_width = area.width.saturating_sub(REASONING_PREFIX.len() as u16);
                    Self::calc_msg_height(content, text_width)
                }
                ChatMessage::Assistant { content } => {
                    let text_width = area.width.saturating_sub(ASSISTANT_PREFIX.len() as u16);
                    Self::calc_msg_height(content, text_width)
                }
            })
            .collect();

        let total_content: u16 =
            heights.iter().sum::<u16>() + (messages.len() as u16).saturating_sub(1) * gap;

        let max_scroll = total_content.saturating_sub(area.height);
        let clamped_offset = self.state.scroll_offset.min(max_scroll);
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

            match message {
                ChatMessage::User {
                    sender,
                    attachments,
                    content,
                    ..
                } => {
                    let prefix = match sender {
                        Some(id) => format!("{} {} ", id.as_str(), USER_PREFIX.trim_end()),
                        None => USER_PREFIX.to_string(),
                    };
                    let prefix_len = prefix.chars().count() as u16;
                    let text_width = area.width.saturating_sub(prefix_len);
                    let text_line_count = Self::calc_msg_height(content, text_width);
                    let attach_count = attachments.len() as u16;

                    // The full message block covers text lines then attachment label lines.
                    // Clip it to the visible window before splitting into sub-areas.
                    let msg_area = Rect::new(area.x, screen_y, area.width, visible_lines);
                    let [text_area, attach_area] = Layout::vertical([
                        Constraint::Length(text_line_count.saturating_sub(clip_top)),
                        Constraint::Length(attach_count),
                    ])
                    .areas(msg_area);

                    if text_area.height > 0 {
                        render_prefix(
                            &prefix,
                            ratatui::style::Color::LightGreen,
                            area.x,
                            buf,
                            screen_y,
                        );
                        let inner = Rect::new(
                            text_area.x + prefix_len,
                            text_area.y,
                            text_width,
                            text_area.height,
                        );
                        Paragraph::new(Text::raw(content))
                            .style(Style::default().fg(ratatui::style::Color::White))
                            .wrap(ratatui::widgets::Wrap { trim: false })
                            .scroll((clip_top, 0))
                            .render(inner, buf);
                    }

                    for (i, attachment) in attachments.iter().enumerate() {
                        let row = Rect::new(
                            attach_area.x,
                            attach_area.y + i as u16,
                            attach_area.width,
                            1,
                        );
                        if row.y >= area.y + area.height {
                            break;
                        }
                        let text = match attachment {
                            AttachmentLabel::Skill(name) => format!("  - skill {}", name),
                            AttachmentLabel::File(path) => format!("  - read {}", path),
                            AttachmentLabel::Dir(path) => format!("  - listed {}", path),
                        };
                        Paragraph::new(Line::from(Span::styled(
                            text,
                            Style::default().fg(ratatui::style::Color::DarkGray),
                        )))
                        .render(row, buf);
                    }
                }
                ChatMessage::Reasoning { content } => {
                    let prefix_len = REASONING_PREFIX.len() as u16;
                    let text_area = Rect::new(
                        area.x + prefix_len,
                        screen_y,
                        area.width.saturating_sub(prefix_len),
                        visible_lines,
                    );
                    render_prefix(
                        REASONING_PREFIX,
                        ratatui::style::Color::DarkGray,
                        area.x,
                        buf,
                        screen_y,
                    );
                    Paragraph::new(Text::raw(content.as_str()))
                        .style(Style::default().fg(ratatui::style::Color::DarkGray))
                        .wrap(ratatui::widgets::Wrap { trim: false })
                        .scroll((clip_top, 0))
                        .render(text_area, buf);
                }
                ChatMessage::Assistant { content } => {
                    let prefix_len = ASSISTANT_PREFIX.len() as u16;
                    let text_area = Rect::new(
                        area.x + prefix_len,
                        screen_y,
                        area.width.saturating_sub(prefix_len),
                        visible_lines,
                    );
                    render_prefix(
                        ASSISTANT_PREFIX,
                        ratatui::style::Color::DarkGray,
                        area.x,
                        buf,
                        screen_y,
                    );
                    // tui_markdown only applies style spans and does not reflow or change
                    // line/column counts, so wrapped_line_count gives the correct height.
                    Paragraph::new(tui_markdown::from_str(content))
                        .wrap(ratatui::widgets::Wrap { trim: false })
                        .scroll((clip_top, 0))
                        .render(text_area, buf);
                }
                ChatMessage::ToolCall {
                    name,
                    arguments,
                    done,
                    is_error,
                    hunks,
                    ..
                } => {
                    let indicator = match (done, is_error) {
                        (false, _) => self.spinner.to_string(),
                        (true, false) => TOOL_SUCCESS_INDICATOR.to_string(),
                        (true, true) => TOOL_ERROR_INDICATOR.to_string(),
                    };
                    let line = format_tool_call_line(&indicator, name, arguments, hunks);
                    let color = match (done, is_error) {
                        (false, _) => ratatui::style::Color::Cyan,
                        (true, false) => ratatui::style::Color::DarkGray,
                        (true, true) => ratatui::style::Color::Red,
                    };
                    let tool_area = Rect::new(area.x, screen_y, area.width, visible_lines);
                    Paragraph::new(Text::raw(line))
                        .style(Style::default().fg(color))
                        .scroll((clip_top, 0))
                        .render(tool_area, buf);
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
            StatefulWidget::render(scrollbar, area, buf, &mut state.scrollbar);
        }
    }
}

/// Returns the number of display lines a tool call occupies without allocating its full string.
fn tool_call_line_count(name: &str, arguments: &serde_json::Value, hunks: &[DiffHunk]) -> u16 {
    let cmd_lines = if name == "bash" {
        arguments
            .get("command")
            .and_then(|v| v.as_str())
            .map(|cmd| split_on_unescaped_and(cmd).len() as u16)
            .unwrap_or(1)
    } else {
        1
    };
    // One line for the main display line, plus one per hunk header.
    cmd_lines + hunks.len() as u16
}

/// Builds the display line for a tool call, including optional diff summary and hunk headers.
fn format_tool_call_line(
    indicator: &str,
    name: &str,
    arguments: &serde_json::Value,
    hunks: &[DiffHunk],
) -> String {
    let raw_arg = tool_display_arg(name, arguments);
    let display_name = if name == "bash" { "$" } else { name };
    let formatted_arg = raw_arg.map(|a| {
        if name == "bash" {
            format_bash_command(&a)
        } else {
            a
        }
    });
    let mut line = match &formatted_arg {
        Some(a) => format!("{} {} {}", indicator, display_name, a),
        None => format!("{} {}", indicator, display_name),
    };
    if !hunks.is_empty() {
        line.push(' ');
        line.push_str(&format_diff_summary(hunks));
    }
    for hunk in hunks {
        line.push('\n');
        line.push_str(&format_hunk_header(hunk));
    }
    line
}

/// Returns a short display string for the most informative argument of a known tool.
///
/// For bash commands the raw command string is returned; callers should pass it through
/// [`format_bash_command`] for display. For the read tool, optional `offset` and `limit`
/// values are appended as `@<offset>` and `+<limit>`. For the write tool, the line count
/// of the written content is appended as `<N>L`.
fn tool_display_arg(name: &str, args: &serde_json::Value) -> Option<String> {
    let key = match name {
        "bash" => "command",
        "read_file" | "write_file" | "edit_file" => "path",
        _ => return None,
    };
    let path = args.get(key)?.as_str()?;
    match name {
        "read_file" => {
            let mut s = path.to_string();
            if let Some(offset) = args.get("offset").and_then(|v| v.as_u64()) {
                s.push_str(&format!(" @{}", offset));
            }
            if let Some(limit) = args.get("limit").and_then(|v| v.as_u64()) {
                s.push_str(&format!(" +{}", limit));
            }
            Some(s)
        }
        "write_file" => {
            let mut s = path.to_string();
            if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
                let line_count = content.lines().count().max(1);
                s.push_str(&format!(" {}L", line_count));
            }
            Some(s)
        }
        "edit_file" => {
            let s = path.to_string();
            Some(s)
        }
        _ => Some(path.to_string()),
    }
}

/// Formats a bash command for display by splitting on unescaped `&&` operators.
///
/// Each part is placed on its own line with `  && ` as a continuation prefix so long
/// chained commands are easier to read at a glance. `&&` inside single or double quotes,
/// or immediately preceded by a backslash (`\&&`), is left untouched.
fn format_bash_command(cmd: &str) -> String {
    let parts = split_on_unescaped_and(cmd);
    if parts.len() == 1 {
        return parts.into_iter().next().unwrap().trim().to_string();
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            out.push_str(part.trim());
        } else {
            out.push_str("\n  && ");
            out.push_str(part.trim());
        }
    }
    out
}

/// Splits `cmd` on `&&` tokens that are not inside single/double quotes and not
/// preceded by a backslash.
fn split_on_unescaped_and(cmd: &str) -> Vec<&str> {
    let bytes = cmd.as_bytes();
    let len = bytes.len();
    let mut parts: Vec<&str> = Vec::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut segment_start = 0usize;
    let mut i = 0usize;

    while i < len {
        match bytes[i] {
            b'\\' => {
                // Skip the next character — it is escaped.
                i += 2;
                continue;
            }
            b'\'' if !in_double => {
                in_single = !in_single;
            }
            b'"' if !in_single => {
                in_double = !in_double;
            }
            b'&' if !in_single && !in_double => {
                if i + 1 < len && bytes[i + 1] == b'&' {
                    parts.push(&cmd[segment_start..i]);
                    // Advance past `&&`.
                    i += 2;
                    segment_start = i;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }

    parts.push(&cmd[segment_start..]);
    parts
}

/// Summarises a diff as `+N -N` for the total lines added and removed across all hunks.
fn format_diff_summary(hunks: &[DiffHunk]) -> String {
    let (added, removed) = hunks.iter().fold((0usize, 0usize), |(a, r), h| {
        (a + h.new_count(), r + h.old_count())
    });
    format!("+{}/-{}", added, removed)
}

/// Formats a diff hunk as a unified-diff header line: `@@ -old_start,old_count +new_start,new_count @@`.
fn format_hunk_header(hunk: &DiffHunk) -> String {
    format!(
        "  @@ -{},{} +{},{} @@",
        hunk.old_start,
        hunk.old_count(),
        hunk.new_start,
        hunk.new_count(),
    )
}

/// Renders a single-line message prefix (e.g. `"> "`, `"< "`) at column `x`.
fn render_prefix(
    prefix: &str,
    color: ratatui::style::Color,
    x: u16,
    buf: &mut Buffer,
    screen_y: u16,
) {
    let prefix_area = Rect::new(x, screen_y, prefix.chars().count() as u16, 1);
    Paragraph::new(Line::from(Span::styled(prefix, Style::default().fg(color))))
        .render(prefix_area, buf);
}
