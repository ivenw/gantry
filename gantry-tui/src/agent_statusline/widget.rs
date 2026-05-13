use std::time::Duration;

use crate::model::StreamState;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::{Color, Style},
    widgets::{Paragraph, StatefulWidget},
};

#[derive(Default)]
pub struct AgentStatuslineWidgetState;

pub struct AgentStatuslineWidget<'a> {
    stream: &'a StreamState,
    status_message: Option<&'a str>,
    spinner: char,
}

impl<'a> AgentStatuslineWidget<'a> {
    /// Creates a new agent statusline widget from the current stream state.
    pub fn new(stream: &'a StreamState, status_message: Option<&'a str>, spinner: char) -> Self {
        Self {
            stream,
            status_message,
            spinner,
        }
    }

    /// Returns the height this widget requires: 1 if there is content to display, 0 otherwise.
    pub fn height(&self) -> u16 {
        let has_stream = !matches!(self.stream, StreamState::Idle);
        if has_stream || self.status_message.is_some() {
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

impl StatefulWidget for AgentStatuslineWidget<'_> {
    type State = AgentStatuslineWidgetState;

    fn render(self, area: Rect, buf: &mut Buffer, _state: &mut Self::State) {
        let (text, color) = match self.stream {
            StreamState::Active { started_at } => (
                format!(
                    "{} EVALUATING ({})",
                    self.spinner,
                    format_duration(started_at.elapsed())
                ),
                Color::Gray,
            ),
            StreamState::Interrupted { .. } => ("INTERRUPTED".to_string(), Color::LightRed),
            StreamState::Done { duration } => (
                format!("* DONE ({})", format_duration(*duration)),
                Color::Gray,
            ),
            StreamState::Idle => {
                if let Some(msg) = self.status_message {
                    (msg.to_string(), Color::Gray)
                } else {
                    return;
                }
            }
        };

        Paragraph::new(text)
            .style(Style::default().fg(color))
            .render(area, buf);
    }
}
