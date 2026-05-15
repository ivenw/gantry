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
}

pub enum Utf8ThrobberStyle {
    BarPulse,
    BrailleCircling,
}

impl Utf8ThrobberStyle {
    /// Returns the animation frames for this style.
    fn frames(&self) -> &'static [char] {
        match self {
            Self::BarPulse => &['-', '=', '≡', '='],
            Self::BrailleCircling => &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
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
}

pub struct Throbber {
    frames: &'static [char],
    idx: usize,
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
            idx: 0,
        }
    }
}

impl Throbber {
    /// Advances to the next frame, wrapping around.
    pub fn next(&mut self) {
        self.idx = (self.idx + 1) % self.frames.len();
    }

    /// Returns the current frame character.
    pub fn current(&self) -> char {
        self.frames[self.idx]
    }
}
