use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::model::CommandPicker;

/// The height of the filter input row inside the picker border.
const FILTER_ROW_HEIGHT: u16 = 1;

pub struct CommandPickerView<'a> {
    state: &'a CommandPicker,
}

impl<'a> CommandPickerView<'a> {
    pub fn new(state: &'a CommandPicker) -> Self {
        Self { state }
    }

    pub fn calc_height(&self, width: u16) -> u16 {
        let filtered = self.state.filtered_commands();
        let text_width = (width.saturating_sub(4)).max(1) as usize;

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
                        desc_len.div_ceil(text_width)
                    };
                    wrapped.max(1) as u16
                })
                .sum()
        };

        // border top + filter row + list rows + border bottom
        list_height + FILTER_ROW_HEIGHT + 2
    }
}

impl Widget for CommandPickerView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let filtered = self.state.filtered_commands();

        let title = if filtered.is_empty() {
            " No commands "
        } else {
            " Commands "
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

        // Render the filter row.
        let filter_display = format!("> {}", self.state.filter);
        buf.set_string(inner.x, inner.y, &filter_display, Style::default().fg(Color::LightGreen));

        let list_area = Rect::new(
            inner.x,
            inner.y + FILTER_ROW_HEIGHT,
            inner.width,
            inner.height.saturating_sub(FILTER_ROW_HEIGHT),
        );

        if list_area.height == 0 {
            return;
        }

        let text_width = list_area.width as usize;
        let mut y = list_area.y;

        for (i, cmd) in filtered.iter().enumerate() {
            if y >= list_area.bottom() {
                break;
            }

            let is_selected = i == self.state.selected_idx;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::LightGreen)
            } else {
                Style::default().fg(Color::White)
            };

            let line = format!("{} - {}", cmd.name, cmd.description);
            let wrapped_lines: Vec<&str> = if line.is_empty() {
                vec![""]
            } else {
                line.as_bytes()
                    .chunks(text_width)
                    .map(|c| unsafe { std::str::from_utf8_unchecked(c) })
                    .collect()
            };

            for (j, line_chunk) in wrapped_lines.iter().enumerate() {
                if y >= list_area.bottom() {
                    break;
                }
                let x = if j == 0 && is_selected {
                    list_area.x
                } else {
                    list_area.x + (cmd.name.len() as u16) + 3
                };
                buf.set_string(x, y, line_chunk, style);
                y += 1;
            }
        }
    }
}
