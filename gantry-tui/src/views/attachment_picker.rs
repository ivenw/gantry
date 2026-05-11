use std::path::Path;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::model::{AttachmentPicker, AttachmentPickerKind};

use super::table::{ColumnSpec, TableView, highlighted_line};

const STYLE_TEXT: Style = Style::new().fg(Color::Gray);
const STYLE_MATCH: Style = Style::new().fg(Color::LightYellow);
const STYLE_SELECTED: Style = Style::new().fg(Color::LightYellow).bold();
const STYLE_DESC: Style = Style::new().fg(Color::DarkGray);

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

        if count == 0 {
            buf.set_string(
                area.x,
                area.y,
                "No results",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        let rows: Vec<Vec<Line>> = match &self.state.kind {
            AttachmentPickerKind::Path(results) => results
                .iter()
                .skip(start)
                .take(max_visible)
                .enumerate()
                .map(|(row_idx, r)| {
                    let abs_idx = start + row_idx;
                    let label = r
                        .path
                        .strip_prefix(self.project_root)
                        .unwrap_or(&r.path)
                        .display()
                        .to_string();
                    let name_line = if abs_idx == selected {
                        Line::from(Span::styled(label, STYLE_SELECTED))
                    } else {
                        highlighted_line(&label, &r.indices, STYLE_TEXT, STYLE_MATCH)
                    };
                    vec![name_line]
                })
                .collect(),
            AttachmentPickerKind::Skill(results) => results
                .iter()
                .skip(start)
                .take(max_visible)
                .enumerate()
                .map(|(row_idx, r)| {
                    let abs_idx = start + row_idx;
                    let name = &r.skill.metadata.name;
                    let desc = &r.skill.metadata.description;
                    let name_line = if abs_idx == selected {
                        Line::from(Span::styled(name.clone(), STYLE_SELECTED))
                    } else {
                        highlighted_line(name, &r.indices, STYLE_TEXT, STYLE_MATCH)
                    };
                    let desc_line = Line::from(Span::styled(desc.clone(), STYLE_DESC));
                    vec![name_line, desc_line]
                })
                .collect(),
        };

        let columns = match &self.state.kind {
            AttachmentPickerKind::Path(_) => vec![ColumnSpec::new(0, None)],
            AttachmentPickerKind::Skill(_) => {
                vec![ColumnSpec::new(12, None), ColumnSpec::new(0, None)]
            }
        };

        TableView::new(columns, rows).render(area, buf);
    }
}
