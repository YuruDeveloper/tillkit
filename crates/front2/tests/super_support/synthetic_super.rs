use super::synthetic_partition::SyntheticPartition;
use super::synthetic_slot::SyntheticSlot;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const SUPER_BYTES: u64 = 8 * 1024 * 1024;
pub const MAXIMUM_SIZE: u32 = 8192;
pub const FIRST_LOGICAL_SECTOR: u64 = 2048;
pub const ALIGNMENT_BYTES: u32 = 65536;
pub const GROUP_MAXIMUM_BYTES: u64 = 5 * 1024 * 1024;

const SLOT_COUNT: u32 = 3;
const GEOMETRY_OFFSET: usize = 4096;
const GEOMETRY_BACKUP_OFFSET: usize = 8192;
const GEOMETRY_BLOCK_BYTES: usize = 4096;
const METADATA_OFFSET: usize = 12288;
const HEADER_BYTES: usize = 256;
const NAME_BYTES: usize = 36;
const HEADER_FLAGS: u32 = 1;
const READ_ONLY_SLOT_SUFFIXED: u32 = 5;
const DYNAMIC_GROUP_INDEX: u32 = 1;
const COW_GROUP_INDEX: u32 = 2;

#[derive(Debug, Clone)]
pub struct SyntheticSuper {
    pub slots: Vec<SyntheticSlot>,
}

impl SyntheticSuper {
    pub fn standard() -> Self {
        Self {
            slots: vec![slot('a'), slot('b'), merged_slot()],
        }
    }

    pub fn primary_offset(slot: u32) -> u64 {
        METADATA_OFFSET as u64 + u64::from(slot) * u64::from(MAXIMUM_SIZE)
    }

    pub fn backup_offset(slot: u32) -> u64 {
        Self::primary_offset(slot + SLOT_COUNT)
    }

    pub fn image(&self) -> Vec<u8> {
        let mut image: Vec<u8> = (0..SUPER_BYTES as u32).map(filler).collect();
        let mut geometry = geometry();
        geometry.resize(GEOMETRY_BLOCK_BYTES, 0);
        for offset in [GEOMETRY_OFFSET, GEOMETRY_BACKUP_OFFSET] {
            image[offset..offset + geometry.len()].copy_from_slice(&geometry);
        }
        for (index, slot) in self.slots.iter().enumerate() {
            let blob = metadata_blob(slot);
            let primary = Self::primary_offset(index as u32) as usize;
            let backup = Self::backup_offset(index as u32) as usize;
            image[primary..primary + blob.len()].copy_from_slice(&blob);
            image[backup..backup + blob.len()].copy_from_slice(&blob);
        }
        image
    }

    pub fn write(&self, path: &Path) {
        std::fs::write(path, self.image()).expect("write the synthetic super");
    }
}

pub fn gsi_image(bytes: u64) -> Vec<u8> {
    (0..bytes as u32)
        .map(|index| filler(index.wrapping_add(0x5EED_0000)) ^ 0xA5)
        .collect()
}

fn filler(index: u32) -> u8 {
    (index.wrapping_mul(2_654_435_761) >> 24) as u8
}

fn slot(suffix: char) -> SyntheticSlot {
    let name = |base: &str| format!("{base}_{suffix}");
    let dynamic = |base: &str, extents: &[(u64, u64)]| {
        SyntheticPartition::linear(
            &name(base),
            READ_ONLY_SLOT_SUFFIXED,
            DYNAMIC_GROUP_INDEX,
            extents,
        )
    };
    SyntheticSlot {
        groups: vec![
            ("default".to_string(), 0),
            (
                format!("qti_dynamic_partitions_{suffix}"),
                GROUP_MAXIMUM_BYTES,
            ),
            ("cow".to_string(), 0),
        ],
        block_device_count: 1,
        partitions: vec![
            dynamic("odm", &[(2048, 64)]),
            dynamic("product", &[(3200, 256), (8192, 128)]),
            dynamic("system", &[(2176, 1024), (4096, 512)]),
            dynamic("system_dlkm", &[(3456, 8)]),
            dynamic("system_ext", &[(4608, 2048)]),
            dynamic("vendor", &[(6656, 512)]),
            dynamic("vendor_dlkm", &[(7168, 256)]),
            SyntheticPartition::linear(
                &format!("{}-cow", name("system")),
                0,
                COW_GROUP_INDEX,
                &[(7680, 128)],
            ),
            SyntheticPartition::zero(
                &format!("{}-cow", name("vendor")),
                COW_GROUP_INDEX,
                2176,
                128,
            ),
        ],
    }
}

fn merged_slot() -> SyntheticSlot {
    let empty =
        |name: &str, group_index: u32| SyntheticPartition::linear(name, 1, group_index, &[]);
    SyntheticSlot {
        groups: vec![
            ("default".to_string(), 0),
            ("qti_dynamic_partitions_a".to_string(), GROUP_MAXIMUM_BYTES),
            ("qti_dynamic_partitions_b".to_string(), GROUP_MAXIMUM_BYTES),
        ],
        block_device_count: 1,
        partitions: vec![empty("system_a", 1), empty("system_b", 2)],
    }
}

fn geometry() -> Vec<u8> {
    let mut geometry = Vec::new();
    geometry.extend_from_slice(&0x616C_4467u32.to_le_bytes());
    geometry.extend_from_slice(&52u32.to_le_bytes());
    geometry.extend_from_slice(&[0u8; 32]);
    geometry.extend_from_slice(&MAXIMUM_SIZE.to_le_bytes());
    geometry.extend_from_slice(&SLOT_COUNT.to_le_bytes());
    geometry.extend_from_slice(&4096u32.to_le_bytes());
    let checksum = Sha256::digest(&geometry);
    geometry[8..40].copy_from_slice(&checksum);
    geometry
}

fn name_field(name: &str) -> [u8; NAME_BYTES] {
    let mut field = [0u8; NAME_BYTES];
    field[..name.len()].copy_from_slice(name.as_bytes());
    field
}

fn partition_tables(slot: &SyntheticSlot) -> (Vec<u8>, Vec<u8>) {
    let mut partitions = Vec::new();
    let mut extents = Vec::new();
    for partition in &slot.partitions {
        partitions.extend_from_slice(&name_field(&partition.name));
        partitions.extend_from_slice(&partition.attributes.to_le_bytes());
        partitions.extend_from_slice(&((extents.len() / 24) as u32).to_le_bytes());
        partitions.extend_from_slice(&(partition.extents.len() as u32).to_le_bytes());
        partitions.extend_from_slice(&partition.group_index.to_le_bytes());
        for (target_sector, sectors, target_type) in &partition.extents {
            extents.extend_from_slice(&sectors.to_le_bytes());
            extents.extend_from_slice(&target_type.to_le_bytes());
            extents.extend_from_slice(&target_sector.to_le_bytes());
            extents.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    (partitions, extents)
}

fn group_table(slot: &SyntheticSlot) -> Vec<u8> {
    let mut groups = Vec::new();
    for (name, maximum_size) in &slot.groups {
        groups.extend_from_slice(&name_field(name));
        groups.extend_from_slice(&0u32.to_le_bytes());
        groups.extend_from_slice(&maximum_size.to_le_bytes());
    }
    groups
}

fn block_device_table(slot: &SyntheticSlot) -> Vec<u8> {
    let mut devices = Vec::new();
    for _ in 0..slot.block_device_count {
        devices.extend_from_slice(&FIRST_LOGICAL_SECTOR.to_le_bytes());
        devices.extend_from_slice(&ALIGNMENT_BYTES.to_le_bytes());
        devices.extend_from_slice(&0u32.to_le_bytes());
        devices.extend_from_slice(&SUPER_BYTES.to_le_bytes());
        devices.extend_from_slice(&name_field("super"));
        devices.extend_from_slice(&0u32.to_le_bytes());
    }
    devices
}

fn metadata_blob(slot: &SyntheticSlot) -> Vec<u8> {
    let (partitions, extents) = partition_tables(slot);
    let tables = [
        (partitions, 52u32),
        (extents, 24),
        (group_table(slot), 48),
        (block_device_table(slot), 64),
    ];
    let mut header = vec![0u8; HEADER_BYTES];
    header[..4].copy_from_slice(&0x414C_5030u32.to_le_bytes());
    header[4..6].copy_from_slice(&10u16.to_le_bytes());
    header[6..8].copy_from_slice(&2u16.to_le_bytes());
    header[8..12].copy_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
    header[128..132].copy_from_slice(&HEADER_FLAGS.to_le_bytes());
    let mut body = Vec::new();
    for (index, (table, entry_bytes)) in tables.iter().enumerate() {
        let position = 80 + index * 12;
        let count = table.len() as u32 / entry_bytes;
        header[position..position + 4].copy_from_slice(&(body.len() as u32).to_le_bytes());
        header[position + 4..position + 8].copy_from_slice(&count.to_le_bytes());
        header[position + 8..position + 12].copy_from_slice(&entry_bytes.to_le_bytes());
        body.extend_from_slice(table);
    }
    header[44..48].copy_from_slice(&(body.len() as u32).to_le_bytes());
    header[48..80].copy_from_slice(&Sha256::digest(&body));
    let checksum = Sha256::digest(&header);
    header[12..44].copy_from_slice(&checksum);
    let mut blob = header;
    blob.extend_from_slice(&body);
    assert!(blob.len() <= MAXIMUM_SIZE as usize);
    blob.resize(MAXIMUM_SIZE as usize, 0);
    blob
}
