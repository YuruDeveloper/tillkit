#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Device,
    Pairs,
    Kernel,
    Write,
    Gsi,
    Info,
}

impl Screen {
    pub const ALL: [Self; 6] = [
        Self::Device,
        Self::Pairs,
        Self::Kernel,
        Self::Write,
        Self::Gsi,
        Self::Info,
    ];

    pub fn caption(self) -> &'static str {
        match self {
            Self::Device => "01 DEVICE",
            Self::Pairs => "02 PAIRS",
            Self::Kernel => "03 KERNEL",
            Self::Write => "04 WRITE",
            Self::Gsi => "05 GSI",
            Self::Info => "06 INFO",
        }
    }

    pub fn has_selection_line(self) -> bool {
        matches!(self, Self::Pairs | Self::Kernel | Self::Write)
    }
}
