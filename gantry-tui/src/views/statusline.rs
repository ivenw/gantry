use crate::effects::throbber::{Throbber, ThrobberStyle};
use crate::model::InputMode;
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
}

pub struct StatuslineView {
    mode: InputMode,
    is_streaming: bool,
}

impl StatuslineView {
    pub fn new(mode: InputMode, is_streaming: bool) -> Self {
        Self { mode, is_streaming }
    }
}

impl StatefulWidget for StatuslineView {
    type State = StatuslineViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let text = if self.is_streaming {
            format!("[{}] EVALUATING", state.throbber.current())
        } else {
            match self.mode {
                InputMode::Normal => "NORMAL".to_string(),
                InputMode::Insert => "INSERT".to_string(),
            }
        };
        Paragraph::new(text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
