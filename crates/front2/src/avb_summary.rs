use crate::device_profile::DeviceProfile;
use crate::digest::hex;
use crate::error::{Front2Error, Result};
use crate::profile;
use avbtool_rs::info::DescriptorInfo;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvbSummary {
    pub top_algorithm: String,
    pub top_key_sha1: String,
    pub top_rollback_index: u64,
    pub top_rollback_index_location: u32,
    pub top_flags: u32,
    pub top_hash_partitions: Vec<String>,
    pub top_chain_partitions: Vec<String>,
    pub init_boot_digest_in_top: String,
    pub init_boot_footer_digest: String,
    pub init_boot_footer_rollback_index: u64,
    pub init_boot_payload_bytes: u64,
}

impl AvbSummary {
    pub fn inspect(init_boot: &Path, vbmeta: &Path) -> Result<Self> {
        let top = inspect(vbmeta)?;
        let footer = inspect(init_boot)?;
        let payload = footer.footer.as_ref().ok_or_else(|| {
            Front2Error::Image(format!("{} has no AVB footer", init_boot.display()))
        })?;
        let top_hash_partitions = top.descriptors.iter().filter_map(hash_name).collect();
        let top_chain_partitions = top.descriptors.iter().filter_map(chain_name).collect();
        Ok(Self {
            top_algorithm: top.algorithm_name.clone(),
            top_key_sha1: top.public_key_sha1.clone().unwrap_or_default(),
            top_rollback_index: top.header.rollback_index,
            top_rollback_index_location: top.header.rollback_index_location,
            top_flags: top.header.flags,
            top_hash_partitions,
            top_chain_partitions,
            init_boot_digest_in_top: hash_digest(&top.descriptors, vbmeta)?,
            init_boot_footer_digest: hash_digest(&footer.descriptors, init_boot)?,
            init_boot_footer_rollback_index: footer.header.rollback_index,
            init_boot_payload_bytes: payload.original_image_size,
        })
    }

    pub fn pair_is_consistent(&self) -> bool {
        self.init_boot_digest_in_top == self.init_boot_footer_digest
    }

    pub fn require_trust(&self, profile: &DeviceProfile) -> Result<()> {
        if self.top_algorithm != profile.top_vbmeta_algorithm {
            return Err(Front2Error::Verification(format!(
                "top vbmeta algorithm is {}, expected {}",
                self.top_algorithm, profile.top_vbmeta_algorithm
            )));
        }
        let key_spec = ltbox_patch::key_map::key_spec_for_pubkey(Some(&self.top_key_sha1));
        if key_spec != Some(profile.top_vbmeta_key_spec) {
            return Err(Front2Error::Verification(format!(
                "top vbmeta key SHA-1 {:?} is not the {} key",
                self.top_key_sha1, profile.top_vbmeta_key_spec
            )));
        }
        if self.top_flags != 0 {
            return Err(Front2Error::Verification(format!(
                "top vbmeta flags are {}, expected 0",
                self.top_flags
            )));
        }
        if !self.pair_is_consistent() {
            return Err(Front2Error::Verification(
                "top vbmeta does not carry the init_boot footer digest".into(),
            ));
        }
        Ok(())
    }
}

fn inspect(path: &Path) -> Result<avbtool_rs::info::AvbImageInfo> {
    avbtool_rs::image::inspect_avb_image(path)
        .map_err(|error| Front2Error::Image(format!("inspect {}: {error}", path.display())))
}

fn hash_name(descriptor: &DescriptorInfo) -> Option<String> {
    match descriptor {
        DescriptorInfo::Hash { partition_name, .. } => Some(partition_name.clone()),
        _ => None,
    }
}

fn chain_name(descriptor: &DescriptorInfo) -> Option<String> {
    match descriptor {
        DescriptorInfo::ChainPartition { partition_name, .. } => Some(partition_name.clone()),
        _ => None,
    }
}

fn hash_digest(descriptors: &[DescriptorInfo], source: &Path) -> Result<String> {
    let mut digests = descriptors
        .iter()
        .filter_map(|descriptor| match descriptor {
            DescriptorInfo::Hash {
                partition_name,
                digest,
                ..
            } if partition_name == profile::INIT_BOOT_BASE => Some(hex(digest)),
            _ => None,
        });
    let digest = digests.next().ok_or_else(|| {
        Front2Error::Image(format!(
            "{} has no hash descriptor for {}",
            source.display(),
            profile::INIT_BOOT_BASE
        ))
    })?;
    if digests.next().is_some() {
        return Err(Front2Error::Image(format!(
            "{} has more than one hash descriptor for {}",
            source.display(),
            profile::INIT_BOOT_BASE
        )));
    }
    Ok(digest)
}
