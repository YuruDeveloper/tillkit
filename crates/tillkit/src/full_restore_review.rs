use front2::full_backup::RestoreCheck;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullRestoreReview {
    pub backup: PathBuf,
    pub with_user_data: bool,
    pub boot_lun: u8,
    pub boot_lun_selected: bool,
    pub areas: usize,
    pub different: Vec<String>,
    pub different_bytes: u64,
    pub first_boot_images: Vec<String>,
    pub same: bool,
}

impl FullRestoreReview {
    pub fn of(backup: &Path, with_user_data: bool, boot_lun: u8, check: &RestoreCheck) -> Self {
        Self {
            backup: backup.to_path_buf(),
            with_user_data,
            boot_lun,
            boot_lun_selected: check.boot_lun_is_selected(),
            areas: check.areas().len(),
            different: check
                .different()
                .iter()
                .map(|area| area.partition.name.clone())
                .collect(),
            different_bytes: check.different_bytes(),
            first_boot_images: check
                .first_boot_images()
                .into_iter()
                .map(str::to_string)
                .collect(),
            same: check.is_same(),
        }
    }

    pub fn can_be_written(&self) -> bool {
        !self.same && self.first_boot_images.is_empty()
    }
}
