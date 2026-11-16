use super::byte_range::ByteRange;
use super::file_ranges::{compare_outside, hash_range};
use super::logical_partition_record::LogicalPartitionRecord;
use super::lp_format::{HEADER_PRESERVED_OFFSET, SECTOR_BYTES};
use super::lp_geometry::LpGeometry;
use super::lp_metadata::{LpMetadata, checked_sum};
use super::lp_partition::LpPartition;
use super::super_verification::SuperVerification;
use super::super_verification_request::SuperVerificationRequest;
use super::{REBUILT_GROUP_INDEX, SYSTEM_ATTRIBUTES, SuperSlot};
use crate::digest::{hex, sha256_file};
use crate::error::{Front2Error, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;

pub fn verify_super_candidate(
    request: &SuperVerificationRequest<'_>,
    slot: &SuperSlot,
) -> Result<SuperVerification> {
    let mut candidate = File::open(request.candidate)?;
    let mut original = File::open(request.original_super)?;
    let candidate_bytes = candidate.metadata()?.len();
    let original_bytes = original.metadata()?.len();
    if candidate_bytes != original_bytes {
        return Err(failure(format!(
            "the candidate is {candidate_bytes} bytes, the original super is {original_bytes} bytes"
        )));
    }
    let geometry = LpGeometry::read(&mut candidate)?;
    let metadata = LpMetadata::read_slot(&mut candidate, &geometry, slot.target_index())?;
    let original_geometry = LpGeometry::read(&mut original)?;
    let source = LpMetadata::read_slot(&mut original, &original_geometry, slot.target_index())?;
    require_expected_tables(&metadata, &source, slot)?;
    let slot_bytes = u64::from(geometry.maximum_size);
    let mut allowed = vec![
        ByteRange::from_length(geometry.primary_offset(slot.target_index())?, slot_bytes)?,
        ByteRange::from_length(geometry.backup_offset(slot.target_index())?, slot_bytes)?,
    ];
    let mut ranges = Vec::new();
    let mut logical_partitions = Vec::new();
    for partition in &metadata.partitions {
        let is_system = partition.name.as_str() == slot.system();
        let extents = partition_ranges(&metadata, &source, partition, candidate_bytes)?;
        require_expected_record(&metadata, &source, partition, slot)?;
        let twin = if is_system { None } else { Some(&mut original) };
        let (bytes, sha256) = hash_extents(&mut candidate, twin, &extents)?;
        require_expected_content(request, &source, partition, bytes, &sha256, slot)?;
        if is_system {
            allowed.extend_from_slice(&extents);
        }
        ranges.extend_from_slice(&extents);
        logical_partitions.push(LogicalPartitionRecord {
            partition: partition.name.as_str().to_string(),
            bytes,
            sha256,
            original_preserved: !is_system,
        });
    }
    ByteRange::sort_disjoint(&mut ranges, candidate_bytes, "the partition extents")?;
    ByteRange::sort_disjoint(&mut allowed, candidate_bytes, "the changed ranges")?;
    let outside = compare_outside(&mut original, &mut candidate, &allowed, candidate_bytes)?;
    let (hashed_bytes, candidate_sha256) = sha256_file(request.candidate)?;
    if hashed_bytes != candidate_bytes {
        return Err(failure(
            "the candidate changed its size while it was read".into(),
        ));
    }
    Ok(SuperVerification {
        metadata_sha256: metadata.sha256.clone(),
        logical_partitions,
        allowed_super_ranges: allowed,
        outside_ranges_equal_bytes: outside,
        candidate_bytes,
        candidate_sha256,
    })
}

fn failure(reason: String) -> Front2Error {
    Front2Error::Verification(format!("super candidate: {reason}"))
}

fn require_expected_tables(
    metadata: &LpMetadata,
    source: &LpMetadata,
    slot: &SuperSlot,
) -> Result<()> {
    let names: BTreeSet<&str> = metadata
        .partitions
        .iter()
        .map(|partition| partition.name.as_str())
        .collect();
    let mut expected_names: BTreeSet<&str> = slot.retained().iter().map(String::as_str).collect();
    expected_names.insert(slot.system());
    if names != expected_names || metadata.partitions.len() != expected_names.len() {
        return Err(failure(format!(
            "the target slot holds {names:?} in {} records, expected {expected_names:?}",
            metadata.partitions.len()
        )));
    }
    if metadata.header.get(HEADER_PRESERVED_OFFSET..)
        != source.header.get(HEADER_PRESERVED_OFFSET..)
    {
        return Err(failure(format!(
            "the header from byte {HEADER_PRESERVED_OFFSET} is not the one of the original target slot"
        )));
    }
    source.single_block_device()?;
    let (_, dynamic_group) = source.group(slot.dynamic_group())?;
    let default_group = source.default_group()?;
    let groups_match = match metadata.groups.as_slice() {
        [first, group] => *first == default_group && group == dynamic_group,
        _ => false,
    };
    if !groups_match || metadata.block_devices != source.block_devices {
        return Err(failure(
            "the groups or the block device table are not the rebuilt ones".into(),
        ));
    }
    Ok(())
}

fn partition_ranges(
    metadata: &LpMetadata,
    source: &LpMetadata,
    partition: &LpPartition,
    candidate_bytes: u64,
) -> Result<Vec<ByteRange>> {
    let device = source.single_block_device()?;
    let mut ranges = Vec::new();
    for extent in metadata.extents_of(partition)? {
        extent.require_data_extent(device.first_logical_sector, candidate_bytes / SECTOR_BYTES)?;
        ranges.push(ByteRange::from_sectors(
            extent.target_sector,
            extent.sectors,
        )?);
    }
    Ok(ranges)
}

fn require_expected_record(
    metadata: &LpMetadata,
    source: &LpMetadata,
    partition: &LpPartition,
    slot: &SuperSlot,
) -> Result<()> {
    let name = partition.name.as_str();
    if partition.group_index != REBUILT_GROUP_INDEX {
        return Err(failure(format!(
            "partition {name} is in group {}",
            partition.group_index
        )));
    }
    if name == slot.system() {
        return require_system_attributes(partition, slot);
    }
    let source_partition = source.partition(name)?;
    let unchanged = partition.attributes == source_partition.attributes
        && metadata.extents_of(partition)? == source.extents_of(source_partition)?;
    if !unchanged {
        return Err(failure(format!(
            "retained partition {name} changed its attributes or extents"
        )));
    }
    Ok(())
}

fn require_system_attributes(partition: &LpPartition, slot: &SuperSlot) -> Result<()> {
    if partition.attributes != SYSTEM_ATTRIBUTES {
        return Err(failure(format!(
            "{} has attributes {}, expected {SYSTEM_ATTRIBUTES}",
            slot.system(),
            partition.attributes
        )));
    }
    Ok(())
}

fn hash_extents(
    candidate: &mut File,
    mut twin: Option<&mut File>,
    extents: &[ByteRange],
) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    for range in extents {
        let read = hash_range(candidate, twin.as_deref_mut(), *range, &mut hasher)?;
        total = checked_sum(total, read)?;
    }
    Ok((total, hex(&hasher.finalize())))
}

fn require_expected_content(
    request: &SuperVerificationRequest<'_>,
    source: &LpMetadata,
    partition: &LpPartition,
    bytes: u64,
    sha256: &str,
    slot: &SuperSlot,
) -> Result<()> {
    let name = partition.name.as_str();
    if name != slot.system() {
        let expected_bytes = source.partition_bytes(source.partition(name)?)?;
        return require_bytes(name, bytes, expected_bytes);
    }
    if bytes != request.gsi_bytes || sha256 != request.gsi_sha256 {
        return Err(failure(format!(
            "{} holds {bytes} bytes with SHA-256 {sha256}, the GSI is {} bytes with SHA-256 {}",
            slot.system(),
            request.gsi_bytes,
            request.gsi_sha256
        )));
    }
    Ok(())
}

fn require_bytes(name: &str, bytes: u64, expected_bytes: u64) -> Result<()> {
    if bytes != expected_bytes {
        return Err(failure(format!(
            "partition {name} holds {bytes} bytes, the original holds {expected_bytes} bytes"
        )));
    }
    Ok(())
}
