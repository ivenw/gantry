use crate::effects::throbber::{Throbber, ThrobberStyle};
use crate::model::InputMode;
use gantry_core::ContextWindow;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::{Color, Style},
    widgets::{Block, Padding, Paragraph, StatefulWidget},
};

pub struct StatuslineViewState {
    throbber: Throbber,
}

impl Default for StatuslineViewState {
    fn default() -> Self {
        Self {
            throbber: Throbber::new(ThrobberStyle::Ascii),
        }
    }
}

impl StatuslineViewState {
    pub fn tick(&mut self) {
        self.throbber.next();
    }

    /// Returns the current spinner frame character.
    pub fn spinner(&self) -> char {
        self.throbber.current()
    }
}

pub struct StatuslineView {
    mode: InputMode,
    is_streaming: bool,
    context_window: Option<ContextWindow>,
}

impl StatuslineView {
    pub fn new(mode: InputMode, is_streaming: bool, context_window: Option<ContextWindow>) -> Self {
        Self {
            mode,
            is_streaming,
            context_window,
        }
    }
}

impl StatefulWidget for StatuslineView {
    type State = StatuslineViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let mode_text = if self.is_streaming {
            format!("[{}] EVALUATING", state.throbber.current())
        } else {
            match self.mode {
                InputMode::Normal => "NORMAL".to_string(),
                InputMode::Insert => "INSERT".to_string(),
            }
        };

        let text = match self.context_window {
            Some(cw) => format!("{mode_text}  {}/{} ctx", cw.total_tokens, cw.max_tokens),
            None => mode_text,
        };

        Paragraph::new(text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
