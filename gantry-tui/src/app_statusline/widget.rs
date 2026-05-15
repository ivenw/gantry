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
use std::path::{Path, PathBuf};

const SEPARATOR: &str = " | ";

pub struct AppStatuslineWidget {
    mode: Mode,
    context_window: Option<ContextWindow>,
    total_consumption: Option<Usage>,
    project_name: String,
    project_path: PathBuf,
    cwd: PathBuf,
}

impl AppStatuslineWidget {
    /// Creates a new app statusline widget.
    pub fn new(
        mode: Mode,
        context_window: Option<ContextWindow>,
        total_consumption: Option<Usage>,
        project_name: String,
        project_path: PathBuf,
        cwd: PathBuf,
    ) -> Self {
        Self {
            mode,
            context_window,
            total_consumption,
            project_name,
            project_path,
            cwd,
        }
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

        let cwd_segment = Some(Span::styled(
            fmt_cwd(&self.project_name, &self.project_path, &self.cwd),
            Style::default().fg(Color::Gray),
        ));

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

        let context_segment = self.context_window.map(|cw| {
            Span::styled(
                format!("{}/{} ctx", cw.total_tokens, cw.max_tokens),
                Style::default().fg(Color::Gray),
            )
        });

        let segments: Vec<Span> = [
            mode_segment,
            cwd_segment,
            consumption_segment,
            context_segment,
        ]
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
                    vec![
                        Span::styled(SEPARATOR, Style::default().fg(Color::DarkGray)),
                        span,
                    ]
                }
            })
            .collect();

        Line::from(spans).render(area, buf);
    }
}

/// Formats the working directory as `<project_name>[/relative]` where `relative` is `cwd`
/// stripped of the `project_path` prefix.
fn fmt_cwd(project_name: &str, project_path: &Path, cwd: &Path) -> String {
    let relative = cwd.strip_prefix(project_path).ok().and_then(|p| {
        if p.as_os_str().is_empty() {
            None
        } else {
            p.to_str()
        }
    });

    let project_name = format!("<{}>", project_name);

    match relative {
        Some(rel) => format!("{}/{}", project_name, rel),
        None => project_name,
    }
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}
