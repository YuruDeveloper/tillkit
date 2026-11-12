use crate::device_profile::DeviceProfile;
use crate::error::{Front2Error, Result};
use crate::gpt::{GptCopyKind, PartitionMap, SECTOR_BYTES, SlotActivation, misc_wipe_sector};
use crate::gsi::backup_manifest::BackupManifest;
use crate::gsi::backup_partition::BackupPartition;
use crate::gsi::candidate_image::CandidateImage;
use crate::gsi::gsi_candidate::{GsiCandidate, SUPER_PARTITION};
use crate::install::layout::{ACTIVATION_LUNS, GPT_LUN_COUNT};
use crate::install::{GptBackup, InstallImage, InstallInputs};
use std::fs;
use std::io::Read;
use std::path::Path;

const METADATA_PARTITION: &str = "metadata";
const MISC_PARTITION: &str = "misc";
const USERDATA_PARTITION: &str = "userdata";

pub fn install_inputs(
    candidate: &GsiCandidate,
    candidate_directory: &Path,
    backup_directory: &Path,
    profile: &DeviceProfile,
) -> Result<InstallInputs> {
    profile.require_gsi()?;
    install_inputs_from_images(
        &candidate.images(),
        &candidate.original_super.sha256,
        candidate_directory,
        backup_directory,
    )
}

pub fn require_slot_a_active(backup_directory: &Path) -> Result<()> {
    let manifest = BackupManifest::load(&std::path::absolute(backup_directory)?)?;
    for lun in ACTIVATION_LUNS {
        let (_, main) = gpt_backup(&manifest, lun, GptCopyKind::Main)?;
        let (_, backup) = gpt_backup(&manifest, lun, GptCopyKind::Backup)?;
        SlotActivation::prepare(lun, &main, &backup)?;
    }
    Ok(())
}

pub fn install_inputs_from_images(
    images: &[CandidateImage],
    original_super_sha256: &str,
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
    let activation = |lun: u8| {
        let position = usize::from(lun);
        SlotActivation::prepare(lun, &main_copies[position], &backup_copies[position])
    };
    let [first_lun, second_lun] = ACTIVATION_LUNS;
    let misc = manifest.partition(MISC_PARTITION)?;
    Ok(InstallInputs {
        candidate_directory: std::path::absolute(candidate_directory)?,
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
        activation_lun0: activation(first_lun)?,
        activation_lun4: activation(second_lun)?,
        backup_directory,
    })
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
