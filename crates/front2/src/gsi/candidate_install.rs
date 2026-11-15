use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::gpt::slot_activation::SLOT_A_SUFFIX;
use crate::gpt::{
    ACTIVE_FLAG, BootSlot, GptCopy, GptCopyKind, PartitionMap, SECTOR_BYTES, SlotActivation,
    misc_wipe_sector,
};
use crate::gsi::backup_manifest::BackupManifest;
use crate::gsi::backup_partition::BackupPartition;
use crate::gsi::candidate_image::CandidateImage;
use crate::gsi::gsi_candidate::{GsiCandidate, SUPER_PARTITION};
use crate::install::layout::{ACTIVATION_LUNS, GPT_LUN_COUNT, NEW_BOOT_LUN, ORIGINAL_BOOT_LUN};
use crate::install::{GptBackup, InstallImage, InstallInputs};
use std::fs;
use std::io::Read;
use std::path::Path;

const METADATA_PARTITION: &str = "metadata";
const MISC_PARTITION: &str = "misc";
const USERDATA_PARTITION: &str = "userdata";
const XBL_BOOT_TABLES: [(u8, &str); 2] = [(ORIGINAL_BOOT_LUN, "xbl_a"), (NEW_BOOT_LUN, "xbl_b")];

pub fn install_inputs(
    candidate: &GsiCandidate,
    candidate_directory: &Path,
    backup_directory: &Path,
    profile: &DeviceProfile,
) -> Result<InstallInputs> {
    profile.require_gsi()?;
    let inputs = install_inputs_from_images(
        &candidate.images(),
        &candidate.original_super.sha256,
        candidate.target_slot,
        candidate_directory,
        backup_directory,
    )?;
    require_matching_stock_slot(candidate.stock_slot, inputs.stock_slot)?;
    require_source_matches_boot(candidate.source_slot, inputs.stock_slot)?;
    Ok(inputs)
}

fn require_matching_stock_slot(candidate: BootSlot, backup: BootSlot) -> Result<()> {
    if candidate != backup {
        return Err(Front2Error::Plan(format!(
            "the candidate was made for stock slot {candidate:?}, the backup boots slot {backup:?}"
        )));
    }
    Ok(())
}

fn require_source_matches_boot(source: BootSlot, boot: BootSlot) -> Result<()> {
    if source != boot {
        return Err(Front2Error::Unsupported(format!(
            "the candidate reads the stock images from the source slot {source:?}, the device boots slot {boot:?}; the install supports a source slot that is the same as the boot slot until the kk gate is complete"
        )));
    }
    Ok(())
}

pub fn backup_boot_slot(backup_directory: &Path) -> Result<BootSlot> {
    let manifest = BackupManifest::load(&std::path::absolute(backup_directory)?)?;
    boot_slot(&manifest)
}

fn boot_slot(manifest: &BackupManifest) -> Result<BootSlot> {
    let [_, boot_lun] = ACTIVATION_LUNS;
    let (_, main) = gpt_backup(manifest, boot_lun, GptCopyKind::Main)?;
    BootSlot::read(boot_lun, &GptCopy::parse(GptCopyKind::Main, &main)?)
}

fn require_boot_slot_a(manifest: &BackupManifest) -> Result<()> {
    match boot_slot(manifest)? {
        BootSlot::A => Ok(()),
        BootSlot::B => Err(Front2Error::Unsupported(
            "the backup shows slot B as the boot slot, the installation changes the boot slot only from A to B".into(),
        )),
    }
}

pub fn require_slot_a_active(backup_directory: &Path) -> Result<()> {
    let manifest = BackupManifest::load(&std::path::absolute(backup_directory)?)?;
    require_boot_slot_a(&manifest)?;
    for lun in ACTIVATION_LUNS {
        let (_, main) = gpt_backup(&manifest, lun, GptCopyKind::Main)?;
        let (_, backup) = gpt_backup(&manifest, lun, GptCopyKind::Backup)?;
        SlotActivation::prepare(lun, BootSlot::B, &main, &backup)?;
    }
    Ok(())
}

pub fn install_inputs_from_images(
    images: &[CandidateImage],
    original_super_sha256: &str,
    target_slot: BootSlot,
    candidate_directory: &Path,
    backup_directory: &Path,
) -> Result<InstallInputs> {
    let backup_directory = std::path::absolute(backup_directory)?;
    let manifest = BackupManifest::load(&backup_directory)?;
    let recorded_super = manifest.partition(SUPER_PARTITION)?;
    if recorded_super.sha256 != original_super_sha256 {
        return Err(Front2Error::Plan(format!(
            "the candidate was made from a super image with SHA-256 {original_super_sha256}, the backup records {}",
            recorded_super.sha256
        )));
    }
    let stock_slot = boot_slot(&manifest)?;
    let mut gpt_backups = Vec::new();
    let mut main_copies = Vec::new();
    let mut backup_copies = Vec::new();
    for lun in 0..GPT_LUN_COUNT {
        let (main, main_bytes) = gpt_backup(&manifest, lun, GptCopyKind::Main)?;
        let (backup, backup_bytes) = gpt_backup(&manifest, lun, GptCopyKind::Backup)?;
        gpt_backups.extend([main, backup]);
        main_copies.push(main_bytes);
        backup_copies.push(backup_bytes);
    }
    let borrowed: Vec<(u8, &[u8])> = (0u8..)
        .zip(&main_copies)
        .map(|(lun, bytes)| (lun, bytes.as_slice()))
        .collect();
    let (activation_lun0, activation_lun4) =
        slot_activations(stock_slot, &main_copies, &backup_copies)?;
    let misc = manifest.partition(MISC_PARTITION)?;
    Ok(InstallInputs {
        candidate_directory: std::path::absolute(candidate_directory)?,
        stock_slot,
        target_slot,
        partition_map: PartitionMap::from_main_copies(&borrowed)?,
        images: images
            .iter()
            .map(|image| install_image(&manifest, image))
            .collect::<Result<Vec<InstallImage>>>()?,
        metadata_original: manifest.partition(METADATA_PARTITION)?.record(),
        misc_first_sector: misc_wipe_sector(&first_sector(&misc.path)?)?,
        misc_original: misc.record(),
        userdata_original: manifest.partition(USERDATA_PARTITION)?.record(),
        gpt_backups,
        activation_lun0,
        activation_lun4,
        backup_directory,
    })
}

fn slot_activations(
    stock_slot: BootSlot,
    main_copies: &[Vec<u8>],
    backup_copies: &[Vec<u8>],
) -> Result<(Option<SlotActivation>, Option<SlotActivation>)> {
    match stock_slot {
        BootSlot::A => {
            let [first_lun, second_lun] = ACTIVATION_LUNS;
            Ok((
                Some(prepare_activation(first_lun, main_copies, backup_copies)?),
                Some(prepare_activation(second_lun, main_copies, backup_copies)?),
            ))
        }
        BootSlot::B => {
            require_tidy_stock_b(main_copies, backup_copies)?;
            Ok((None, None))
        }
    }
}

fn prepare_activation(
    lun: u8,
    main_copies: &[Vec<u8>],
    backup_copies: &[Vec<u8>],
) -> Result<SlotActivation> {
    let position = usize::from(lun);
    SlotActivation::prepare(
        lun,
        BootSlot::B,
        &main_copies[position],
        &backup_copies[position],
    )
}

fn require_tidy_stock_b(main_copies: &[Vec<u8>], backup_copies: &[Vec<u8>]) -> Result<()> {
    for lun in ACTIVATION_LUNS {
        let position = usize::from(lun);
        let main = GptCopy::parse(GptCopyKind::Main, &main_copies[position])?;
        let backup = GptCopy::parse(GptCopyKind::Backup, &backup_copies[position])?;
        main.require_pair_with(&backup, &format!("LUN {lun}"))?;
        require_no_active_slot_a(lun, &main)?;
    }
    require_stock_b_boot_lun(main_copies)
}

fn require_no_active_slot_a(lun: u8, main: &GptCopy) -> Result<()> {
    for entry in main.entries() {
        if entry.name.ends_with(SLOT_A_SUFFIX) && entry.flags & ACTIVE_FLAG != 0 {
            return Err(Front2Error::Unsupported(format!(
                "LUN {lun}: {} has the active flag, a tidy stock slot B backup has no active slot A partition",
                entry.name
            )));
        }
    }
    Ok(())
}

fn require_stock_b_boot_lun(main_copies: &[Vec<u8>]) -> Result<()> {
    let boot_lun = xbl_boot_lun(main_copies)?;
    if boot_lun != NEW_BOOT_LUN {
        return Err(Front2Error::Unsupported(format!(
            "the backup shows boot LUN {boot_lun}, a stock slot B backup boots LUN {NEW_BOOT_LUN}"
        )));
    }
    Ok(())
}

fn xbl_boot_lun(main_copies: &[Vec<u8>]) -> Result<u8> {
    let mut active = Vec::new();
    for (lun, name) in XBL_BOOT_TABLES {
        let main = GptCopy::parse(GptCopyKind::Main, &main_copies[usize::from(lun)])?;
        if main
            .entry(name)
            .is_some_and(|entry| entry.flags & ACTIVE_FLAG != 0)
        {
            active.push(lun);
        }
    }
    match active.as_slice() {
        [lun] => Ok(*lun),
        _ => Err(Front2Error::Unsupported(format!(
            "the backup does not show one boot LUN: {} of xbl_a on LUN {ORIGINAL_BOOT_LUN} and xbl_b on LUN {NEW_BOOT_LUN} have the active flag",
            active.len()
        ))),
    }
}

fn gpt_backup(
    manifest: &BackupManifest,
    lun: u8,
    kind: GptCopyKind,
) -> Result<(GptBackup, Vec<u8>)> {
    let name = format!("gpt_{}{lun}", kind.label());
    let partition = manifest.verified_partition(&name)?;
    if partition.lun != lun {
        return Err(Front2Error::Image(format!(
            "the backup holds {name} under LUN {}",
            partition.lun
        )));
    }
    let bytes = fs::read(&partition.path)?;
    let backup = GptBackup {
        lun,
        copy: kind,
        record: partition.record(),
    };
    Ok((backup, bytes))
}

fn install_image(manifest: &BackupManifest, image: &CandidateImage) -> Result<InstallImage> {
    let original: BackupPartition = manifest.partition(&image.partition)?;
    Ok(InstallImage {
        partition: image.partition.clone(),
        candidate: image.image.clone(),
        original: original.record(),
    })
}

fn first_sector(path: &Path) -> Result<Vec<u8>> {
    let mut sector = vec![0u8; SECTOR_BYTES];
    fs::File::open(path)?.read_exact(&mut sector)?;
    Ok(sector)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_candidate_and_a_backup_of_the_same_stock_slot_agree() {
        require_matching_stock_slot(BootSlot::A, BootSlot::A).expect("slot A and slot A agree");
        require_matching_stock_slot(BootSlot::B, BootSlot::B).expect("slot B and slot B agree");
    }

    #[test]
    fn a_candidate_of_a_different_stock_slot_than_the_backup_is_rejected() {
        let error = require_matching_stock_slot(BootSlot::B, BootSlot::A)
            .expect_err("a slot B candidate on a slot A backup");
        assert!(matches!(error, Front2Error::Plan(_)), "{error}");
        assert!(error.to_string().contains("stock slot"), "{error}");
    }

    #[test]
    fn a_candidate_source_that_matches_the_boot_slot_is_accepted() {
        require_source_matches_boot(BootSlot::A, BootSlot::A)
            .expect("a slot A source on a slot A boot");
        require_source_matches_boot(BootSlot::B, BootSlot::B)
            .expect("a slot B source on a slot B boot");
    }

    #[test]
    fn a_candidate_source_that_is_not_the_boot_slot_is_rejected() {
        let error = require_source_matches_boot(BootSlot::A, BootSlot::B)
            .expect_err("a slot A source on a slot B boot");
        assert!(matches!(error, Front2Error::Unsupported(_)), "{error}");
        assert!(error.to_string().contains("source slot"), "{error}");
    }
}
