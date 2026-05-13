use crate::model::Mode;
use crate::theme;
use gantry_core::ContextWindow;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::Style,
    widgets::{Block, Padding, Paragraph},
};

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
        let mode_text = match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
        };

        let text = match self.context_window {
            Some(cw) => format!("{mode_text}  {}/{} ctx", cw.total_tokens, cw.max_tokens),
            None => mode_text.to_string(),
        };

        Paragraph::new(text)
            .style(Style::default().fg(theme::mode_color(self.mode)))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
