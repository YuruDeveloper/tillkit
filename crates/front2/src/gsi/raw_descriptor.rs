use crate::error::{Front2Error, Result};
use avbtool_rs::info::DescriptorInfo;
use std::path::Path;

const VBMETA_HEADER_BYTES: usize = 256;
const DESCRIPTOR_HEADER_BYTES: usize = 16;
const DESCRIPTOR_ALIGNMENT: usize = 8;
const CHAIN_PARTITION_TAG: u64 = 4;
const CHAIN_PARTITION_FIXED_BYTES: usize = 92;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDescriptor {
    pub info: DescriptorInfo,
    pub bytes: Vec<u8>,
}

impl RawDescriptor {
    pub fn read_all(image: &Path) -> Result<Vec<Self>> {
        let failure = |reason: String| {
            Front2Error::Image(format!("{} descriptors: {reason}", image.display()))
        };
        let info = avbtool_rs::image::inspect_avb_image(image)
            .map_err(|error| failure(error.to_string()))?;
        let blob = avbtool_rs::image::load_vbmeta_blob(image)
            .map_err(|error| failure(error.to_string()))?;
        let header = &info.header;
        let start = VBMETA_HEADER_BYTES as u64
            + header.authentication_data_block_size
            + header.descriptors_offset;
        let region = usize::try_from(start)
            .ok()
            .zip(usize::try_from(header.descriptors_size).ok())
            .and_then(|(start, size)| blob.get(start..start.checked_add(size)?))
            .ok_or_else(|| failure("the descriptor area is outside the vbmeta blob".into()))?;
        let slices = split(region).ok_or_else(|| failure("a descriptor is truncated".into()))?;
        if slices.len() != info.descriptors.len() {
            return Err(failure(format!(
                "{} encoded descriptors, the parser found {}",
                slices.len(),
                info.descriptors.len()
            )));
        }
        Ok(info
            .descriptors
            .into_iter()
            .zip(slices)
            .map(|(info, bytes)| Self {
                info,
                bytes: bytes.to_vec(),
            })
            .collect())
    }

    pub fn chain_partition(
        partition_name: &str,
        rollback_index_location: u32,
        public_key: &[u8],
        flags: u32,
    ) -> Self {
        let unpadded = CHAIN_PARTITION_FIXED_BYTES + partition_name.len() + public_key.len();
        let total = unpadded.next_multiple_of(DESCRIPTOR_ALIGNMENT);
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&CHAIN_PARTITION_TAG.to_be_bytes());
        bytes.extend_from_slice(&((total - DESCRIPTOR_HEADER_BYTES) as u64).to_be_bytes());
        bytes.extend_from_slice(&rollback_index_location.to_be_bytes());
        bytes.extend_from_slice(&(partition_name.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&(public_key.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&flags.to_be_bytes());
        bytes.resize(CHAIN_PARTITION_FIXED_BYTES, 0);
        bytes.extend_from_slice(partition_name.as_bytes());
        bytes.extend_from_slice(public_key);
        bytes.resize(total, 0);
        let info = DescriptorInfo::ChainPartition {
            rollback_index_location,
            partition_name: partition_name.to_string(),
            public_key: public_key.to_vec(),
            flags,
        };
        Self { info, bytes }
    }
}

fn split(region: &[u8]) -> Option<Vec<&[u8]>> {
    let mut slices = Vec::new();
    let mut position = 0usize;
    while position < region.len() {
        let length_field = region.get(position + 8..position + DESCRIPTOR_HEADER_BYTES)?;
        let following = u64::from_be_bytes(length_field.try_into().ok()?);
        let end = position
            .checked_add(DESCRIPTOR_HEADER_BYTES)?
            .checked_add(usize::try_from(following).ok()?)?;
        slices.push(region.get(position..end)?);
        position = end;
    }
    Some(slices)
}
