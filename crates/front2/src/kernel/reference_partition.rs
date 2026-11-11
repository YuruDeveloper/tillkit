use crate::device_shell::DeviceShell;
use crate::error::{Front2Error, Result};
use crate::kernel::hash_descriptor::HashDescriptor;
use crate::partition_target::PartitionTarget;
use crate::profile;
use crate::root_access::RootAccess;
use serde::{Deserialize, Serialize};

pub const REFERENCE_BASES: [&str; 4] = ["vendor_boot", "dtbo", "init_boot", "recovery"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencePartition {
    pub base: String,
    pub label: String,
    pub bytes: u64,
    pub sha256: String,
    pub descriptor_image_bytes: u64,
    pub descriptor_digest: String,
}

impl ReferencePartition {
    pub fn read(
        shell: &mut dyn DeviceShell,
        root: RootAccess,
        slot_suffix: &str,
        descriptor: &HashDescriptor,
    ) -> Result<Self> {
        let label = profile::slot_label(&descriptor.partition, slot_suffix);
        let target = PartitionTarget::resolve(shell, root, &label)?;
        let before = target.sha256(shell, root)?;
        let salted = descriptor.device_digest(shell, root, &target)?;
        let after = target.sha256(shell, root)?;
        if before != after || salted != descriptor.digest {
            return Err(Front2Error::Verification(format!(
                "{label}: the top vbmeta records digest {} over {} bytes, the device computed {salted}; partition SHA-256 {before} before and {after} after",
                descriptor.digest, descriptor.image_bytes
            )));
        }
        Ok(Self {
            base: descriptor.partition.clone(),
            label,
            bytes: target.bytes,
            sha256: after,
            descriptor_image_bytes: descriptor.image_bytes,
            descriptor_digest: descriptor.digest.clone(),
        })
    }

    pub fn require_unchanged(&self, shell: &mut dyn DeviceShell, root: RootAccess) -> Result<()> {
        let target = PartitionTarget::resolve(shell, root, &self.label)?;
        let current = target.sha256(shell, root)?;
        if target.bytes != self.bytes || current != self.sha256 {
            return Err(Front2Error::Plan(format!(
                "{} holds {} bytes with SHA-256 {current}, the kernel set records {} bytes with SHA-256 {}; read the current kernel set again",
                self.label, target.bytes, self.bytes, self.sha256
            )));
        }
        Ok(())
    }
}
