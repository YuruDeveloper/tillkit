use super::lp_extent::LpExtent;
use super::lp_format::{
    BLOCK_DEVICE_ENTRY_BYTES, BLOCK_DEVICE_TABLE_OFFSET, CHECKSUM_BYTES, EXTENT_ENTRY_BYTES,
    EXTENT_TABLE_OFFSET, GROUP_ENTRY_BYTES, GROUP_TABLE_OFFSET, HEADER_CHECKSUM_OFFSET,
    HEADER_PRESERVED_OFFSET, PARTITION_ENTRY_BYTES, PARTITION_TABLE_OFFSET, TABLES_BYTES_OFFSET,
    TABLES_CHECKSUM_OFFSET, put,
};
use super::lp_geometry::LpGeometry;
use super::lp_group::LpGroup;
use super::lp_metadata::{LpMetadata, checked_sum};
use super::lp_partition::LpPartition;
use super::lp_table::LpTable;
use super::partition_summary::PartitionSummary;
use super::super_layout_plan::SuperLayoutPlan;
use super::{
    DEFAULT_GROUP, DYNAMIC_GROUP, REBUILT_GROUP_INDEX, SYSTEM_ATTRIBUTES, SYSTEM_PARTITION,
    TARGET_SLOT,
};
use crate::digest::sha256_bytes;
use crate::error::{Front2Error, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::path::Path;

const SELECTED_PARTITION_COUNT: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RebuiltMetadata {
    #[serde(skip_serializing)]
    pub blob: Vec<u8>,
    pub source_metadata_sha256: String,
    pub candidate_bytes: u64,
    pub candidate_sha256: String,
    pub primary_offset: u64,
    pub backup_offset: u64,
    pub partitions: Vec<PartitionSummary>,
    pub used_bytes: u64,
    pub group_maximum_bytes: u64,
}

impl RebuiltMetadata {
    pub fn from_super_file(super_image: &Path, plan: &SuperLayoutPlan) -> Result<Self> {
        let mut file = File::open(super_image)?;
        let geometry = LpGeometry::read(&mut file)?;
        let original = LpMetadata::read_slot(&mut file, &geometry, TARGET_SLOT)?;
        Self::build(&geometry, &original, plan)
    }

    pub fn build(
        geometry: &LpGeometry,
        original: &LpMetadata,
        plan: &SuperLayoutPlan,
    ) -> Result<Self> {
        require_plan_matches_device(original, plan)?;
        let selected = selected_names(plan)?;
        let (group_index, dynamic_group) = original.group(DYNAMIC_GROUP)?;
        let groups = vec![default_group(original)?.clone(), dynamic_group.clone()];
        let (partitions, extents) = select_partitions(original, plan, &selected, group_index)?;
        let tables = encode_tables(original, &partitions, &extents, &groups);
        let blob = assemble(original, &tables, geometry.maximum_size as usize)?;
        let decoded = LpMetadata::decode(&blob)?;
        let summaries = verify(original, plan, &groups, &decoded)?;
        let used_bytes = require_used_bytes(&summaries, plan, dynamic_group.maximum_size)?;
        Ok(Self {
            source_metadata_sha256: original.sha256.clone(),
            candidate_bytes: blob.len() as u64,
            candidate_sha256: sha256_bytes(&blob),
            primary_offset: geometry.primary_offset(TARGET_SLOT)?,
            backup_offset: geometry.backup_offset(TARGET_SLOT)?,
            partitions: summaries,
            used_bytes,
            group_maximum_bytes: dynamic_group.maximum_size,
            blob,
        })
    }
}

fn failure(reason: String) -> Front2Error {
    Front2Error::Verification(format!("rebuilt super metadata: {reason}"))
}

fn require_plan_matches_device(original: &LpMetadata, plan: &SuperLayoutPlan) -> Result<()> {
    let device = original.single_block_device()?;
    device.alignment_sectors()?;
    let matches = device.size == plan.super_bytes
        && device.alignment == plan.alignment_bytes
        && device.alignment_offset == plan.alignment_offset_bytes;
    if !matches {
        return Err(Front2Error::Plan(format!(
            "the plan is for a super of {} bytes aligned to {} bytes with offset {}, the metadata describes {device:?}",
            plan.super_bytes, plan.alignment_bytes, plan.alignment_offset_bytes
        )));
    }
    Ok(())
}

fn selected_names(plan: &SuperLayoutPlan) -> Result<BTreeSet<String>> {
    let mut selected: BTreeSet<String> = plan
        .retained
        .iter()
        .map(|partition| partition.name.clone())
        .collect();
    selected.insert(SYSTEM_PARTITION.to_string());
    if selected.len() != SELECTED_PARTITION_COUNT {
        return Err(Front2Error::Plan(format!(
            "the plan selects {selected:?}, expected {SELECTED_PARTITION_COUNT} partitions"
        )));
    }
    Ok(selected)
}

fn default_group(original: &LpMetadata) -> Result<&LpGroup> {
    original
        .groups
        .first()
        .filter(|group| group.name.as_str() == DEFAULT_GROUP)
        .ok_or_else(|| {
            Front2Error::Image(format!(
                "super metadata: the first group is not named {DEFAULT_GROUP}"
            ))
        })
}

fn select_partitions(
    original: &LpMetadata,
    plan: &SuperLayoutPlan,
    selected: &BTreeSet<String>,
    group_index: usize,
) -> Result<(Vec<LpPartition>, Vec<LpExtent>)> {
    let mut partitions = Vec::new();
    let mut extents: Vec<LpExtent> = Vec::new();
    for record in &original.partitions {
        if !selected.contains(record.name.as_str()) {
            continue;
        }
        if record.group_index as usize != group_index {
            return Err(Front2Error::Plan(format!(
                "partition {} is in group {}, expected group {group_index}",
                record.name.as_str(),
                record.group_index
            )));
        }
        let first_extent_index = extents.len();
        let attributes = append_extents(original, plan, record, &mut extents)?;
        partitions.push(LpPartition {
            name: record.name.clone(),
            attributes,
            first_extent_index: table_count(first_extent_index)?,
            extent_count: table_count(extents.len() - first_extent_index)?,
            group_index: REBUILT_GROUP_INDEX,
        });
    }
    let names: BTreeSet<String> = partitions
        .iter()
        .map(|partition| partition.name.as_str().to_string())
        .collect();
    if names != *selected || partitions.len() != SELECTED_PARTITION_COUNT {
        return Err(Front2Error::Plan(format!(
            "slot B holds {names:?} in {} records, the plan selects {selected:?}",
            partitions.len()
        )));
    }
    Ok((partitions, extents))
}

fn append_extents(
    original: &LpMetadata,
    plan: &SuperLayoutPlan,
    record: &LpPartition,
    extents: &mut Vec<LpExtent>,
) -> Result<u32> {
    if record.name.as_str() != SYSTEM_PARTITION {
        extents.extend_from_slice(original.extents_of(record)?);
        return Ok(record.attributes);
    }
    for allocation in &plan.allocation {
        allocation.byte_range()?;
        extents.push(LpExtent::linear(
            allocation.target_sector,
            allocation.sectors,
        ));
    }
    Ok(SYSTEM_ATTRIBUTES)
}

fn table_count(count: usize) -> Result<u32> {
    u32::try_from(count).map_err(|_| {
        Front2Error::Plan(format!("{count} extents do not fit a super metadata table"))
    })
}

fn encode_tables(
    original: &LpMetadata,
    partitions: &[LpPartition],
    extents: &[LpExtent],
    groups: &[LpGroup],
) -> [Vec<u8>; 4] {
    [
        partitions.iter().flat_map(LpPartition::encode).collect(),
        extents.iter().flat_map(LpExtent::encode).collect(),
        groups.iter().flat_map(LpGroup::encode).collect(),
        original
            .block_devices
            .iter()
            .flat_map(|device| device.encode())
            .collect(),
    ]
}

fn assemble(original: &LpMetadata, tables: &[Vec<u8>; 4], maximum_size: usize) -> Result<Vec<u8>> {
    let layout = [
        (PARTITION_TABLE_OFFSET, PARTITION_ENTRY_BYTES),
        (EXTENT_TABLE_OFFSET, EXTENT_ENTRY_BYTES),
        (GROUP_TABLE_OFFSET, GROUP_ENTRY_BYTES),
        (BLOCK_DEVICE_TABLE_OFFSET, BLOCK_DEVICE_ENTRY_BYTES),
    ];
    let mut header = original.header.clone();
    let mut body = Vec::new();
    for ((position, entry_bytes), table) in layout.into_iter().zip(tables) {
        let descriptor = LpTable {
            offset: body.len(),
            count: table.len() / entry_bytes,
            entry_bytes,
        };
        descriptor.write(&mut header, position)?;
        body.extend_from_slice(table);
    }
    let body_bytes = table_count(body.len())?;
    put(&mut header, TABLES_BYTES_OFFSET, &body_bytes.to_le_bytes())?;
    put(&mut header, TABLES_CHECKSUM_OFFSET, &Sha256::digest(&body))?;
    put(&mut header, HEADER_CHECKSUM_OFFSET, &[0u8; CHECKSUM_BYTES])?;
    let checksum = Sha256::digest(&header);
    put(&mut header, HEADER_CHECKSUM_OFFSET, &checksum)?;
    if header.len() + body.len() > maximum_size {
        return Err(Front2Error::Plan(format!(
            "the rebuilt metadata needs {} bytes, a metadata slot holds {maximum_size} bytes",
            header.len() + body.len()
        )));
    }
    let mut blob = header;
    blob.extend_from_slice(&body);
    blob.resize(maximum_size, 0);
    Ok(blob)
}

fn verify(
    original: &LpMetadata,
    plan: &SuperLayoutPlan,
    groups: &[LpGroup],
    decoded: &LpMetadata,
) -> Result<Vec<PartitionSummary>> {
    let preserved = decoded.block_devices == original.block_devices
        && decoded.groups == groups
        && decoded.header.get(HEADER_PRESERVED_OFFSET..)
            == original.header.get(HEADER_PRESERVED_OFFSET..);
    if !preserved {
        return Err(failure(
            "the block device table, the groups or the header tail changed".into(),
        ));
    }
    let mut ranges = Vec::new();
    let mut used = vec![false; decoded.extents.len()];
    let mut summaries = Vec::new();
    for partition in &decoded.partitions {
        mark_used(partition, &mut used)?;
        summaries.push(verify_partition(
            original,
            plan,
            decoded,
            partition,
            &mut ranges,
        )?);
    }
    ranges.sort();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(failure(format!("extents overlap: {ranges:?}")));
    }
    if !used.iter().all(|used| *used) {
        return Err(failure("an extent belongs to no partition".into()));
    }
    Ok(summaries)
}

fn mark_used(partition: &LpPartition, used: &mut [bool]) -> Result<()> {
    let first = partition.first_extent_index as usize;
    let count = partition.extent_count as usize;
    let slots = first
        .checked_add(count)
        .and_then(|end| used.get_mut(first..end))
        .ok_or_else(|| failure("a partition names extents outside the table".into()))?;
    if slots.iter().any(|used| *used) {
        return Err(failure(format!(
            "partition {} shares an extent with another partition",
            partition.name.as_str()
        )));
    }
    slots.fill(true);
    Ok(())
}

fn verify_partition(
    original: &LpMetadata,
    plan: &SuperLayoutPlan,
    decoded: &LpMetadata,
    partition: &LpPartition,
    ranges: &mut Vec<(u64, u64)>,
) -> Result<PartitionSummary> {
    let name = partition.name.as_str();
    let is_system = name == SYSTEM_PARTITION;
    let bytes = verify_extents(decoded, partition, is_system, ranges)?;
    let expected_bytes = expected_bytes(plan, name)?;
    if partition.group_index != REBUILT_GROUP_INDEX || bytes != expected_bytes {
        return Err(failure(format!(
            "partition {name} is in group {} with {bytes} bytes, expected group {REBUILT_GROUP_INDEX} with {expected_bytes} bytes",
            partition.group_index
        )));
    }
    if !is_system {
        require_retained_unchanged(original, decoded, partition)?;
    }
    Ok(PartitionSummary {
        name: name.to_string(),
        attributes: partition.attributes,
        bytes,
        extent_count: partition.extent_count,
    })
}

fn verify_extents(
    decoded: &LpMetadata,
    partition: &LpPartition,
    is_system: bool,
    ranges: &mut Vec<(u64, u64)>,
) -> Result<u64> {
    let device = decoded.single_block_device()?;
    let alignment_sectors = device.alignment_sectors()?;
    let mut bytes = 0u64;
    for extent in decoded.extents_of(partition)? {
        extent.require_data_extent(device.first_logical_sector, device.last_sector())?;
        if is_system && !extent.target_sector.is_multiple_of(alignment_sectors) {
            return Err(failure(format!(
                "{extent:?} of {SYSTEM_PARTITION} does not start on a multiple of {alignment_sectors} sectors"
            )));
        }
        bytes = checked_sum(bytes, extent.bytes()?)?;
        ranges.push((extent.target_sector, extent.end_sector()?));
    }
    Ok(bytes)
}

fn expected_bytes(plan: &SuperLayoutPlan, name: &str) -> Result<u64> {
    if name == SYSTEM_PARTITION {
        return Ok(plan.gsi_bytes);
    }
    plan.retained
        .iter()
        .find(|partition| partition.name == name)
        .map(|partition| partition.bytes)
        .ok_or_else(|| failure(format!("the plan does not retain partition {name}")))
}

fn require_retained_unchanged(
    original: &LpMetadata,
    decoded: &LpMetadata,
    partition: &LpPartition,
) -> Result<()> {
    let source = original.partition(partition.name.as_str())?;
    let unchanged = partition.attributes == source.attributes
        && decoded.extents_of(partition)? == original.extents_of(source)?;
    if !unchanged {
        return Err(failure(format!(
            "retained partition {} changed its attributes or extents",
            partition.name.as_str()
        )));
    }
    Ok(())
}

fn require_used_bytes(
    summaries: &[PartitionSummary],
    plan: &SuperLayoutPlan,
    maximum_bytes: u64,
) -> Result<u64> {
    let mut used_bytes = 0u64;
    for summary in summaries {
        used_bytes = checked_sum(used_bytes, summary.bytes)?;
    }
    if used_bytes != plan.used_bytes || (maximum_bytes != 0 && used_bytes > maximum_bytes) {
        return Err(failure(format!(
            "the partitions use {used_bytes} bytes, the plan records {} and the group allows {maximum_bytes}",
            plan.used_bytes
        )));
    }
    Ok(used_bytes)
}
