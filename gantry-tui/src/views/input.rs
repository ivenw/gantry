use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

use crate::{
    model::{InputMode, InputModel},
    theme,
};

const PREFIX: &str = ">> ";
const PREFIX_WIDTH: u16 = PREFIX.len() as u16;
const BORDER_HEIGHT: u16 = 2;

pub struct InputView<'a> {
    state: &'a InputModel,
    project_root: &'a Path,
    /// Byte offset of the cursor within the raw display string.
    cursor: usize,
    /// Raw display string (sigils inlined, paths stripped to relative).
    raw_display: String,
    /// Number of trailing bytes in the raw display string that belong to an active picker filter.
    /// These characters are rendered in LightYellow to indicate the pending picker state.
    picker_filter_len: usize,
    mode: InputMode,
}

impl<'a> InputView<'a> {
    /// Creates an `InputView` from the input model and the project root for path display.
    pub fn new(state: &'a InputModel, project_root: &'a Path) -> Self {
        let (raw_display, cursor) = state.display_with_cursor(project_root);
        Self {
            state,
            project_root,
            cursor,
            raw_display,
            picker_filter_len: 0,
            mode: InputMode::Normal,
        }
    }

    /// Sets the input mode, used to color the border.
    pub fn with_mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the number of trailing bytes that represent an active picker filter, for highlight rendering.
    pub fn with_picker_filter_len(mut self, len: usize) -> Self {
        self.picker_filter_len = len;
        self
    }

    /// Returns the widget height required to fit the content within `width` terminal columns.
    pub fn height(&self, width: u16) -> u16 {
        let text_width = width.saturating_sub(PREFIX_WIDTH).max(1) as usize;
        let wrapped_lines = Self::wrapped_line_count(&self.raw_display, text_width);
        (wrapped_lines as u16 + BORDER_HEIGHT).max(3)
    }

    /// Returns `(col, row)` of the cursor within the text area.
    fn calc_cursor_pos(&self, text_width: usize) -> (u16, u16) {
        let mut col = 0usize;
        let mut row = 0usize;

        for (i, c) in self.raw_display.char_indices() {
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
        use gantry_core::InputToken;

        let border_color = match self.mode {
            InputMode::Normal => Color::DarkGray,
            InputMode::Insert => Color::LightGreen,
        };
        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_set(theme::border_set())
            .border_style(Style::default().fg(border_color))
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
            Style::default().fg(Color::LightGreen),
        );

        let text_area = Rect::new(
            content_area.x + PREFIX_WIDTH,
            content_area.y,
            content_area.width.saturating_sub(PREFIX_WIDTH),
            content_area.height,
        );

        let text_width = text_area.width as usize;

        // Render tokens one span at a time, tracking col/row to handle wrapping.
        // Trailing picker_filter_len bytes of the raw display string are highlighted as pending filter input.
        let filter_start_byte = self.raw_display.len().saturating_sub(self.picker_filter_len);
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

        let (cur_col, cur_row) = self.calc_cursor_pos(text_width);
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
