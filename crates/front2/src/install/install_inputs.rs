use crate::gpt::{PartitionMap, SlotActivation};
use crate::image_record::ImageRecord;
use crate::install::gpt_backup::GptBackup;
use crate::install::install_image::InstallImage;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct InstallInputs {
    pub candidate_directory: PathBuf,
    pub backup_directory: PathBuf,
    pub partition_map: PartitionMap,
    pub images: Vec<InstallImage>,
    pub metadata_original: ImageRecord,
    pub misc_original: ImageRecord,
    pub userdata_original: ImageRecord,
    pub misc_first_sector: Vec<u8>,
    pub gpt_backups: Vec<GptBackup>,
    pub activation_lun0: SlotActivation,
    pub activation_lun4: SlotActivation,
}
