use crate::model::Mode;
use crate::theme;
use gantry_core::ContextWindow;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
};

const SEPARATOR: &str = "    ";

pub struct AppStatuslineWidget {
    mode: Mode,
    context_window: Option<ContextWindow>,
}

impl AppStatuslineWidget {
    /// Creates a new app statusline widget.
    pub fn new(mode: Mode, context_window: Option<ContextWindow>) -> Self {
        Self {
            mode,
            context_window,
        }
    }
}

impl Widget for AppStatuslineWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode = {
            let text = match self.mode {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
            };

            Some(Span::styled(
                format!("[{}]", text),
                Style::default().fg(theme::mode_color(self.mode)),
            ))
        };

        let context = self.context_window.map(|cw| {
            Span::styled(
                format!("{}/{} ctx", cw.total_tokens, cw.max_tokens),
                Style::default().fg(Color::Gray),
            )
        });

        let segments: Vec<Span> = [mode, context].into_iter().flatten().collect();
        let spans: Vec<Span> = segments
            .into_iter()
            .enumerate()
            .flat_map(|(i, span)| {
                if i == 0 {
                    vec![span]
                } else {
                    vec![Span::raw(SEPARATOR), span]
                }
            })
            .collect();

        Paragraph::new(Line::from(spans))
            .block(Block::new().padding(Padding::horizontal(0)))
            .render(area, buf);
    }
}
