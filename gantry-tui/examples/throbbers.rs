use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use gantry_tui::effects::throbber::{AsciiThrobberStyle, Throbber, Utf8ThrobberStyle};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    text::Line,
    widgets::Paragraph,
};

struct Row {
    label: &'static str,
    throbber: Throbber,
}

impl Row {
    fn new(label: &'static str, throbber: impl Into<Throbber>) -> Self {
        Self {
            label,
            throbber: throbber.into(),
        }
    }
}

fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut rows = vec![
        Row::new("Ascii / Propeller", AsciiThrobberStyle::Propeller),
        Row::new("Ascii / CirclePulse", AsciiThrobberStyle::CirclePulse),
        Row::new("Ascii / Qpbd", AsciiThrobberStyle::Qpbd),
        Row::new("Utf8 / LinesPulse", Utf8ThrobberStyle::LinesPulse),
        Row::new("Utf8 / BrailleCircling", Utf8ThrobberStyle::BrailleCircling),
        Row::new("Utf8 / BarPulse", Utf8ThrobberStyle::BarPulse),
        Row::new("Utf8 / BarSweep", Utf8ThrobberStyle::BarSweep),
        Row::new("Utf8 / ArcSpin", Utf8ThrobberStyle::ArcSpin),
    ];

    loop {
        terminal.draw(|frame| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    rows.iter()
                        .flat_map(|_| [Constraint::Length(1), Constraint::Length(1)])
                        .collect::<Vec<_>>(),
                )
                .split(frame.area());

            for (row, area) in rows.iter().zip(areas.iter().step_by(2)) {
                let line = Line::from(format!("{}  {}", row.throbber.current(), row.label));
                frame.render_widget(Paragraph::new(line), *area);
            }
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        let now = Instant::now();
        for row in &mut rows {
            row.throbber.tick(now);
        }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
