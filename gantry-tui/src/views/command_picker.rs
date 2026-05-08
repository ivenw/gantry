use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::commands::MAX_CMD_NAME_LEN;
use crate::model::CommandPicker;

/// Minimum spaces between the end of a command name and the start of its description.
const CMD_DESC_GAP: usize = 12;

/// Column offset at which descriptions start, relative to the list area.
const DESC_COL: u16 = (MAX_CMD_NAME_LEN + CMD_DESC_GAP) as u16;

pub struct CommandPickerView<'a> {
    state: &'a CommandPicker,
}

impl<'a> CommandPickerView<'a> {
    pub fn new(state: &'a CommandPicker) -> Self {
        Self { state }
    }

    /// Calculates the total height needed to render the picker at the given width.
    pub fn calc_height(&self, width: u16) -> u16 {
        let filtered = self.state.filtered_commands();
        // Available width for descriptions: subtract borders and the name column.
        let desc_width = (width.saturating_sub(2) as usize)
            .saturating_sub(DESC_COL as usize)
            .max(1);

        let list_height: u16 = if filtered.is_empty() {
            1
        } else {
            filtered
                .iter()
                .map(|cmd| {
                    let desc_len = cmd.description.len();
                    let wrapped = if desc_len == 0 {
                        1
                    } else {
                        desc_len.div_ceil(desc_width)
                    };
                    wrapped.max(1) as u16
                })
                .sum()
        };

        // border top + list rows + border bottom
        list_height + 2
    }
}

impl Widget for CommandPickerView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let filtered = self.state.filtered_commands();

        let title = if self.state.filter.is_empty() {
            if filtered.is_empty() {
                " No commands ".to_string()
            } else {
                " Commands ".to_string()
            }
        } else {
            format!(" Commands: {} ", self.state.filter)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(title);
        block.render(area, buf);

        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let desc_col_x = inner.x + DESC_COL;
        let desc_width = (inner.width as usize)
            .saturating_sub(DESC_COL as usize)
            .max(1);
        let mut y = inner.y;

        for (i, cmd) in filtered.iter().enumerate() {
            if y >= inner.bottom() {
                break;
            }

            let is_selected = i == self.state.selected_idx;
            let name_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::LightGreen)
            } else {
                Style::default().fg(Color::White)
            };
            let desc_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::LightGreen)
            } else {
                Style::default().fg(Color::White)
            };

            // Pad the name to fill the name column so the selection highlight is uniform.
            let padded_name = format!("{:<width$}", cmd.name, width = MAX_CMD_NAME_LEN);
            buf.set_string(inner.x, y, &padded_name, name_style);

            // For selected rows, fill the gap between name and description columns.
            if is_selected {
                let gap_x = inner.x + MAX_CMD_NAME_LEN as u16;
                let gap_width = DESC_COL.saturating_sub(MAX_CMD_NAME_LEN as u16) as usize;
                buf.set_string(gap_x, y, &" ".repeat(gap_width), desc_style);
            }

            let desc_chunks: Vec<&str> = if cmd.description.is_empty() {
                vec![""]
            } else {
                cmd.description
                    .as_bytes()
                    .chunks(desc_width)
                    .map(|c| unsafe { std::str::from_utf8_unchecked(c) })
                    .collect()
            };

            for (j, chunk) in desc_chunks.iter().enumerate() {
                if y >= inner.bottom() {
                    break;
                }
                // Continuation lines re-indent to the description column.
                if j > 0 {
                    buf.set_string(inner.x, y, &" ".repeat(DESC_COL as usize), desc_style);
                }
                buf.set_string(desc_col_x, y, chunk, desc_style);
                y += 1;
            }
        }
    }
}
