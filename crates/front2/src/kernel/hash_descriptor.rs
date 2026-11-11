use crate::device_shell::DeviceShell;
use crate::digest::{hex, is_sha256_hex};
use crate::error::{Front2Error, Result};
use crate::partition_target::PartitionTarget;
use crate::root_access::{RootAccess, quote};
use avbtool_rs::info::DescriptorInfo;
use sha2::{Digest, Sha256};
use std::path::Path;

const HASH_ALGORITHM: &str = "sha256";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashDescriptor {
    pub partition: String,
    pub image_bytes: u64,
    pub salt: Vec<u8>,
    pub digest: String,
}

impl HashDescriptor {
    pub fn read_all(image: &Path) -> Result<Vec<Self>> {
        let info = avbtool_rs::image::inspect_avb_image(image)
            .map_err(|error| Front2Error::Image(format!("inspect {}: {error}", image.display())))?;
        info.descriptors
            .iter()
            .filter_map(from_descriptor)
            .collect()
    }

    pub fn find<'a>(descriptors: &'a [Self], partition: &str) -> Result<&'a Self> {
        let mut matches = descriptors
            .iter()
            .filter(|descriptor| descriptor.partition == partition);
        let found = matches.next().ok_or_else(|| {
            Front2Error::Image(format!("there is no hash descriptor for {partition}"))
        })?;
        if matches.next().is_some() {
            return Err(Front2Error::Image(format!(
                "there is more than one hash descriptor for {partition}"
            )));
        }
        Ok(found)
    }

    pub fn digest_of(&self, image: &[u8]) -> Result<String> {
        let covered = usize::try_from(self.image_bytes)
            .ok()
            .and_then(|bytes| image.get(..bytes))
            .ok_or_else(|| {
                Front2Error::Verification(format!(
                    "the {} hash descriptor covers {} bytes, the image has {} bytes",
                    self.partition,
                    self.image_bytes,
                    image.len()
                ))
            })?;
        let mut hasher = Sha256::new();
        hasher.update(&self.salt);
        hasher.update(covered);
        Ok(hex(&hasher.finalize()))
    }

    pub fn device_command(&self, by_name: &str) -> String {
        let salt: String = self
            .salt
            .iter()
            .map(|byte| format!("\\{byte:03o}"))
            .collect();
        format!(
            "{{ printf {}; head -c {} {}; }} | sha256sum",
            quote(&salt),
            self.image_bytes,
            quote(by_name)
        )
    }

    pub fn device_digest(
        &self,
        shell: &mut dyn DeviceShell,
        root: RootAccess,
        target: &PartitionTarget,
    ) -> Result<String> {
        if self.image_bytes > target.bytes {
            return Err(Front2Error::Verification(format!(
                "the {} hash descriptor covers {} bytes, {} has {} bytes",
                self.partition, self.image_bytes, target.label, target.bytes
            )));
        }
        let line = shell.run_checked(&root.command(&self.device_command(&target.by_name)))?;
        let digest = line.split_whitespace().next().unwrap_or_default();
        if !is_sha256_hex(digest) {
            return Err(Front2Error::Verification(format!(
                "the salted digest of {} returned {line:?}",
                target.label
            )));
        }
        Ok(digest.to_string())
    }
}

fn from_descriptor(descriptor: &DescriptorInfo) -> Option<Result<HashDescriptor>> {
    let DescriptorInfo::Hash {
        image_size,
        hash_algorithm,
        partition_name,
        salt,
        digest,
        ..
    } = descriptor
    else {
        return None;
    };
    if hash_algorithm != HASH_ALGORITHM {
        return Some(Err(Front2Error::Image(format!(
            "the {partition_name} hash descriptor uses {hash_algorithm}, expected {HASH_ALGORITHM}"
        ))));
    }
    Some(Ok(HashDescriptor {
        partition: partition_name.clone(),
        image_bytes: *image_size,
        salt: salt.clone(),
        digest: hex(digest),
    }))
}
