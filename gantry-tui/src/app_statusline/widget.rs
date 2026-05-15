use crate::model::Mode;
use crate::theme;
use gantry_core::{ContextWindow, Usage};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::Widget,
    style::{Color, Style},
    text::{Line, Span},
};

const SEPARATOR: &str = "    ";

pub struct AppStatuslineWidget {
    mode: Mode,
    context_window: Option<ContextWindow>,
    total_consumption: Option<Usage>,
}

impl AppStatuslineWidget {
    /// Creates a new app statusline widget.
    pub fn new(
        mode: Mode,
        context_window: Option<ContextWindow>,
        total_consumption: Option<Usage>,
    ) -> Self {
        Self {
            mode,
            context_window,
            total_consumption,
        }
    }
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

impl Widget for AppStatuslineWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_segment = {
            let text = match self.mode {
                Mode::Normal => "NORMAL",
                Mode::Insert => "INSERT",
            };

            Some(Span::styled(
                format!("[{}]", text),
                Style::default().fg(theme::mode_color(self.mode)),
            ))
        };

        let context_segment = self.context_window.map(|cw| {
            Span::styled(
                format!("{}/{} ctx", cw.total_tokens, cw.max_tokens),
                Style::default().fg(Color::Gray),
            )
        });

        let consumption_segment = self.total_consumption.map(|u| {
            Span::styled(
                format!(
                    "I{} O{} R{} W{}",
                    fmt_tokens(u.input_tokens),
                    fmt_tokens(u.output_tokens),
                    fmt_tokens(u.cached_input_tokens),
                    fmt_tokens(u.cache_creation_input_tokens),
                ),
                Style::default().fg(Color::Gray),
            )
        });

        let segments: Vec<Span> = [mode_segment, context_segment, consumption_segment]
            .into_iter()
            .flatten()
            .collect();
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

        Line::from(spans).render(area, buf);
    }
}
