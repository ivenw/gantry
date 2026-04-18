use crate::effects::throbber::{Throbber, ThrobberStyle};
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
    is_streaming: bool,
}

impl StatuslineView {
    pub fn new(is_streaming: bool) -> Self {
        Self { is_streaming }
    }
}

impl StatefulWidget for StatuslineView {
    type State = StatuslineViewState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let text = if self.is_streaming {
            format!("[{}] EVALUATING", state.throbber.current())
        } else {
            String::new()
        };
        Paragraph::new(text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::new().padding(Padding::horizontal(2)))
            .render(area, buf);
    }
}
