use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::model::{AttachmentPicker, AttachmentPickerKind};

pub struct AttachmentPickerView<'a> {
    state: &'a AttachmentPicker,
    project_root: &'a Path,
}

impl<'a> AttachmentPickerView<'a> {
    pub fn new(state: &'a AttachmentPicker, project_root: &'a Path) -> Self {
        Self { state, project_root }
    }

    /// Calculates the height required to render the picker.
    pub fn calc_height(&self) -> u16 {
        let rows = self.state.len().max(1).min(10) as u16;
        rows + 2 // borders
    }
}

impl Widget for AttachmentPickerView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = match &self.state.kind {
            AttachmentPickerKind::Path(_) => {
                if self.state.filter.is_empty() {
                    " + Files ".to_string()
                } else {
                    format!(" + Files: {} ", self.state.filter)
                }
            }
            AttachmentPickerKind::Skill(_) => {
                if self.state.filter.is_empty() {
                    " / Skills ".to_string()
                } else {
                    format!(" / Skills: {} ", self.state.filter)
                }
            }
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

        let max_visible = inner.height as usize;
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

        let labels: Vec<String> = match &self.state.kind {
            AttachmentPickerKind::Path(paths) => paths
                .iter()
                .skip(start)
                .take(max_visible)
                .map(|p| {
                    p.strip_prefix(self.project_root)
                        .unwrap_or(p)
                        .display()
                        .to_string()
                })
                .collect(),
            AttachmentPickerKind::Skill(skills) => skills
                .iter()
                .skip(start)
                .take(max_visible)
                .map(|s| s.metadata.name.clone())
                .collect(),
        };

        if labels.is_empty() {
            buf.set_string(
                inner.x,
                inner.y,
                "No results",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        for (row_idx, label) in labels.iter().enumerate() {
            let abs_idx = start + row_idx;
            let is_selected = abs_idx == selected;
            let style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::LightGreen)
            } else {
                Style::default().fg(Color::White)
            };

            // Pad to fill the row so the highlight spans the full width.
            let padded = format!("{:<width$}", label, width = inner.width as usize);
            buf.set_string(inner.x, inner.y + row_idx as u16, &padded, style);
        }
    }
}
