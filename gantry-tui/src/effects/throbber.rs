use std::time::{Duration, Instant};

pub enum AsciiThrobberStyle {
    Propeller,
    CirclePulse,
    Qpbd,
}

impl AsciiThrobberStyle {
    /// Returns the animation frames for this style.
    fn frames(&self) -> &'static [char] {
        match self {
            Self::Propeller => &['|', '/', '-', '\\'],
            Self::CirclePulse => &['.', 'o', 'O', 'o'],
            Self::Qpbd => &['q', 'p', 'b', 'd'],
        }
    }

    /// Returns the default frame duration for this style.
    fn frame_duration(&self) -> Duration {
        match self {
            Self::Propeller => Duration::from_millis(150),
            Self::CirclePulse => Duration::from_millis(120),
            Self::Qpbd => Duration::from_millis(120),
        }
    }
}

pub enum Utf8ThrobberStyle {
    LinesPulse,
    BrailleCircling,
    BarPulse,
    BarSweep,
    ArcSpin,
}

impl Utf8ThrobberStyle {
    /// Returns the animation frames for this style.
    fn frames(&self) -> &'static [char] {
        match self {
            Self::LinesPulse => &['-', '=', '≡', '='],
            Self::BrailleCircling => &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
            Self::BarPulse => &[
                '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', '▇', '▆', '▅', '▄', '▃',
            ],
            Self::BarSweep => &[
                '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█', '▉', '▊', '▋', '▌', '▍', '▎',
            ],
            Self::ArcSpin => &['◜', '◠', '◝', '◞', '◡', '◟'],
        }
    }

    /// Returns the default frame duration for this style.
    fn frame_duration(&self) -> Duration {
        match self {
            Self::LinesPulse => Duration::from_millis(120),
            Self::BrailleCircling => Duration::from_millis(80),
            Self::BarPulse => Duration::from_millis(80),
            Self::BarSweep => Duration::from_millis(80),
            Self::ArcSpin => Duration::from_millis(120),
        }
    }
}

enum ThrobberStyle {
    Ascii(AsciiThrobberStyle),
    Utf8(Utf8ThrobberStyle),
}

impl ThrobberStyle {
    /// Returns the animation frames for this style.
    fn frames(&self) -> &'static [char] {
        match self {
            Self::Ascii(s) => s.frames(),
            Self::Utf8(s) => s.frames(),
        }
    }

    /// Returns the default frame duration for this style.
    fn frame_duration(&self) -> Duration {
        match self {
            Self::Ascii(s) => s.frame_duration(),
            Self::Utf8(s) => s.frame_duration(),
        }
    }
}

pub struct Throbber {
    frames: &'static [char],
    frame_duration: Duration,
    idx: usize,
    last_advanced: Instant,
}

impl From<AsciiThrobberStyle> for Throbber {
    fn from(style: AsciiThrobberStyle) -> Self {
        ThrobberStyle::Ascii(style).into()
    }
}

impl From<Utf8ThrobberStyle> for Throbber {
    fn from(style: Utf8ThrobberStyle) -> Self {
        ThrobberStyle::Utf8(style).into()
    }
}

impl From<ThrobberStyle> for Throbber {
    fn from(style: ThrobberStyle) -> Self {
        Self {
            frames: style.frames(),
            frame_duration: style.frame_duration(),
            idx: 0,
            last_advanced: Instant::now(),
        }
    }
}

impl Throbber {
    /// Advances to the next frame if the frame duration has elapsed since the last advance.
    pub fn tick(&mut self, now: Instant) {
        if now.duration_since(self.last_advanced) >= self.frame_duration {
            self.idx = (self.idx + 1) % self.frames.len();
            self.last_advanced = now;
        }
    }

    /// Returns the current frame character.
    pub fn current(&self) -> char {
        self.frames[self.idx]
    }
}
