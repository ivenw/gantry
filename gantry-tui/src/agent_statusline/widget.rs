use std::time::Duration;

use crate::model::StreamState;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, StatefulWidget},
};

const SEPARATOR: &str = "    ";

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
        let stream_span = match self.stream {
            StreamState::Active { started_at } => Some(Span::styled(
                format!(
                    "{} EVALUATING ({})",
                    self.spinner,
                    format_duration(started_at.elapsed())
                ),
                Style::default().fg(Color::Gray),
            )),
            StreamState::Interrupted { .. } => Some(Span::styled(
                "INTERRUPTED",
                Style::default().fg(Color::LightRed),
            )),
            StreamState::Done { duration } => Some(Span::styled(
                format!("* DONE ({})", format_duration(*duration)),
                Style::default().fg(Color::Gray),
            )),
            StreamState::Idle => None,
        };

        let status_span = self
            .status_message
            .map(|msg| Span::styled(msg, Style::default().fg(Color::Gray)));

        let segments: Vec<Span> = [stream_span, status_span].into_iter().flatten().collect();

        if segments.is_empty() {
            return;
        }

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

        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
