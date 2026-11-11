use crate::error::{Front2Error, Result};
use crate::gpt::{GptCopyKind, SECTOR_BYTES};
use std::path::{Component, Path};

pub const PLAN_FILE: &str = "install-plan.json";
pub const PLAN_SCHEMA: u32 = 1;
pub const IMAGE_PARTITIONS: [&str; 8] = [
    "boot_b",
    "dtbo_b",
    "init_boot_b",
    "recovery_b",
    "vendor_boot_b",
    "vbmeta_system_b",
    "vbmeta_b",
    "super",
];
pub const METADATA_PARTITION: &str = "metadata";
pub const MISC_PARTITION: &str = "misc";
pub const USERDATA_PARTITION: &str = "userdata";
pub const MISC_PLAN_FILE: &str = "misc-first-sector.bin";
pub const MISC_STEP_SECTORS: u64 = 1;
pub const GPT_LUN_COUNT: u8 = 6;
pub const ACTIVATION_LUNS: [u8; 2] = [0, 4];
pub const GPT_STEP_ORDER: [(GptCopyKind, u8); 4] = [
    (GptCopyKind::Backup, 0),
    (GptCopyKind::Backup, 4),
    (GptCopyKind::Main, 0),
    (GptCopyKind::Main, 4),
];
pub const ORIGINAL_BOOT_LUN: u8 = 1;
pub const NEW_BOOT_LUN: u8 = 2;
pub const BOOT_WLUN: u8 = 176;
pub const BOOT_LUN_STEP_NAME: &str = "set_boot_lun";
pub const CHUNK_SECTORS: u64 = 1024;

pub fn gpt_step_name(kind: GptCopyKind, lun: u8) -> String {
    format!("gpt_{}{lun}", kind.label())
}

pub fn gpt_plan_file(kind: GptCopyKind, lun: u8) -> String {
    format!("gpt_{}{lun}-slot-b.bin", kind.label())
}

pub fn write_step_names() -> Vec<String> {
    let mut names: Vec<String> = IMAGE_PARTITIONS
        .iter()
        .map(|name| name.to_string())
        .collect();
    names.push(METADATA_PARTITION.to_string());
    names.extend(
        GPT_STEP_ORDER
            .iter()
            .map(|(kind, lun)| gpt_step_name(*kind, *lun)),
    );
    names.push(MISC_PARTITION.to_string());
    names
}

pub fn sectors_to_bytes(sectors: u64) -> Option<u64> {
    sectors.checked_mul(SECTOR_BYTES as u64)
}

pub fn require_relative_file(file: &str) -> Result<()> {
    let plain = !file.is_empty()
        && Path::new(file)
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !plain {
        return Err(Front2Error::Plan(format!(
            "file name {file:?} is not a plain relative path"
        )));
    }
    Ok(())
}

pub fn require_plain_file_name(file: &str) -> Result<()> {
    require_relative_file(file)?;
    if Path::new(file).components().count() != 1 {
        return Err(Front2Error::Plan(format!(
            "file name {file:?} names a file outside the plan directory"
        )));
    }
    Ok(())
}
