use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::model::{AttachmentPicker, AttachmentPickerKind};

const COLOR_TEXT: Color = Color::Gray;
const COLOR_MATCH: Color = Color::LightYellow;

pub struct AttachmentPickerView<'a> {
    state: &'a AttachmentPicker,
    project_root: &'a Path,
}

impl<'a> AttachmentPickerView<'a> {
    /// Creates an `AttachmentPickerView` from picker state and the project root for path display.
    pub fn new(state: &'a AttachmentPicker, project_root: &'a Path) -> Self {
        Self {
            state,
            project_root,
        }
    }
}

impl Widget for AttachmentPickerView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let max_visible = area.height as usize;
        let selected = self.state.selected_idx;
        let count = self.state.len();

        // Scroll window: keep selected_idx visible.
        let start = if count <= max_visible {
            0
        } else if selected + 1 > max_visible {
            selected + 1 - max_visible
        } else {
            0
        };

        struct Row {
            label: String,
            indices: Vec<u32>,
        }

        let rows: Vec<Row> = match &self.state.kind {
            AttachmentPickerKind::Path(results) => results
                .iter()
                .skip(start)
                .take(max_visible)
                .map(|r| Row {
                    label: r
                        .path
                        .strip_prefix(self.project_root)
                        .unwrap_or(&r.path)
                        .display()
                        .to_string(),
                    indices: r.indices.clone(),
                })
                .collect(),
            AttachmentPickerKind::Skill(results) => results
                .iter()
                .skip(start)
                .take(max_visible)
                .map(|r| Row {
                    label: r.skill.metadata.name.clone(),
                    indices: r.indices.clone(),
                })
                .collect(),
        };

        if rows.is_empty() {
            buf.set_string(
                area.x,
                area.y,
                "No results",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        for (row_idx, row) in rows.iter().enumerate() {
            let abs_idx = start + row_idx;
            let is_selected = abs_idx == selected;

            if is_selected {
                // Selected row: solid highlight, no per-character styling.
                let padded = format!("{:<width$}", row.label, width = area.width as usize);
                buf.set_string(
                    area.x,
                    area.y + row_idx as u16,
                    &padded,
                    Style::default().bold().fg(Color::LightYellow),
                );
            } else {
                // Unselected row: render character-by-character, highlighting matches.
                let padded = format!("{:<width$}", row.label, width = area.width as usize);
                for (char_idx, ch) in padded.chars().enumerate() {
                    let style = if row.indices.contains(&(char_idx as u32)) {
                        Style::default().fg(COLOR_MATCH)
                    } else {
                        Style::default().fg(COLOR_TEXT)
                    };
                    buf.set_string(
                        area.x + char_idx as u16,
                        area.y + row_idx as u16,
                        &ch.to_string(),
                        style,
                    );
                }
            }
        }
    }
}
