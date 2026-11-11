#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Device,
    Backup,
    Pairs,
    Kernel,
    Write,
    Gsi,
    Info,
}

impl Screen {
    pub const ALL: [Self; 7] = [
        Self::Device,
        Self::Backup,
        Self::Pairs,
        Self::Kernel,
        Self::Write,
        Self::Gsi,
        Self::Info,
    ];

    pub fn caption(self) -> &'static str {
        match self {
            Self::Device => "01 DEVICE",
            Self::Backup => "02 BACKUP",
            Self::Pairs => "03 PAIRS",
            Self::Kernel => "04 KERNEL",
            Self::Write => "05 WRITE",
            Self::Gsi => "06 GSI",
            Self::Info => "07 INFO",
        }
    }

    pub fn has_selection_line(self) -> bool {
        matches!(self, Self::Pairs | Self::Kernel | Self::Write)
    }
}
