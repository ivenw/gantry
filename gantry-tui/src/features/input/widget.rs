use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

use crate::features::input::InputState;
use crate::model::Mode;
use crate::theme;
use crate::utils::wrapped_line_count;

const PREFIX: &str = "> ";
const PREFIX_WIDTH: u16 = PREFIX.len() as u16;
const BORDER_HEIGHT: u16 = 2;

pub struct InputWidget<'a> {
    state: &'a InputState,
    project_root: &'a Path,
    /// Number of trailing bytes in the raw display string that belong to an active picker filter.
    /// These characters are rendered in LightYellow to indicate the pending picker state.
    picker_filter_len: usize,
    mode: Mode,
}

impl<'a> InputWidget<'a> {
    /// Creates an `InputWidget` from the input model, the project root for path display, the
    /// current mode for border coloring, and the active picker filter byte length for highlighting.
    pub fn new(
        state: &'a InputState,
        project_root: &'a Path,
        mode: Mode,
        picker_filter_len: usize,
    ) -> Self {
        Self {
            state,
            project_root,
            picker_filter_len,
            mode,
        }
    }

    /// Returns the widget height required to fit the content within `width` terminal columns.
    pub fn height(&self, width: u16) -> u16 {
        let raw_display = self.state.raw_display(self.project_root);
        let text_width = width.saturating_sub(PREFIX_WIDTH).max(1) as usize;
        let lines = wrapped_line_count(&raw_display, text_width);
        (lines as u16 + BORDER_HEIGHT).max(3)
    }

    /// Returns `(col, row)` of the cursor within the text area.
    fn calc_cursor_pos(raw_display: &str, cursor: usize, text_width: usize) -> (u16, u16) {
        let mut col = 0usize;
        let mut row = 0usize;

        for (i, c) in raw_display.char_indices() {
            if i == cursor {
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
}

impl Widget for InputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use gantry_core::InputToken;

        let mode_color = theme::mode_color(self.mode);
        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_set(theme::border_set())
            .border_style(Style::default().fg(mode_color))
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
            Style::default().fg(mode_color),
        );

        let text_area = Rect::new(
            content_area.x + PREFIX_WIDTH,
            content_area.y,
            content_area.width.saturating_sub(PREFIX_WIDTH),
            content_area.height,
        );

        let text_width = text_area.width as usize;

        let (raw_display, cursor) = self.state.display_with_cursor(self.project_root);

        // Render tokens one span at a time, tracking col/row to handle wrapping.
        // Trailing picker_filter_len bytes of the raw display string are highlighted as pending filter input.
        let filter_start_byte = raw_display.len().saturating_sub(self.picker_filter_len);
        let mut raw_byte = 0usize;
        let mut col = 0usize;
        let mut row = 0usize;
        let mut sigil_buf;
        for token in &self.state.tokens {
            let (text, base_style) = match token {
                InputToken::Text(t) => (t.as_str(), Style::default().fg(Color::White)),
                InputToken::Path(p) => {
                    let rel = p.strip_prefix(self.project_root).unwrap_or(p);
                    sigil_buf = format!("+{}", rel.display());
                    (sigil_buf.as_str(), Style::default().fg(Color::LightYellow))
                }
                InputToken::Skill { name, .. } => {
                    sigil_buf = format!("/{}", name);
                    (sigil_buf.as_str(), Style::default().fg(Color::LightYellow))
                }
            };

            for c in text.chars() {
                let style = if self.picker_filter_len > 0 && raw_byte >= filter_start_byte {
                    Style::default().fg(Color::LightYellow)
                } else {
                    base_style
                };
                raw_byte += c.len_utf8();

                if row >= text_area.height as usize {
                    break;
                }
                if c == '\n' {
                    row += 1;
                    col = 0;
                    continue;
                }
                if col >= text_width {
                    row += 1;
                    col = 0;
                }
                let cx = text_area.x + col as u16;
                let cy = text_area.y + row as u16;
                if cx < text_area.right()
                    && cy < text_area.bottom()
                    && let Some(cell) = buf.cell_mut((cx, cy))
                {
                    cell.set_char(c).set_style(style);
                }
                col += 1;
            }
        }

        let (cur_col, cur_row) = Self::calc_cursor_pos(&raw_display, cursor, text_width);
        let cursor_x = text_area.x + cur_col;
        let cursor_y = text_area.y + cur_row;

        if cursor_x < text_area.right()
            && cursor_y < text_area.bottom()
            && let Some(cell) = buf.cell_mut((cursor_x, cursor_y))
        {
            cell.set_style(Style::default().fg(Color::Black).bg(Color::White));
        }
    }
}
