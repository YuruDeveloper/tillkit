#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Device,
    Backup,
    Write,
    Info,
}

impl Screen {
    pub const ALL: [Self; 4] = [Self::Device, Self::Backup, Self::Write, Self::Info];

    pub fn caption(self) -> &'static str {
        match self {
            Self::Device => "01 DEVICE",
            Self::Backup => "02 BACKUP",
            Self::Write => "03 WRITE",
            Self::Info => "04 INFO",
        }
    }

    pub fn has_selection_line(self) -> bool {
        self == Self::Write
    }
}
