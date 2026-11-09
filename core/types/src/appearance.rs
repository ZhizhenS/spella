//! Candidate appearance shared by settings and the native presenter.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CandidateTheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateAppearance {
    pub theme: CandidateTheme,
    pub font_size: u32,
    pub width: u32,
    pub compact: bool,
    pub show_meanings: bool,
    pub show_part_of_speech: bool,
    pub show_phonetics: bool,
    pub show_key_hints: bool,
}

impl Default for CandidateAppearance {
    fn default() -> Self {
        Self {
            theme: CandidateTheme::System,
            font_size: 17,
            width: 420,
            compact: false,
            show_meanings: true,
            show_part_of_speech: true,
            show_phonetics: false,
            show_key_hints: true,
        }
    }
}

impl CandidateAppearance {
    pub fn normalized(mut self) -> Self {
        self.font_size = self.font_size.clamp(14, 22);
        self.width = self.width.clamp(320, 460);
        self
    }
}
