use crate::digest::{hex, is_sha256_hex, sha256_bytes};
use crate::error::{Front2Error, Result};
use crate::gpt::{
    GptCopy, GptCopyKind, GptOutput, PartitionLocation, PartitionMap, SECTOR_BYTES, SlotActivation,
    misc_wipe_sector, require_current_copy_matches,
};
use crate::image_record::ImageRecord;
use crate::install::gpt_copy_record::GptCopyRecord;
use crate::install::install_image::InstallImage;
use crate::install::install_inputs::InstallInputs;
use crate::install::layout::{
    ACTIVATION_LUNS, GPT_LUN_COUNT, GPT_STEP_ORDER, IMAGE_PARTITIONS, METADATA_PARTITION,
    MISC_PARTITION, MISC_PLAN_FILE, MISC_STEP_SECTORS, USERDATA_PARTITION, gpt_plan_file,
    gpt_step_name, require_relative_file, sectors_to_bytes,
};
use crate::install::plan_payload::PlanPayload;
use crate::install::planned_target::PlannedTarget;
use crate::install::restore_entry::RestoreEntry;
use crate::install::step_source::StepSource;
use crate::install::write_step::WriteStep;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

const ZERO_BLOCK_BYTES: u64 = 1024 * 1024;

pub fn gpt_copy_records(inputs: &InstallInputs) -> Result<Vec<GptCopyRecord>> {
    let expected = usize::from(GPT_LUN_COUNT) * 2;
    if inputs.gpt_backups.len() != expected {
        return Err(Front2Error::Plan(format!(
            "the inputs hold {} GPT backup copies, the main and backup copies of LUN 0 to {} are required",
            inputs.gpt_backups.len(),
            GPT_LUN_COUNT - 1
        )));
    }
    let mut records = Vec::new();
    let mut main_copies: Vec<(u8, Vec<u8>)> = Vec::new();
    for lun in 0..GPT_LUN_COUNT {
        let (main_record, main_bytes) = read_gpt_backup(inputs, lun, GptCopyKind::Main)?;
        let (backup_record, backup_bytes) = read_gpt_backup(inputs, lun, GptCopyKind::Backup)?;
        GptCopy::parse(GptCopyKind::Main, &main_bytes)?.require_pair_with(
            &GptCopy::parse(GptCopyKind::Backup, &backup_bytes)?,
            &format!("backup of LUN {lun}"),
        )?;
        records.push(main_record);
        records.push(backup_record);
        main_copies.push((lun, main_bytes));
    }
    require_map_from_backup(&inputs.partition_map, &main_copies)?;
    Ok(records)
}

pub fn planned_targets(inputs: &InstallInputs) -> Result<Vec<PlannedTarget>> {
    require_known_images(&inputs.images)?;
    let mut targets = Vec::new();
    for name in IMAGE_PARTITIONS {
        targets.push(image_target(inputs, name)?);
    }
    targets.push(metadata_target(inputs)?);
    for (kind, lun) in GPT_STEP_ORDER {
        targets.push(gpt_target(inputs, kind, lun)?);
    }
    targets.push(misc_target(inputs)?);
    Ok(targets)
}

pub fn userdata_restore(inputs: &InstallInputs) -> Result<RestoreEntry> {
    let name = USERDATA_PARTITION;
    let original = &inputs.userdata_original;
    let location = inputs.partition_map.locate(name)?;
    let bytes = partition_bytes(name, location)?;
    require_whole_partition(name, "original backup", original, bytes)?;
    require_file_bytes(&inputs.backup_directory, original)?;
    Ok(restore_entry(name, location, original))
}

fn read_gpt_backup(
    inputs: &InstallInputs,
    lun: u8,
    kind: GptCopyKind,
) -> Result<(GptCopyRecord, Vec<u8>)> {
    let mut matching = inputs
        .gpt_backups
        .iter()
        .filter(|backup| backup.lun == lun && backup.copy == kind);
    let (Some(backup), None) = (matching.next(), matching.next()) else {
        return Err(Front2Error::Plan(format!(
            "the inputs do not hold exactly one backup of the {} GPT copy of LUN {lun}",
            kind.label()
        )));
    };
    let record = &backup.record;
    require_relative_file(&record.file)?;
    let path = inputs.backup_directory.join(&record.file);
    let bytes = fs::read(&path)?;
    let sha256 = sha256_bytes(&bytes);
    if bytes.len() as u64 != record.bytes || sha256 != record.sha256 {
        return Err(Front2Error::Verification(format!(
            "{} is {} bytes with SHA-256 {sha256}, recorded {} bytes with SHA-256 {}",
            path.display(),
            bytes.len(),
            record.bytes,
            record.sha256
        )));
    }
    let copy = GptCopy::parse(kind, &bytes).map_err(|err| with_lun(lun, err))?;
    let plan_record = GptCopyRecord {
        lun,
        copy: kind,
        start_sector: copy.start_sector(),
        sectors: copy.sectors(),
        file: record.file.clone(),
        bytes: record.bytes,
        sha256,
    };
    Ok((plan_record, bytes))
}

fn with_lun(lun: u8, err: Front2Error) -> Front2Error {
    match err {
        Front2Error::Image(text) => Front2Error::Image(format!("backup of LUN {lun}: {text}")),
        other => other,
    }
}

fn require_map_from_backup(
    partition_map: &PartitionMap,
    main_copies: &[(u8, Vec<u8>)],
) -> Result<()> {
    let borrowed: Vec<(u8, &[u8])> = main_copies
        .iter()
        .map(|(lun, bytes)| (*lun, bytes.as_slice()))
        .collect();
    if PartitionMap::from_main_copies(&borrowed)? != *partition_map {
        return Err(Front2Error::Plan(
            "the partition map differs from the map of the main GPT copies in the backup".into(),
        ));
    }
    Ok(())
}

fn require_known_images(images: &[InstallImage]) -> Result<()> {
    let unknown = images
        .iter()
        .find(|image| !IMAGE_PARTITIONS.contains(&image.partition.as_str()));
    match unknown {
        Some(image) => Err(Front2Error::Plan(format!(
            "the install plan has no step for an image of {:?}",
            image.partition
        ))),
        None => Ok(()),
    }
}

fn single_image<'inputs>(
    inputs: &'inputs InstallInputs,
    name: &str,
) -> Result<&'inputs InstallImage> {
    let mut matching = inputs.images.iter().filter(|image| image.partition == name);
    match (matching.next(), matching.next()) {
        (Some(image), None) => Ok(image),
        _ => Err(Front2Error::Plan(format!(
            "the inputs do not hold exactly one image for {name}"
        ))),
    }
}

fn partition_bytes(name: &str, location: PartitionLocation) -> Result<u64> {
    sectors_to_bytes(location.sectors)
        .filter(|bytes| *bytes > 0)
        .ok_or_else(|| {
            Front2Error::Plan(format!(
                "the partition {name} has {} sectors",
                location.sectors
            ))
        })
}

fn require_whole_partition(name: &str, role: &str, record: &ImageRecord, bytes: u64) -> Result<()> {
    require_relative_file(&record.file)?;
    if !is_sha256_hex(&record.sha256) {
        return Err(Front2Error::Plan(format!(
            "the {role} of {name} records a SHA-256 that is not 64 hexadecimal digits"
        )));
    }
    let relation = match record.bytes.cmp(&bytes) {
        std::cmp::Ordering::Greater => "larger",
        std::cmp::Ordering::Less => "smaller",
        std::cmp::Ordering::Equal => return Ok(()),
    };
    Err(Front2Error::Plan(format!(
        "the {role} of {name} is {} bytes, {relation} than the partition of {bytes} bytes; an image of the whole partition is required",
        record.bytes
    )))
}

fn require_file_bytes(directory: &Path, record: &ImageRecord) -> Result<()> {
    let path = directory.join(&record.file);
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() || metadata.len() != record.bytes {
        return Err(Front2Error::Verification(format!(
            "{} is not a file of {} bytes",
            path.display(),
            record.bytes
        )));
    }
    Ok(())
}

fn restore_entry(name: &str, location: PartitionLocation, original: &ImageRecord) -> RestoreEntry {
    RestoreEntry {
        name: name.to_string(),
        lun: location.lun,
        start_sector: location.start_sector,
        sectors: location.sectors,
        file: original.file.clone(),
        bytes: original.bytes,
        sha256: original.sha256.clone(),
    }
}

fn image_target(inputs: &InstallInputs, name: &str) -> Result<PlannedTarget> {
    let image = single_image(inputs, name)?;
    let location = inputs.partition_map.locate(name)?;
    let bytes = partition_bytes(name, location)?;
    require_whole_partition(name, "candidate image", &image.candidate, bytes)?;
    require_whole_partition(name, "original backup", &image.original, bytes)?;
    require_file_bytes(&inputs.candidate_directory, &image.candidate)?;
    require_file_bytes(&inputs.backup_directory, &image.original)?;
    let step = WriteStep {
        name: name.to_string(),
        lun: location.lun,
        start_sector: location.start_sector,
        sectors: location.sectors,
        source: StepSource::CandidateFile {
            file: image.candidate.file.clone(),
        },
        bytes,
        sha256: image.candidate.sha256.clone(),
        expected_before_sha256: image.original.sha256.clone(),
        requires_data_wipe_approval: false,
    };
    Ok(PlannedTarget {
        step,
        restore: restore_entry(name, location, &image.original),
        payload: None,
    })
}

fn metadata_target(inputs: &InstallInputs) -> Result<PlannedTarget> {
    let name = METADATA_PARTITION;
    let original = &inputs.metadata_original;
    let location = inputs.partition_map.locate(name)?;
    let bytes = partition_bytes(name, location)?;
    require_whole_partition(name, "original backup", original, bytes)?;
    require_file_bytes(&inputs.backup_directory, original)?;
    let step = WriteStep {
        name: name.to_string(),
        lun: location.lun,
        start_sector: location.start_sector,
        sectors: location.sectors,
        source: StepSource::ZeroFill,
        bytes,
        sha256: zero_sha256(bytes),
        expected_before_sha256: original.sha256.clone(),
        requires_data_wipe_approval: false,
    };
    Ok(PlannedTarget {
        step,
        restore: restore_entry(name, location, original),
        payload: None,
    })
}

fn zero_sha256(bytes: u64) -> String {
    let block = vec![0u8; ZERO_BLOCK_BYTES as usize];
    let mut hasher = Sha256::new();
    let mut remaining = bytes;
    while remaining > 0 {
        let count = remaining.min(ZERO_BLOCK_BYTES);
        hasher.update(&block[..count as usize]);
        remaining -= count;
    }
    hex(&hasher.finalize())
}

fn activation_for(inputs: &InstallInputs, lun: u8) -> Result<&SlotActivation> {
    let activations = [&inputs.activation_lun0, &inputs.activation_lun4];
    ACTIVATION_LUNS
        .iter()
        .zip(activations)
        .find(|(expected, activation)| **expected == lun && activation.lun == lun)
        .map(|(_, activation)| activation)
        .ok_or_else(|| {
            Front2Error::Plan(format!(
                "the inputs hold no slot activation that was prepared for LUN {lun}"
            ))
        })
}

fn require_output_fits(name: &str, output: &GptOutput, record: &GptCopyRecord) -> Result<()> {
    let fits = output.lun == record.lun
        && output.copy == record.copy
        && output.start_sector == record.start_sector
        && output.sectors == record.sectors
        && output.bytes == record.bytes
        && output.restore_sha256 == record.sha256
        && output.data.len() as u64 == output.bytes
        && sha256_bytes(&output.data) == output.sha256;
    if !fits {
        return Err(Front2Error::Plan(format!(
            "the slot activation output for {name} does not fit the GPT copy in the backup"
        )));
    }
    Ok(())
}

fn gpt_target(inputs: &InstallInputs, kind: GptCopyKind, lun: u8) -> Result<PlannedTarget> {
    let name = gpt_step_name(kind, lun);
    let file = gpt_plan_file(kind, lun);
    let output = activation_for(inputs, lun)?.output(kind)?;
    let (record, original) = read_gpt_backup(inputs, lun, kind)?;
    require_output_fits(&name, output, &record)?;
    require_current_copy_matches(kind, &original, &output.data)?;
    let step = WriteStep {
        name: name.clone(),
        lun,
        start_sector: record.start_sector,
        sectors: record.sectors,
        source: StepSource::PlanFile { file: file.clone() },
        bytes: output.bytes,
        sha256: output.sha256.clone(),
        expected_before_sha256: record.sha256.clone(),
        requires_data_wipe_approval: false,
    };
    let restore = RestoreEntry {
        name,
        lun,
        start_sector: record.start_sector,
        sectors: record.sectors,
        file: record.file,
        bytes: record.bytes,
        sha256: record.sha256,
    };
    Ok(PlannedTarget {
        step,
        restore,
        payload: Some(PlanPayload {
            file,
            data: output.data.clone(),
        }),
    })
}

fn read_first_sector(path: &Path) -> Result<Vec<u8>> {
    let mut sector = vec![0u8; SECTOR_BYTES];
    fs::File::open(path)?.read_exact(&mut sector)?;
    Ok(sector)
}

fn misc_target(inputs: &InstallInputs) -> Result<PlannedTarget> {
    let name = MISC_PARTITION;
    let original = &inputs.misc_original;
    let location = inputs.partition_map.locate(name)?;
    let bytes = partition_bytes(name, location)?;
    require_whole_partition(name, "original backup", original, bytes)?;
    original.verify(&inputs.backup_directory)?;
    let original_sector = read_first_sector(&inputs.backup_directory.join(&original.file))?;
    if misc_wipe_sector(&original_sector)? != inputs.misc_first_sector {
        return Err(Front2Error::Plan(
            "the new first misc sector is not the data wipe request that the backup gives".into(),
        ));
    }
    let step = WriteStep {
        name: name.to_string(),
        lun: location.lun,
        start_sector: location.start_sector,
        sectors: MISC_STEP_SECTORS,
        source: StepSource::PlanFile {
            file: MISC_PLAN_FILE.to_string(),
        },
        bytes: inputs.misc_first_sector.len() as u64,
        sha256: sha256_bytes(&inputs.misc_first_sector),
        expected_before_sha256: sha256_bytes(&original_sector),
        requires_data_wipe_approval: true,
    };
    Ok(PlannedTarget {
        step,
        restore: restore_entry(name, location, original),
        payload: Some(PlanPayload {
            file: MISC_PLAN_FILE.to_string(),
            data: inputs.misc_first_sector.clone(),
        }),
    })
}
