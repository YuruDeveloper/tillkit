#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Device,
    Backup,
    Write,
    Root,
    Advanced,
    Info,
}

impl Screen {
    pub const ALL: [Self; 6] = [
        Self::Device,
        Self::Backup,
        Self::Write,
        Self::Root,
        Self::Advanced,
        Self::Info,
    ];

    pub fn caption(self) -> &'static str {
        match self {
            Self::Device => "01 DEVICE",
            Self::Backup => "02 BACKUP",
            Self::Write => "03 WRITE",
            Self::Root => "04 ROOT",
            Self::Advanced => "05 ADVANCED",
            Self::Info => "06 INFO",
        }
    }

    pub fn has_selection_line(self) -> bool {
        matches!(self, Self::Write | Self::Root)
    }
}
