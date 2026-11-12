use crate::full_backup::restore_area::RestoreArea;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreCheck {
    pub areas: usize,
    pub different: Vec<RestoreArea>,
    pub boot_lun_selected: bool,
}

impl RestoreCheck {
    pub fn is_same(&self) -> bool {
        self.different.is_empty() && self.boot_lun_selected
    }

    pub fn first_boot_images(&self) -> Vec<&str> {
        self.different
            .iter()
            .filter(|area| area.is_first_boot_image())
            .map(|area| area.partition.name.as_str())
            .collect()
    }

    pub fn different_bytes(&self) -> u64 {
        self.different.iter().map(|area| area.partition.bytes).sum()
    }
}
