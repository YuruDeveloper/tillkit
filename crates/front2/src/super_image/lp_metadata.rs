use super::file_ranges::read_at;
use super::lp_block_device::LpBlockDevice;
use super::lp_extent::LpExtent;
use super::lp_format::{
    BLOCK_DEVICE_ENTRY_BYTES, BLOCK_DEVICE_TABLE_OFFSET, CHECKSUM_BYTES, EXTENT_ENTRY_BYTES,
    EXTENT_TABLE_OFFSET, GROUP_ENTRY_BYTES, GROUP_TABLE_OFFSET, HEADER_BYTES,
    HEADER_CHECKSUM_OFFSET, HEADER_MAGIC, MAJOR_VERSION, MINOR_VERSION, PARTITION_ENTRY_BYTES,
    PARTITION_TABLE_OFFSET, TABLES_BYTES_OFFSET, TABLES_CHECKSUM_OFFSET, TARGET_TYPE_LINEAR,
    TARGET_TYPE_ZERO, bytes_at, u16_at, u32_at,
};
use super::lp_geometry::LpGeometry;
use super::lp_group::LpGroup;
use super::lp_partition::LpPartition;
use super::lp_table::LpTable;
use crate::digest::{hex, sha256_bytes};
use crate::error::{Front2Error, Result};
use std::io::{Read, Seek};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LpMetadata {
    pub header: Vec<u8>,
    pub sha256: String,
    pub partitions: Vec<LpPartition>,
    pub extents: Vec<LpExtent>,
    pub groups: Vec<LpGroup>,
    pub block_devices: Vec<LpBlockDevice>,
}

impl LpMetadata {
    pub fn decode(blob: &[u8]) -> Result<Self> {
        let header = checked_header(blob)?;
        let tables = checked_tables(blob)?;
        let partitions = table(header, PARTITION_TABLE_OFFSET, tables)?;
        let extents = table(header, EXTENT_TABLE_OFFSET, tables)?;
        let groups = table(header, GROUP_TABLE_OFFSET, tables)?;
        let block_devices = table(header, BLOCK_DEVICE_TABLE_OFFSET, tables)?;
        require_disjoint(&[partitions, extents, groups, block_devices])?;
        let metadata = Self {
            header: header.to_vec(),
            sha256: sha256_bytes(blob),
            partitions: decode_records(&partitions, tables, LpPartition::decode)?,
            extents: decode_records(&extents, tables, LpExtent::decode)?,
            groups: decode_records(&groups, tables, LpGroup::decode)?,
            block_devices: decode_records(&block_devices, tables, LpBlockDevice::decode)?,
        };
        for partition in &metadata.partitions {
            metadata.extents_of(partition)?;
        }
        Ok(metadata)
    }

    pub fn read_slot<Source: Read + Seek>(
        source: &mut Source,
        geometry: &LpGeometry,
        slot: u32,
    ) -> Result<Self> {
        let length = geometry.maximum_size as usize;
        let primary = read_at(source, geometry.primary_offset(slot)?, length)?;
        let backup = read_at(source, geometry.backup_offset(slot)?, length)?;
        if primary != backup {
            return Err(Front2Error::Image(format!(
                "super metadata: the primary and backup copies of slot {slot} differ"
            )));
        }
        Self::decode(&primary)
    }

    pub fn extents_of(&self, partition: &LpPartition) -> Result<&[LpExtent]> {
        let first = partition.first_extent_index as usize;
        first
            .checked_add(partition.extent_count as usize)
            .and_then(|end| self.extents.get(first..end))
            .ok_or_else(|| {
                Front2Error::Image(format!(
                    "super metadata: partition {} names {} extents from index {first}, the table holds {}",
                    partition.name.as_str(),
                    partition.extent_count,
                    self.extents.len()
                ))
            })
    }

    pub fn partition_bytes(&self, partition: &LpPartition) -> Result<u64> {
        let mut total = 0u64;
        for extent in self.extents_of(partition)? {
            total = checked_sum(total, extent.bytes()?)?;
        }
        Ok(total)
    }

    pub fn partition(&self, name: &str) -> Result<&LpPartition> {
        let mut matches = self
            .partitions
            .iter()
            .filter(|partition| partition.name.as_str() == name);
        match (matches.next(), matches.next()) {
            (Some(partition), None) => Ok(partition),
            _ => Err(Front2Error::Image(format!(
                "super metadata: there is not exactly one partition named {name}"
            ))),
        }
    }

    pub fn group(&self, name: &str) -> Result<(usize, &LpGroup)> {
        self.groups
            .iter()
            .enumerate()
            .find(|(_, group)| group.name.as_str() == name)
            .ok_or_else(|| {
                Front2Error::Image(format!("super metadata: there is no group named {name}"))
            })
    }

    pub fn single_block_device(&self) -> Result<&LpBlockDevice> {
        match self.block_devices.as_slice() {
            [device] => Ok(device),
            devices => Err(Front2Error::Image(format!(
                "super metadata: {} block devices, the Front 2 layout has exactly one",
                devices.len()
            ))),
        }
    }

    pub fn require_known_extent_targets(&self) -> Result<()> {
        for partition in &self.partitions {
            let extents = self.extents_of(partition)?;
            require_known_targets(partition.name.as_str(), extents)?;
        }
        Ok(())
    }
}

pub fn checked_sum(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right).ok_or_else(|| {
        Front2Error::Image(format!(
            "super metadata: {left} plus {right} bytes overflows"
        ))
    })
}

fn checked_header(blob: &[u8]) -> Result<&[u8]> {
    let failure = |reason: &str| Front2Error::Image(format!("super metadata header: {reason}"));
    let mut zeroed: [u8; HEADER_BYTES] = bytes_at(blob, 0)?;
    let stored: [u8; CHECKSUM_BYTES] = bytes_at(blob, HEADER_CHECKSUM_OFFSET)?;
    zeroed[HEADER_CHECKSUM_OFFSET..HEADER_CHECKSUM_OFFSET + CHECKSUM_BYTES].fill(0);
    let expected_shape = u32_at(blob, 0)? == HEADER_MAGIC
        && u16_at(blob, 4)? == MAJOR_VERSION
        && u16_at(blob, 6)? == MINOR_VERSION
        && u32_at(blob, 8)? as usize == HEADER_BYTES;
    if !expected_shape {
        return Err(failure(
            "the magic, the version or the header size is not LP 10.2 with 256 bytes",
        ));
    }
    if sha256_bytes(&zeroed) != hex(&stored) {
        return Err(failure("the checksum does not match"));
    }
    blob.get(..HEADER_BYTES)
        .ok_or_else(|| failure("the blob is shorter than the header"))
}

fn checked_tables(blob: &[u8]) -> Result<&[u8]> {
    let failure = |reason: &str| Front2Error::Image(format!("super metadata tables: {reason}"));
    let tables_bytes = u32_at(blob, TABLES_BYTES_OFFSET)? as usize;
    let stored: [u8; CHECKSUM_BYTES] = bytes_at(blob, TABLES_CHECKSUM_OFFSET)?;
    let tables = HEADER_BYTES
        .checked_add(tables_bytes)
        .and_then(|end| blob.get(HEADER_BYTES..end))
        .ok_or_else(|| failure("the tables end after the metadata slot"))?;
    if sha256_bytes(tables) != hex(&stored) {
        return Err(failure("the checksum does not match"));
    }
    Ok(tables)
}

fn table(header: &[u8], position: usize, tables: &[u8]) -> Result<LpTable> {
    let entry_bytes = match position {
        PARTITION_TABLE_OFFSET => PARTITION_ENTRY_BYTES,
        EXTENT_TABLE_OFFSET => EXTENT_ENTRY_BYTES,
        GROUP_TABLE_OFFSET => GROUP_ENTRY_BYTES,
        _ => BLOCK_DEVICE_ENTRY_BYTES,
    };
    LpTable::read(header, position, entry_bytes, tables.len())
}

fn require_disjoint(tables: &[LpTable]) -> Result<()> {
    let mut ranges: Vec<(usize, usize)> = tables
        .iter()
        .map(|table| (table.offset, table.end()))
        .collect();
    ranges.sort();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(Front2Error::Image(format!(
            "super metadata tables overlap: {ranges:?}"
        )));
    }
    Ok(())
}

fn decode_records<Record>(
    table: &LpTable,
    tables: &[u8],
    decode: fn(&[u8]) -> Result<Record>,
) -> Result<Vec<Record>> {
    table.records(tables)?.into_iter().map(decode).collect()
}

fn require_known_targets(name: &str, extents: &[LpExtent]) -> Result<()> {
    let known = |extent: &LpExtent| {
        extent.target_source == 0
            && (extent.target_type == TARGET_TYPE_LINEAR || extent.target_type == TARGET_TYPE_ZERO)
    };
    if !extents.iter().all(known) {
        return Err(Front2Error::Image(format!(
            "super metadata: partition {name} has an extent that is neither linear nor zero on the first block device"
        )));
    }
    Ok(())
}
