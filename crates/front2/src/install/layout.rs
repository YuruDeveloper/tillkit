use crate::error::{Front2Error, Result};
use crate::gpt::{BootSlot, GptCopyKind, SECTOR_BYTES};
use crate::profile::slot_label;
use std::path::{Component, Path};

pub const PLAN_FILE: &str = "install-plan.json";
pub const PLAN_SCHEMA: u32 = 1;
pub const IMAGE_PARTITION_BASES: [&str; 7] = [
    "boot",
    "dtbo",
    "init_boot",
    "recovery",
    "vendor_boot",
    "vbmeta_system",
    "vbmeta",
];
pub const SUPER_PARTITION: &str = "super";
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
pub const SET_BOOT_LUN_STEP_NAME: &str = "set_boot_lun";
pub const KEEP_BOOT_LUN_STEP_NAME: &str = "keep_boot_lun";
pub const CHUNK_SECTORS: u64 = 1024;

pub fn image_partitions(target_slot: BootSlot) -> Vec<String> {
    let mut names: Vec<String> = IMAGE_PARTITION_BASES
        .iter()
        .map(|base| slot_label(base, target_slot.suffix()))
        .collect();
    names.push(SUPER_PARTITION.to_string());
    names
}

pub fn boot_lun_pair(target_slot: BootSlot) -> (u8, u8) {
    match target_slot {
        BootSlot::A => (NEW_BOOT_LUN, ORIGINAL_BOOT_LUN),
        BootSlot::B => (ORIGINAL_BOOT_LUN, NEW_BOOT_LUN),
    }
}

fn slot_letter(slot: BootSlot) -> &'static str {
    &slot.suffix()[1..]
}

pub fn gpt_step_name(kind: GptCopyKind, lun: u8) -> String {
    format!("gpt_{}{lun}", kind.label())
}

pub fn gpt_plan_file(kind: GptCopyKind, lun: u8, target_slot: BootSlot) -> String {
    format!(
        "gpt_{}{lun}-slot-{}.bin",
        kind.label(),
        slot_letter(target_slot)
    )
}

pub fn write_step_names(boot_slot: BootSlot, target_slot: BootSlot) -> Vec<String> {
    let mut names = image_partitions(target_slot);
    names.push(METADATA_PARTITION.to_string());
    if boot_slot != target_slot {
        names.extend(
            GPT_STEP_ORDER
                .iter()
                .map(|(kind, lun)| gpt_step_name(*kind, *lun)),
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_partitions_take_the_suffix_of_the_target_slot() {
        assert_eq!(
            image_partitions(BootSlot::B),
            [
                "boot_b",
                "dtbo_b",
                "init_boot_b",
                "recovery_b",
                "vendor_boot_b",
                "vbmeta_system_b",
                "vbmeta_b",
                "super",
            ]
        );
        assert_eq!(
            image_partitions(BootSlot::A),
            [
                "boot_a",
                "dtbo_a",
                "init_boot_a",
                "recovery_a",
                "vendor_boot_a",
                "vbmeta_system_a",
                "vbmeta_a",
                "super",
            ]
        );
    }

    #[test]
    fn the_boot_lun_pair_gives_the_target_lun_as_new_and_the_other_lun_as_original() {
        assert_eq!(
            boot_lun_pair(BootSlot::B),
            (ORIGINAL_BOOT_LUN, NEW_BOOT_LUN)
        );
        assert_eq!(
            boot_lun_pair(BootSlot::A),
            (NEW_BOOT_LUN, ORIGINAL_BOOT_LUN)
        );
    }

    #[test]
    fn the_gpt_plan_file_takes_the_letter_of_the_target_slot() {
        assert_eq!(
            gpt_plan_file(GptCopyKind::Main, 0, BootSlot::B),
            "gpt_main0-slot-b.bin"
        );
        assert_eq!(
            gpt_plan_file(GptCopyKind::Main, 0, BootSlot::A),
            "gpt_main0-slot-a.bin"
        );
    }

    #[test]
    fn the_write_step_names_hold_the_gpt_steps_only_when_the_boot_slot_differs_from_the_target() {
        let same = write_step_names(BootSlot::B, BootSlot::B);
        assert!(!same.iter().any(|name| name.starts_with("gpt_")));

        let switch_to_a = write_step_names(BootSlot::B, BootSlot::A);
        assert!(switch_to_a.contains(&"boot_a".to_string()));
        assert!(switch_to_a.contains(&"gpt_main0".to_string()));
        assert!(switch_to_a.contains(&"gpt_backup4".to_string()));

        let switch_to_b = write_step_names(BootSlot::A, BootSlot::B);
        assert!(switch_to_b.contains(&"boot_b".to_string()));
        assert!(switch_to_b.contains(&"gpt_main0".to_string()));
    }
}
