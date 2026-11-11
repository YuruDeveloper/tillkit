use crate::gsi_review::GsiReview;
use crate::install_review::InstallReview;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum GsiMessage {
    PickBackup,
    BackupPicked(Option<PathBuf>),
    PickImage,
    ImagePicked(Option<PathBuf>),
    Build,
    Built(Result<Box<GsiReview>, String>),
    PlanInstall,
    InstallPlanned(Result<Box<InstallReview>, String>),
    Close,
}
