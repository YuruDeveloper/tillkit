#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Device,
    Pairs,
    Write,
    Scope,
}

impl Screen {
    pub const ALL: [Self; 4] = [Self::Device, Self::Pairs, Self::Write, Self::Scope];

    pub fn caption(self) -> &'static str {
        match self {
            Self::Device => "01 DEVICE",
            Self::Pairs => "02 PAIRS",
            Self::Write => "03 WRITE",
            Self::Scope => "04 SCOPE",
        }
    }
}
