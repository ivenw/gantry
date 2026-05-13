use std::time::{Duration, Instant};

use crate::effects::throbber::{Throbber, ThrobberStyle};
use crate::model::InputMode;
use crate::theme;
use gantry_core::ContextWindow;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::{Color, Style},
    widgets::{Block, Padding, Paragraph, StatefulWidget},
};

pub struct AgentStatuslineState {
    throbber: Throbber,
}

impl Default for AgentStatuslineState {
    fn default() -> Self {
        Self {
            throbber: Throbber::new(ThrobberStyle::Ascii),
        }
    }
}

impl AgentStatuslineState {
    /// Advances the spinner to the next frame.
    pub fn tick(&mut self) {
        self.throbber.next();
    }

    /// Returns the current spinner frame character.
    pub fn spinner(&self) -> char {
        self.throbber.current()
    }
}

pub struct AgentStatusline<'a> {
    is_streaming: bool,
    is_interrupted: bool,
    stream_started_at: Option<Instant>,
    stream_duration: Option<Duration>,
    status_message: Option<&'a str>,
}

impl<'a> AgentStatusline<'a> {
    /// Creates a new agent statusline view.
    pub fn new(
        is_streaming: bool,
        is_interrupted: bool,
        stream_started_at: Option<Instant>,
        stream_duration: Option<Duration>,
        status_message: Option<&'a str>,
    ) -> Self {
        Self {
            is_streaming,
            is_interrupted,
            stream_started_at,
            stream_duration,
            status_message,
        }
    }

    /// Returns the height this widget requires: 1 if there is content to display, 0 otherwise.
    pub fn height(&self) -> u16 {
        if self.is_streaming
            || self.is_interrupted
            || self.stream_duration.is_some()
            || self.status_message.is_some()
        {
            1
        } else {
            0
        }
    }
}

/// Formats a duration as `Xm Ys` or `Xs`.
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

impl StatefulWidget for AgentStatusline<'_> {
    type State = AgentStatuslineState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let (text, color) = if self.is_streaming {
            let elapsed = self
                .stream_started_at
                .map(|t| format_duration(t.elapsed()))
                .unwrap_or_default();
            (
                format!("{} EVALUATING ({})", state.throbber.current(), elapsed),
                Color::Gray,
            )
        } else if self.is_interrupted {
            ("INTERRUPTED".to_string(), Color::LightRed)
        } else if let Some(d) = self.stream_duration {
            (format!("* DONE ({})", format_duration(d)), Color::Gray)
        } else if let Some(msg) = self.status_message {
            (msg.to_string(), Color::Gray)
        } else {
            return;
        };

        Paragraph::new(text)
            .style(Style::default().fg(color))
            .render(area, buf);
    }
}

pub struct AppStatusline {
    mode: InputMode,
    context_window: Option<ContextWindow>,
}

impl AppStatusline {
    /// Creates a new app statusline view.
    pub fn new(mode: InputMode, context_window: Option<ContextWindow>) -> Self {
        Self {
            mode,
            context_window,
        }
    }
}

impl Widget for AppStatusline {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_text = match self.mode {
            InputMode::Normal => "NORMAL",
            InputMode::Insert => "INSERT",
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
