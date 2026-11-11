use crate::gsi_message::GsiMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GsiAction {
    PickBackup,
    PickImage,
    Build,
    PlanInstall,
}

impl GsiAction {
    pub const ALL: [Self; 4] = [
        Self::PickBackup,
        Self::PickImage,
        Self::Build,
        Self::PlanInstall,
    ];

    pub fn caption(self) -> &'static str {
        match self {
            Self::PickBackup => "PICK BACKUP FOLDER",
            Self::PickImage => "PICK GSI IMAGE",
            Self::Build => "BUILD CANDIDATE",
            Self::PlanInstall => "PLAN INSTALL",
        }
    }

    pub fn message(self) -> GsiMessage {
        match self {
            Self::PickBackup => GsiMessage::PickBackup,
            Self::PickImage => GsiMessage::PickImage,
            Self::Build => GsiMessage::Build,
            Self::PlanInstall => GsiMessage::PlanInstall,
        }
    }
}
