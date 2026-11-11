use super::lp_block_device::LpBlockDevice;
use super::lp_extent::LpExtent;
use super::lp_format::{SECTOR_BYTES, TARGET_TYPE_LINEAR, sectors_to_bytes};
use super::lp_geometry::LpGeometry;
use super::lp_metadata::{LpMetadata, checked_sum};
use super::partition_overlap::PartitionOverlap;
use super::planned_extent::PlannedExtent;
use super::retained_partition::RetainedPartition;
use super::{
    DYNAMIC_GROUP, EXTENT_ALIGNMENT_SECTORS, GSI_BLOCK_BYTES, ORIGINAL_SLOT, RETAINED_PARTITIONS,
    SYSTEM_PARTITION, TARGET_SLOT,
};
use crate::error::{Front2Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuperLayoutPlan {
    pub gsi_bytes: u64,
    pub super_bytes: u64,
    pub retained: Vec<RetainedPartition>,
    pub allocation: Vec<PlannedExtent>,
    pub alignment_bytes: u32,
    pub alignment_offset_bytes: u32,
    pub used_bytes: u64,
    pub group_maximum_bytes: u64,
    pub original_a_overlaps: Vec<PartitionOverlap>,
}

impl SuperLayoutPlan {
    pub fn from_super_file(super_image: &Path, gsi_bytes: u64) -> Result<Self> {
        let mut file = File::open(super_image)?;
        let geometry = LpGeometry::read(&mut file)?;
        let target = LpMetadata::read_slot(&mut file, &geometry, TARGET_SLOT)?;
        let original_a = LpMetadata::read_slot(&mut file, &geometry, ORIGINAL_SLOT)?;
        Self::plan(&target, &original_a, gsi_bytes)
    }

    pub fn plan(target: &LpMetadata, original_a: &LpMetadata, gsi_bytes: u64) -> Result<Self> {
        if gsi_bytes == 0 || !gsi_bytes.is_multiple_of(GSI_BLOCK_BYTES) {
            return Err(Front2Error::Plan(format!(
                "a GSI of {gsi_bytes} bytes is not a positive multiple of {GSI_BLOCK_BYTES} bytes"
            )));
        }
        target.require_known_extent_targets()?;
        original_a.require_known_extent_targets()?;
        let device = target.single_block_device()?;
        let alignment_sectors = device.alignment_sectors()?;
        let (retained, ranges) = retained_ranges(target, device)?;
        let free = free_ranges(ranges, device.first_logical_sector, device.last_sector())?;
        let allocation = allocate(&free, alignment_sectors, gsi_bytes / SECTOR_BYTES)?;
        let used_bytes = used_bytes(&retained, gsi_bytes)?;
        let (_, group) = target.group(DYNAMIC_GROUP)?;
        require_group_fits(used_bytes, group.maximum_size)?;
        require_aligned_system(target, alignment_sectors)?;
        Ok(Self {
            gsi_bytes,
            super_bytes: device.size,
            retained,
            original_a_overlaps: overlaps(original_a, &allocation)?,
            allocation,
            alignment_bytes: device.alignment,
            alignment_offset_bytes: device.alignment_offset,
            used_bytes,
            group_maximum_bytes: group.maximum_size,
        })
    }
}

type NamedRange = (u64, u64, String);

fn retained_ranges(
    target: &LpMetadata,
    device: &LpBlockDevice,
) -> Result<(Vec<RetainedPartition>, Vec<NamedRange>)> {
    let mut retained = Vec::new();
    let mut ranges = Vec::new();
    for name in RETAINED_PARTITIONS {
        target.partition(name)?;
    }
    for partition in &target.partitions {
        let name = partition.name.as_str();
        if !RETAINED_PARTITIONS.contains(&name) {
            continue;
        }
        for extent in target.extents_of(partition)? {
            ranges.push(retained_range(extent, device, name)?);
        }
        retained.push(RetainedPartition {
            name: name.to_string(),
            bytes: target.partition_bytes(partition)?,
        });
    }
    Ok((retained, ranges))
}

fn retained_range(extent: &LpExtent, device: &LpBlockDevice, name: &str) -> Result<NamedRange> {
    let start = extent.target_sector;
    let end = extent.end_sector()?;
    let inside = device.first_logical_sector <= start && start < end && end <= device.last_sector();
    if !extent.is_linear_on_first_device() || !inside {
        return Err(Front2Error::Plan(format!(
            "retained partition {name} has {extent:?}, which is not a linear extent inside sectors {}..{}",
            device.first_logical_sector,
            device.last_sector()
        )));
    }
    Ok((start, end, name.to_string()))
}

fn free_ranges(mut ranges: Vec<NamedRange>, first: u64, last: u64) -> Result<Vec<(u64, u64)>> {
    ranges.sort();
    let mut free = Vec::new();
    let mut cursor = first;
    for (start, end, name) in ranges {
        if cursor > start {
            return Err(Front2Error::Plan(format!(
                "retained partition {name} overlaps another retained partition at sector {start}"
            )));
        }
        if cursor < start {
            free.push((cursor, start));
        }
        cursor = end;
    }
    if cursor < last {
        free.push((cursor, last));
    }
    Ok(free)
}

fn allocate(
    free: &[(u64, u64)],
    alignment_sectors: u64,
    gsi_sectors: u64,
) -> Result<Vec<PlannedExtent>> {
    let mut remaining = gsi_sectors;
    let mut allocation = Vec::new();
    for (start, end) in free {
        let aligned = start
            .div_ceil(alignment_sectors)
            .checked_mul(alignment_sectors)
            .ok_or_else(|| Front2Error::Plan(format!("sector {start} cannot be aligned")))?;
        let capacity =
            end.saturating_sub(aligned) / EXTENT_ALIGNMENT_SECTORS * EXTENT_ALIGNMENT_SECTORS;
        let count = remaining.min(capacity);
        if count == 0 {
            continue;
        }
        allocation.push(PlannedExtent::from_sectors(aligned, count)?);
        remaining -= count;
    }
    if remaining != 0 {
        return Err(Front2Error::Plan(format!(
            "the GSI needs {gsi_sectors} sectors, the free aligned space of slot B is {remaining} sectors short"
        )));
    }
    Ok(allocation)
}

fn used_bytes(retained: &[RetainedPartition], gsi_bytes: u64) -> Result<u64> {
    let mut total = gsi_bytes;
    for partition in retained {
        total = checked_sum(total, partition.bytes)?;
    }
    Ok(total)
}

fn require_group_fits(used_bytes: u64, maximum_bytes: u64) -> Result<()> {
    if maximum_bytes != 0 && used_bytes > maximum_bytes {
        return Err(Front2Error::Plan(format!(
            "the retained partitions and the GSI use {used_bytes} bytes, group {DYNAMIC_GROUP} allows {maximum_bytes} bytes"
        )));
    }
    Ok(())
}

fn require_aligned_system(target: &LpMetadata, alignment_sectors: u64) -> Result<()> {
    let system = target.partition(SYSTEM_PARTITION)?;
    let aligned = |extent: &LpExtent| {
        extent.target_type == TARGET_TYPE_LINEAR
            && extent.target_sector.is_multiple_of(alignment_sectors)
    };
    if !target.extents_of(system)?.iter().all(aligned) {
        return Err(Front2Error::Plan(format!(
            "{SYSTEM_PARTITION} has an extent that is not linear or does not start on a multiple of {alignment_sectors} sectors"
        )));
    }
    Ok(())
}

fn overlaps(
    original_a: &LpMetadata,
    allocation: &[PlannedExtent],
) -> Result<Vec<PartitionOverlap>> {
    let mut overlaps = Vec::new();
    for partition in &original_a.partitions {
        let mut overlap_sectors = 0u64;
        for extent in original_a.extents_of(partition)? {
            overlap_sectors = checked_sum(overlap_sectors, overlap_with(extent, allocation)?)?;
        }
        if overlap_sectors == 0 {
            continue;
        }
        overlaps.push(PartitionOverlap {
            partition: partition.name.as_str().to_string(),
            overlap_bytes: sectors_to_bytes(overlap_sectors)?,
        });
    }
    Ok(overlaps)
}

fn overlap_with(extent: &LpExtent, allocation: &[PlannedExtent]) -> Result<u64> {
    if extent.target_type != TARGET_TYPE_LINEAR {
        return Ok(0);
    }
    let end = extent.end_sector()?;
    let mut total = 0u64;
    for proposed in allocation {
        let shared_end = end.min(proposed.end_sector());
        let shared_start = extent.target_sector.max(proposed.target_sector);
        total = checked_sum(total, shared_end.saturating_sub(shared_start))?;
    }
    Ok(total)
}
