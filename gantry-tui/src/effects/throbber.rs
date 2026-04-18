const FRAMES_ASCII: &[char] = &['|', '/', '-', '\\'];
const FRAMES_BRAILLE_CIRCLING: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub enum ThrobberStyle {
    Ascii,
    BrailleCircling,
}

pub struct Throbber {
    frames: &'static [char],
    idx: usize,
}

impl Throbber {
    pub fn new(style: ThrobberStyle) -> Self {
        let frames = match style {
            ThrobberStyle::Ascii => FRAMES_ASCII,
            ThrobberStyle::BrailleCircling => FRAMES_BRAILLE_CIRCLING,
        };
        Self { frames, idx: 0 }
    }

    pub fn next(&mut self) {
        self.idx = (self.idx + 1) % self.frames.len();
    }

    pub fn current(&self) -> char {
        self.frames[self.idx]
    }
}
