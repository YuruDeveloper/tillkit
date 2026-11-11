use crate::error::{Front2Error, Result};
use crate::kernel::hash_descriptor::HashDescriptor;
use crate::kernel::kernel_set::BOOT_BASE;
use crate::kernel::reference_partition::REFERENCE_BASES;
use crate::profile;
use avbtool_rs::info::DescriptorInfo;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelAvbSummary {
    pub top_algorithm: String,
    pub top_key_sha1: String,
    pub top_rollback_index: u64,
    pub top_rollback_index_location: u32,
    pub top_flags: u32,
    pub top_hash_partitions: Vec<String>,
    pub top_chain_partitions: Vec<String>,
    pub boot_digest_in_top: String,
    pub boot_footer_digest: String,
    pub boot_footer_algorithm: String,
    pub boot_footer_rollback_index: u64,
    pub boot_payload_bytes: u64,
}

impl KernelAvbSummary {
    pub fn inspect(boot: &Path, vbmeta: &Path) -> Result<Self> {
        let top = inspect(vbmeta)?;
        let footer = inspect(boot)?;
        let payload = footer
            .footer
            .as_ref()
            .ok_or_else(|| Front2Error::Image(format!("{} has no AVB footer", boot.display())))?;
        let top_hashes = HashDescriptor::read_all(vbmeta)?;
        let footer_hashes = HashDescriptor::read_all(boot)?;
        let mut top_hash_partitions: Vec<String> = top_hashes
            .iter()
            .map(|descriptor| descriptor.partition.clone())
            .collect();
        top_hash_partitions.sort();
        Ok(Self {
            top_algorithm: top.algorithm_name.clone(),
            top_key_sha1: top.public_key_sha1.clone().unwrap_or_default(),
            top_rollback_index: top.header.rollback_index,
            top_rollback_index_location: top.header.rollback_index_location,
            top_flags: top.header.flags,
            top_hash_partitions,
            top_chain_partitions: top.descriptors.iter().filter_map(chain_name).collect(),
            boot_digest_in_top: HashDescriptor::find(&top_hashes, BOOT_BASE)?.digest.clone(),
            boot_footer_digest: HashDescriptor::find(&footer_hashes, BOOT_BASE)?
                .digest
                .clone(),
            boot_footer_algorithm: footer.algorithm_name.clone(),
            boot_footer_rollback_index: footer.header.rollback_index,
            boot_payload_bytes: payload.original_image_size,
        })
    }

    pub fn boot_is_consistent(&self) -> bool {
        self.boot_digest_in_top == self.boot_footer_digest
    }

    pub fn require_front2_trust(&self) -> Result<()> {
        if self.top_algorithm != profile::TOP_VBMETA_ALGORITHM {
            return Err(Front2Error::Verification(format!(
                "top vbmeta algorithm is {}, expected {}",
                self.top_algorithm,
                profile::TOP_VBMETA_ALGORITHM
            )));
        }
        let key_spec = ltbox_patch::key_map::key_spec_for_pubkey(Some(&self.top_key_sha1));
        if key_spec != Some(profile::TOP_VBMETA_KEY_SPEC) {
            return Err(Front2Error::Verification(format!(
                "top vbmeta key SHA-1 {:?} is not the {} key",
                self.top_key_sha1,
                profile::TOP_VBMETA_KEY_SPEC
            )));
        }
        if self.top_flags != 0 {
            return Err(Front2Error::Verification(format!(
                "top vbmeta flags are {}, expected 0",
                self.top_flags
            )));
        }
        if self.top_hash_partitions != measured_hash_partitions() {
            return Err(Front2Error::Verification(format!(
                "top vbmeta carries hash descriptors for {:?}, expected {:?}",
                self.top_hash_partitions,
                measured_hash_partitions()
            )));
        }
        if !self.boot_is_consistent() {
            return Err(Front2Error::Verification(
                "top vbmeta does not carry the boot footer digest".into(),
            ));
        }
        Ok(())
    }
}

pub fn measured_hash_partitions() -> Vec<String> {
    let mut names: Vec<String> = REFERENCE_BASES
        .iter()
        .chain([&BOOT_BASE])
        .map(|name| name.to_string())
        .collect();
    names.sort();
    names
}

fn inspect(path: &Path) -> Result<avbtool_rs::info::AvbImageInfo> {
    avbtool_rs::image::inspect_avb_image(path)
        .map_err(|error| Front2Error::Image(format!("inspect {}: {error}", path.display())))
}

fn chain_name(descriptor: &DescriptorInfo) -> Option<String> {
    match descriptor {
        DescriptorInfo::ChainPartition { partition_name, .. } => Some(partition_name.clone()),
        _ => None,
    }
}
